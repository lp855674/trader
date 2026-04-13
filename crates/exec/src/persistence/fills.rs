use std::collections::HashSet;

use domain::{InstrumentId, Side};

use crate::core::position::FillRecord;

pub struct FillRepository {
    pub fills: Vec<FillRecord>,
    seen_ids: HashSet<String>,
}

impl FillRepository {
    pub fn new() -> Self {
        Self {
            fills: Vec::new(),
            seen_ids: HashSet::new(),
        }
    }

    fn dedup_key(fill: &FillRecord) -> String {
        format!("{}_{}", fill.order_id, fill.ts_ms)
    }

    pub fn insert(&mut self, fill: FillRecord) -> bool {
        let key = Self::dedup_key(&fill);
        if self.seen_ids.contains(&key) {
            return false;
        }
        self.seen_ids.insert(key);
        self.fills.push(fill);
        true
    }

    pub fn find_by_order(&self, order_id: &str) -> Vec<&FillRecord> {
        self.fills
            .iter()
            .filter(|f| f.order_id == order_id)
            .collect()
    }

    pub fn find_by_instrument(&self, instrument: &InstrumentId) -> Vec<&FillRecord> {
        self.fills
            .iter()
            .filter(|f| &f.instrument == instrument)
            .collect()
    }

    pub fn total_qty(&self, instrument: &InstrumentId, side: Side) -> f64 {
        self.fills
            .iter()
            .filter(|f| &f.instrument == instrument && f.side == side)
            .map(|f| f.qty)
            .sum()
    }

    pub fn count(&self) -> usize {
        self.fills.len()
    }
}

impl Default for FillRepository {
    fn default() -> Self {
        Self::new()
    }
}

impl FillRepository {
    /// Batch insert fills, returns count of newly inserted (non-duplicate) fills.
    pub fn insert_batch(&mut self, fills: Vec<FillRecord>) -> usize {
        fills.into_iter().filter(|f| self.insert(f.clone())).count()
    }

    /// Data quality check: flag fills with zero or negative price/qty.
    pub fn quality_check(&self) -> Vec<String> {
        self.fills
            .iter()
            .filter(|f| f.price <= 0.0 || f.qty <= 0.0)
            .map(|f| {
                format!(
                    "fill {} has invalid price={} qty={}",
                    f.order_id, f.price, f.qty
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use domain::{InstrumentId, Side, Venue};

    use super::*;

    fn btc() -> InstrumentId {
        InstrumentId::new(Venue::Crypto, "BTC-USD")
    }

    fn fill(order_id: &str, ts_ms: i64) -> FillRecord {
        FillRecord {
            order_id: order_id.to_string(),
            instrument: btc(),
            side: Side::Buy,
            qty: 1.0,
            price: 100.0,
            commission: 0.5,
            ts_ms,
        }
    }

    #[test]
    fn insert_and_dedup() {
        let mut repo = FillRepository::new();
        let f = fill("o1", 1000);
        assert!(repo.insert(f.clone()));
        assert!(!repo.insert(f)); // duplicate
        assert_eq!(repo.count(), 1);
    }

    #[test]
    fn find_by_order() {
        let mut repo = FillRepository::new();
        repo.insert(fill("o1", 1000));
        repo.insert(fill("o2", 2000));
        let found = repo.find_by_order("o1");
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn total_qty() {
        let mut repo = FillRepository::new();
        repo.insert(fill("o1", 1000));
        repo.insert(fill("o2", 2000));
        assert!((repo.total_qty(&btc(), Side::Buy) - 2.0).abs() < 1e-9);
    }
}
