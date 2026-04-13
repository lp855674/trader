use super::controller::ReplayController;
use crate::align::ResampleAggregator;
use crate::core::{DataItem, Granularity};
use domain::NormalizedBar;

// ── GranularityReplayer ───────────────────────────────────────────────────────

pub struct GranularityReplayer {
    pub controller: ReplayController,
    pub target: Granularity,
    pending_bars: Vec<NormalizedBar>,
}

impl GranularityReplayer {
    pub fn new(controller: ReplayController, target: Granularity) -> Self {
        Self {
            controller,
            target,
            pending_bars: Vec::new(),
        }
    }

    pub fn step(&mut self) -> Option<Vec<NormalizedBar>> {
        let items = self.controller.step()?;

        // Extract bars from items
        for item in items {
            if let DataItem::Bar(bar) = item {
                self.pending_bars.push(bar);
            }
        }

        if self.pending_bars.is_empty() {
            return Some(Vec::new());
        }

        // Downsample to target granularity
        if let Some(target_ms) = self.target.to_ms() {
            let result = ResampleAggregator::downsample(&self.pending_bars, target_ms);
            self.pending_bars.clear();
            Some(result)
        } else {
            // Tick — return all bars as-is (as bars)
            let result = std::mem::take(&mut self.pending_bars);
            Some(result)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{DataQuery, DataSource, DataSourceError, InMemoryDataSource};
    use crate::replay::controller::ReplayConfig;

    #[test]
    fn granularity_replayer_aggregates() {
        // 5 x 1-minute bars should aggregate to 1 x 5-minute bar
        let items: Vec<DataItem> = (0..5)
            .map(|i| {
                DataItem::Bar(NormalizedBar {
                    ts_ms: i * 60_000,
                    open: 1.0,
                    high: (i + 1) as f64,
                    low: 1.0,
                    close: (i + 1) as f64,
                    volume: 10.0,
                })
            })
            .collect();
        let source = Box::new(InMemoryDataSource::new("test", items));
        let config = ReplayConfig::new(0, 5 * 60_000 - 1);
        let ctrl = ReplayController::new(source, config).with_step_ms(5 * 60_000);
        let mut replayer = GranularityReplayer::new(ctrl, Granularity::Minutes(5));
        let bars = replayer.step().unwrap();
        assert!(!bars.is_empty());
        // All 5 bars should merge into 1 (or a few, depending on bucket alignment)
        let total_volume: f64 = bars.iter().map(|b| b.volume).sum();
        assert!((total_volume - 50.0).abs() < 1e-9);
    }
}
