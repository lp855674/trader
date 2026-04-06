// Position Manager — tracks open positions, computes PnL, enforces exposure
// limits.

use std::collections::HashMap;

use domain::{InstrumentId, Side};
use thiserror::Error;

// ─── PositionError ────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum PositionError {
    #[error("Maximum number of positions reached")]
    MaxPositionsReached,

    #[error("Opposite position already exists for {0}")]
    OppositePositionExists(InstrumentId),

    #[error("Insufficient position to reduce")]
    InsufficientPosition,

    #[error("Limit exceeded: {0}")]
    LimitExceeded(String),

    #[error("Position not found for {0}")]
    NotFound(InstrumentId),
}

// ─── PositionEntry ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PositionEntry {
    pub instrument: InstrumentId,
    pub side: Side,
    pub quantity: f64,
    pub avg_entry_price: f64,
    pub opened_ts_ms: i64,
    pub last_updated_ts: i64,
    pub realised_pnl: f64,
}

impl PositionEntry {
    pub fn new(
        instrument: InstrumentId,
        side: Side,
        quantity: f64,
        price: f64,
        ts_ms: i64,
    ) -> Self {
        Self {
            instrument,
            side,
            quantity,
            avg_entry_price: price,
            opened_ts_ms: ts_ms,
            last_updated_ts: ts_ms,
            realised_pnl: 0.0,
        }
    }

    /// Unrealised PnL: sign depends on side.
    pub fn unrealised_pnl(&self, current_price: f64) -> f64 {
        let sign = match self.side {
            Side::Buy => 1.0,
            Side::Sell => -1.0,
        };
        (current_price - self.avg_entry_price) * self.quantity * sign
    }

    /// Notional value = qty * current_price.
    pub fn notional_value(&self, current_price: f64) -> f64 {
        self.quantity * current_price
    }

    /// Update position via VWAP when adding to an existing position.
    pub fn update_entry(&mut self, additional_qty: f64, price: f64, ts_ms: i64) {
        let total_qty = self.quantity + additional_qty;
        if total_qty > 0.0 {
            self.avg_entry_price =
                (self.quantity * self.avg_entry_price + additional_qty * price) / total_qty;
        }
        self.quantity = total_qty;
        self.last_updated_ts = ts_ms;
    }
}

// ─── ExposureLimit ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct ExposureLimit {
    pub max_notional: Option<f64>,
    pub max_quantity: Option<f64>,
    /// Maximum fraction of total portfolio value (0.0–1.0).
    pub max_concentration: Option<f64>,
}

impl ExposureLimit {
    pub fn check(
        &self,
        entry: &PositionEntry,
        current_price: f64,
        total_portfolio_value: f64,
    ) -> Result<(), PositionError> {
        if let Some(max_n) = self.max_notional {
            let notional = entry.notional_value(current_price);
            if notional > max_n {
                return Err(PositionError::LimitExceeded(format!(
                    "notional {notional:.2} exceeds max {max_n:.2}"
                )));
            }
        }
        if let Some(max_q) = self.max_quantity {
            if entry.quantity > max_q {
                return Err(PositionError::LimitExceeded(format!(
                    "quantity {} exceeds max {max_q}",
                    entry.quantity
                )));
            }
        }
        if let Some(max_conc) = self.max_concentration {
            if total_portfolio_value > 0.0 {
                let conc = entry.notional_value(current_price) / total_portfolio_value;
                if conc > max_conc {
                    return Err(PositionError::LimitExceeded(format!(
                        "concentration {conc:.4} exceeds max {max_conc:.4}"
                    )));
                }
            }
        }
        Ok(())
    }
}

// ─── PositionManager ─────────────────────────────────────────────────────────

pub struct PositionManager {
    pub positions: HashMap<InstrumentId, PositionEntry>,
    pub limits: HashMap<InstrumentId, ExposureLimit>,
    pub global_max_positions: usize,
}

impl PositionManager {
    pub fn new(max_positions: usize) -> Self {
        Self {
            positions: HashMap::new(),
            limits: HashMap::new(),
            global_max_positions: max_positions,
        }
    }

    pub fn set_limit(&mut self, instrument: InstrumentId, limit: ExposureLimit) {
        self.limits.insert(instrument, limit);
    }

    /// Open a new position or add to an existing same-side position.
    pub fn open_or_add(
        &mut self,
        instrument: InstrumentId,
        side: Side,
        qty: f64,
        price: f64,
        ts_ms: i64,
    ) -> Result<(), PositionError> {
        if let Some(existing) = self.positions.get_mut(&instrument) {
            // Must be same side
            if std::mem::discriminant(&existing.side) != std::mem::discriminant(&side) {
                return Err(PositionError::OppositePositionExists(instrument));
            }
            existing.update_entry(qty, price, ts_ms);
        } else {
            // New position — check capacity
            if self.positions.len() >= self.global_max_positions {
                return Err(PositionError::MaxPositionsReached);
            }
            self.positions
                .insert(instrument.clone(), PositionEntry::new(instrument, side, qty, price, ts_ms));
        }
        Ok(())
    }

    /// Reduce or fully close a position.  Returns the realised PnL for the
    /// closed quantity.
    pub fn reduce_or_close(
        &mut self,
        instrument: &InstrumentId,
        qty: f64,
        price: f64,
        ts_ms: i64,
    ) -> Result<f64, PositionError> {
        let entry = self
            .positions
            .get_mut(instrument)
            .ok_or_else(|| PositionError::NotFound(instrument.clone()))?;

        if qty > entry.quantity + 1e-12 {
            return Err(PositionError::InsufficientPosition);
        }

        let sign = match entry.side {
            Side::Buy => 1.0,
            Side::Sell => -1.0,
        };
        let realised = (price - entry.avg_entry_price) * qty * sign;
        entry.realised_pnl += realised;
        entry.quantity -= qty;
        entry.last_updated_ts = ts_ms;

        if entry.quantity <= 1e-12 {
            self.positions.remove(instrument);
        }

        Ok(realised)
    }

    /// Total notional exposure across all open positions.
    pub fn total_exposure(&self, prices: &HashMap<InstrumentId, f64>) -> f64 {
        self.positions
            .iter()
            .map(|(inst, entry)| {
                let price = prices.get(inst).copied().unwrap_or(entry.avg_entry_price);
                entry.notional_value(price)
            })
            .sum()
    }

    /// Check per-instrument limits for adding additional_qty at price.
    pub fn check_limits(
        &self,
        instrument: &InstrumentId,
        additional_qty: f64,
        price: f64,
        prices: &HashMap<InstrumentId, f64>,
    ) -> Result<(), PositionError> {
        if let Some(limit) = self.limits.get(instrument) {
            let existing_qty = self
                .positions
                .get(instrument)
                .map(|e| e.quantity)
                .unwrap_or(0.0);
            let hypothetical = PositionEntry {
                instrument: instrument.clone(),
                side: Side::Buy,
                quantity: existing_qty + additional_qty,
                avg_entry_price: price,
                opened_ts_ms: 0,
                last_updated_ts: 0,
                realised_pnl: 0.0,
            };
            let portfolio_value = self.total_exposure(prices);
            limit.check(&hypothetical, price, portfolio_value)?;
        }
        Ok(())
    }

    pub fn get(&self, instrument: &InstrumentId) -> Option<&PositionEntry> {
        self.positions.get(instrument)
    }

    pub fn all(&self) -> &HashMap<InstrumentId, PositionEntry> {
        &self.positions
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{InstrumentId, Venue};

    fn btc() -> InstrumentId {
        InstrumentId::new(Venue::Crypto, "BTC")
    }

    fn eth() -> InstrumentId {
        InstrumentId::new(Venue::Crypto, "ETH")
    }

    #[test]
    fn open_new_position() {
        let mut pm = PositionManager::new(10);
        pm.open_or_add(btc(), Side::Buy, 1.0, 100.0, 0).unwrap();
        let entry = pm.get(&btc()).unwrap();
        assert!((entry.quantity - 1.0).abs() < 1e-9);
        assert!((entry.avg_entry_price - 100.0).abs() < 1e-9);
    }

    #[test]
    fn add_to_existing_vwap() {
        let mut pm = PositionManager::new(10);
        pm.open_or_add(btc(), Side::Buy, 2.0, 100.0, 0).unwrap();
        pm.open_or_add(btc(), Side::Buy, 1.0, 130.0, 1).unwrap();
        let entry = pm.get(&btc()).unwrap();
        assert!((entry.quantity - 3.0).abs() < 1e-9);
        // VWAP = (2*100 + 1*130) / 3 = 110
        assert!((entry.avg_entry_price - 110.0).abs() < 1e-9);
    }

    #[test]
    fn reduce_position() {
        let mut pm = PositionManager::new(10);
        pm.open_or_add(btc(), Side::Buy, 2.0, 100.0, 0).unwrap();
        let pnl = pm.reduce_or_close(&btc(), 1.0, 120.0, 1).unwrap();
        // pnl = (120-100)*1*1 = 20
        assert!((pnl - 20.0).abs() < 1e-9);
        let entry = pm.get(&btc()).unwrap();
        assert!((entry.quantity - 1.0).abs() < 1e-9);
    }

    #[test]
    fn close_position_removes_entry() {
        let mut pm = PositionManager::new(10);
        pm.open_or_add(btc(), Side::Buy, 1.0, 100.0, 0).unwrap();
        pm.reduce_or_close(&btc(), 1.0, 200.0, 1).unwrap();
        assert!(pm.get(&btc()).is_none());
    }

    #[test]
    fn opposite_side_rejected() {
        let mut pm = PositionManager::new(10);
        pm.open_or_add(btc(), Side::Buy, 1.0, 100.0, 0).unwrap();
        let err = pm.open_or_add(btc(), Side::Sell, 1.0, 100.0, 1);
        assert!(matches!(err, Err(PositionError::OppositePositionExists(_))));
    }

    #[test]
    fn max_positions_enforced() {
        let mut pm = PositionManager::new(1);
        pm.open_or_add(btc(), Side::Buy, 1.0, 100.0, 0).unwrap();
        let err = pm.open_or_add(eth(), Side::Buy, 1.0, 100.0, 0);
        assert!(matches!(err, Err(PositionError::MaxPositionsReached)));
    }

    #[test]
    fn exposure_limit_max_notional() {
        let limit = ExposureLimit {
            max_notional: Some(500.0),
            ..Default::default()
        };
        let entry = PositionEntry::new(btc(), Side::Buy, 10.0, 100.0, 0);
        // notional = 10 * 100 = 1000, exceeds 500
        let result = limit.check(&entry, 100.0, 100_000.0);
        assert!(matches!(result, Err(PositionError::LimitExceeded(_))));
    }

    #[test]
    fn reduce_insufficient_errors() {
        let mut pm = PositionManager::new(10);
        pm.open_or_add(btc(), Side::Buy, 1.0, 100.0, 0).unwrap();
        let err = pm.reduce_or_close(&btc(), 5.0, 100.0, 1);
        assert!(matches!(err, Err(PositionError::InsufficientPosition)));
    }

    #[test]
    fn unrealised_pnl_long() {
        let entry = PositionEntry::new(btc(), Side::Buy, 2.0, 100.0, 0);
        let pnl = entry.unrealised_pnl(150.0);
        assert!((pnl - 100.0).abs() < 1e-9);
    }

    #[test]
    fn unrealised_pnl_short() {
        let entry = PositionEntry::new(btc(), Side::Sell, 1.0, 100.0, 0);
        // short: (current - avg) * qty * -1
        let pnl = entry.unrealised_pnl(80.0);
        // (80-100)*1*(-1) = 20
        assert!((pnl - 20.0).abs() < 1e-9);
    }
}
