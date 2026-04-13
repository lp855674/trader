use std::collections::HashMap;

use domain::InstrumentId;

use crate::core::position::{ExecPosition, ExecPositionManager};

pub struct PositionRepository {
    pub snapshots: Vec<(i64, HashMap<InstrumentId, ExecPosition>)>,
    pub max_snapshots: usize,
}

impl PositionRepository {
    pub fn new(max_snapshots: usize) -> Self {
        Self {
            snapshots: Vec::new(),
            max_snapshots,
        }
    }

    pub fn take_snapshot(&mut self, manager: &ExecPositionManager, ts_ms: i64) {
        if self.snapshots.len() >= self.max_snapshots {
            self.snapshots.remove(0);
        }
        self.snapshots.push((ts_ms, manager.positions.clone()));
    }

    pub fn latest(&self) -> Option<&HashMap<InstrumentId, ExecPosition>> {
        self.snapshots.last().map(|(_, positions)| positions)
    }

    pub fn history_since(&self, since_ms: i64) -> Vec<&(i64, HashMap<InstrumentId, ExecPosition>)> {
        self.snapshots
            .iter()
            .filter(|(ts, _)| *ts >= since_ms)
            .collect()
    }

    /// Aggregation query: total notional exposure across all instruments in latest snapshot.
    pub fn total_notional(&self, prices: &HashMap<InstrumentId, f64>) -> f64 {
        match self.latest() {
            None => 0.0,
            Some(positions) => positions
                .iter()
                .map(|(id, pos)| {
                    let price = prices.get(id).copied().unwrap_or(0.0);
                    pos.net_qty.abs() * price
                })
                .sum(),
        }
    }

    /// Aggregation query: count of open positions (non-zero net qty).
    pub fn open_position_count(&self) -> usize {
        self.latest()
            .map(|p| p.values().filter(|pos| pos.net_qty.abs() > 1e-9).count())
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use domain::{InstrumentId, Side, Venue};

    use super::*;
    use crate::core::position::{ExecPositionManager, FillRecord, TaxLotMethod};

    fn btc() -> InstrumentId {
        InstrumentId::new(Venue::Crypto, "BTC-USD")
    }

    #[test]
    fn snapshot_and_latest() {
        let mut repo = PositionRepository::new(10);
        let mut mgr = ExecPositionManager::new(TaxLotMethod::Fifo);
        mgr.apply_fill(&FillRecord {
            order_id: "o1".to_string(),
            instrument: btc(),
            side: Side::Buy,
            qty: 5.0,
            price: 100.0,
            commission: 0.0,
            ts_ms: 1000,
        });
        repo.take_snapshot(&mgr, 1000);
        let latest = repo.latest().unwrap();
        let pos = latest.get(&btc()).unwrap();
        assert!((pos.net_qty - 5.0).abs() < 1e-9);
    }

    #[test]
    fn history_since_filters() {
        let mut repo = PositionRepository::new(10);
        let mgr = ExecPositionManager::new(TaxLotMethod::Fifo);
        repo.take_snapshot(&mgr, 1000);
        repo.take_snapshot(&mgr, 2000);
        repo.take_snapshot(&mgr, 3000);
        let hist = repo.history_since(2000);
        assert_eq!(hist.len(), 2);
    }

    #[test]
    fn max_snapshots_evicts_oldest() {
        let mut repo = PositionRepository::new(2);
        let mgr = ExecPositionManager::new(TaxLotMethod::Fifo);
        repo.take_snapshot(&mgr, 1000);
        repo.take_snapshot(&mgr, 2000);
        repo.take_snapshot(&mgr, 3000);
        assert_eq!(repo.snapshots.len(), 2);
        assert_eq!(repo.snapshots[0].0, 2000);
    }
}
