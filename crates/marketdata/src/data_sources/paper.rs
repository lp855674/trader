use crate::core::data::{DataItem, DataQuery, DataSource, DataSourceError, Granularity};
use domain::NormalizedBar;

pub struct PaperDataSource {
    instrument: String,
    bars: Vec<NormalizedBar>,
}

impl PaperDataSource {
    pub fn new_random(
        instrument: &str,
        n_bars: usize,
        start_ts_ms: i64,
        interval_ms: u64,
        seed: u64,
    ) -> Self {
        let mut state = seed;
        let mut bars = Vec::with_capacity(n_bars);
        let mut price = 100.0f64;

        for i in 0..n_bars {
            let ts_ms = start_ts_ms + i as i64 * interval_ms as i64;

            // LCG random
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let r1 = (state >> 11) as f64 / (1u64 << 53) as f64;
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let r2 = (state >> 11) as f64 / (1u64 << 53) as f64;
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let r3 = (state >> 11) as f64 / (1u64 << 53) as f64;
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let r4 = (state >> 11) as f64 / (1u64 << 53) as f64;

            let open = price;
            // Small random price change (-1% to +1%)
            let change = (r1 - 0.5) * 0.02;
            let close = (open * (1.0 + change)).max(0.01);

            let high = open.max(close) * (1.0 + r2 * 0.01);
            let low = open.min(close) * (1.0 - r3 * 0.01);
            let volume = 1000.0 * (0.5 + r4);

            bars.push(NormalizedBar {
                ts_ms,
                open,
                high,
                low,
                close,
                volume,
            });

            price = close;
        }

        Self {
            instrument: instrument.to_string(),
            bars,
        }
    }

    pub fn new_from_bars(instrument: &str, bars: Vec<NormalizedBar>) -> Self {
        Self {
            instrument: instrument.to_string(),
            bars,
        }
    }
}

impl DataSource for PaperDataSource {
    fn name(&self) -> &str {
        &self.instrument
    }

    fn query(&self, query: &DataQuery) -> Result<Vec<DataItem>, DataSourceError> {
        let mut result: Vec<DataItem> = self
            .bars
            .iter()
            .filter(|b| b.ts_ms >= query.start_ts_ms && b.ts_ms <= query.end_ts_ms)
            .map(|b| DataItem::Bar(b.clone()))
            .collect();
        if let Some(limit) = query.limit {
            result.truncate(limit);
        }
        Ok(result)
    }

    fn supports_granularity(&self, _g: &Granularity) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paper_source_generates_bars() {
        let source = PaperDataSource::new_random("BTC", 100, 0, 60_000, 42);
        assert_eq!(source.bars.len(), 100);
    }

    #[test]
    fn paper_source_query_filters_range() {
        let source = PaperDataSource::new_random("BTC", 100, 0, 60_000, 42);
        let q = DataQuery::new("BTC", 60_000, 300_000);
        let result = source.query(&q).unwrap();
        assert!(
            result
                .iter()
                .all(|i| i.ts_ms() >= 60_000 && i.ts_ms() <= 300_000)
        );
    }
}
