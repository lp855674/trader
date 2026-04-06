use crate::core::data::{DataItem, DataQuery, DataSource, DataSourceError, Granularity};
use crate::analysis::market_depth::DepthSnapshot;

#[derive(Debug, Clone)]
pub struct OrderBookConfig {
    pub instrument: String,
    pub n_levels: usize,
    pub base_price: f64,
    pub spread_bps: f64,
}

pub struct OrderBookSource {
    config: OrderBookConfig,
    snapshots: Vec<DataItem>,
}

impl OrderBookSource {
    pub fn new(config: OrderBookConfig) -> Self {
        Self {
            config,
            snapshots: Vec::new(),
        }
    }

    pub fn generate_snapshot(&self, ts_ms: i64) -> DataItem {
        let half_spread = self.config.base_price * self.config.spread_bps / 10000.0 / 2.0;
        let best_bid = self.config.base_price - half_spread;
        let best_ask = self.config.base_price + half_spread;

        let tick = self.config.base_price * 0.0001;

        let bids: Vec<(f64, f64)> = (0..self.config.n_levels)
            .map(|i| (best_bid - i as f64 * tick, 100.0 * (i as f64 + 1.0)))
            .collect();

        let asks: Vec<(f64, f64)> = (0..self.config.n_levels)
            .map(|i| (best_ask + i as f64 * tick, 100.0 * (i as f64 + 1.0)))
            .collect();

        DataItem::OrderBook { ts_ms, bids, asks }
    }

    pub fn with_generated_snapshots(mut self, start_ms: i64, n: usize, interval_ms: u64) -> Self {
        self.snapshots = (0..n)
            .map(|i| self.generate_snapshot(start_ms + i as i64 * interval_ms as i64))
            .collect();
        self
    }
}

impl DataSource for OrderBookSource {
    fn name(&self) -> &str {
        &self.config.instrument
    }

    fn query(&self, query: &DataQuery) -> Result<Vec<DataItem>, DataSourceError> {
        let mut result: Vec<DataItem> = self
            .snapshots
            .iter()
            .filter(|i| i.ts_ms() >= query.start_ts_ms && i.ts_ms() <= query.end_ts_ms)
            .cloned()
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

pub fn to_depth_snapshot(item: &DataItem) -> Option<DepthSnapshot> {
    match item {
        DataItem::OrderBook { ts_ms, bids, asks } => Some(DepthSnapshot {
            ts_ms: *ts_ms,
            bids: bids.clone(),
            asks: asks.clone(),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_snapshot_has_correct_levels() {
        let config = OrderBookConfig {
            instrument: "BTC".to_string(),
            n_levels: 5,
            base_price: 50000.0,
            spread_bps: 2.0,
        };
        let source = OrderBookSource::new(config);
        let snap = source.generate_snapshot(0);
        if let DataItem::OrderBook { bids, asks, .. } = &snap {
            assert_eq!(bids.len(), 5);
            assert_eq!(asks.len(), 5);
            // best bid < best ask
            assert!(bids[0].0 < asks[0].0);
        } else {
            panic!("Expected OrderBook");
        }
    }

    #[test]
    fn to_depth_snapshot_converts() {
        let config = OrderBookConfig {
            instrument: "ETH".to_string(),
            n_levels: 3,
            base_price: 2000.0,
            spread_bps: 5.0,
        };
        let source = OrderBookSource::new(config);
        let item = source.generate_snapshot(1000);
        let snap = to_depth_snapshot(&item).unwrap();
        assert_eq!(snap.ts_ms, 1000);
        assert!(!snap.bids.is_empty());
    }
}
