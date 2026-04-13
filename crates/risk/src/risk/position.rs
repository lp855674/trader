use crate::core::{RiskChecker, RiskDecision, RiskError, RiskInput};
use domain::{InstrumentId, Side};
use std::collections::HashMap;

// ── PositionEntry ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PositionEntry {
    pub instrument: InstrumentId,
    pub side: Side,
    pub quantity: f64,
    pub avg_price: f64,
    pub unrealised_pnl: f64,
    pub realised_pnl: f64,
    pub opened_ts_ms: i64,
    /// Peak value for trailing stop
    pub peak_value: f64,
}

impl PositionEntry {
    pub fn new(
        instrument: InstrumentId,
        side: Side,
        quantity: f64,
        price: f64,
        ts_ms: i64,
    ) -> Self {
        let notional = quantity * price;
        Self {
            instrument,
            side,
            quantity,
            avg_price: price,
            unrealised_pnl: 0.0,
            realised_pnl: 0.0,
            opened_ts_ms: ts_ms,
            peak_value: notional,
        }
    }

    pub fn notional_value(&self) -> f64 {
        self.quantity * self.avg_price
    }

    pub fn update_unrealised(&mut self, current_price: f64) {
        self.unrealised_pnl = match self.side {
            Side::Buy => (current_price - self.avg_price) * self.quantity,
            Side::Sell => (self.avg_price - current_price) * self.quantity,
        };
        let current_notional = self.quantity * current_price;
        if current_notional > self.peak_value {
            self.peak_value = current_notional;
        }
    }
}

// ── PnLLimits ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PnLLimits {
    /// Max allowed daily loss (negative number)
    pub daily_loss_limit: f64,
    /// Per-position max loss
    pub position_loss_limit: f64,
    /// Max drawdown from peak (fraction, e.g. 0.10 = 10%)
    pub max_drawdown_pct: f64,
}

// ── StopLossConfig ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct StopLossConfig {
    /// Immediate reject if loss exceeds this fraction
    pub hard_stop_pct: f64,
    /// Trailing stop — stop follows peak price
    pub trailing_stop_pct: f64,
}

impl StopLossConfig {
    /// Returns true if stop is triggered for the given position at current_price
    pub fn check_stop(&self, entry: &PositionEntry, current_price: f64) -> bool {
        let current_notional = entry.quantity * current_price;
        let entry_notional = entry.quantity * entry.avg_price;

        // Hard stop: current loss exceeds hard_stop_pct from entry
        let loss_pct = match entry.side {
            Side::Buy => (entry.avg_price - current_price) / entry.avg_price,
            Side::Sell => (current_price - entry.avg_price) / entry.avg_price,
        };
        if loss_pct >= self.hard_stop_pct {
            return true;
        }

        // Trailing stop: current notional is trailing_stop_pct below peak
        let drawdown_from_peak = if entry.peak_value > 0.0 {
            (entry.peak_value - current_notional) / entry.peak_value
        } else {
            0.0
        };

        // Only apply trailing stop if we are in profit first (peak > entry)
        let _ = entry_notional; // suppress warning
        if entry.peak_value > entry.notional_value() && drawdown_from_peak >= self.trailing_stop_pct
        {
            return true;
        }

        false
    }
}

// ── RiskPositionManager ───────────────────────────────────────────────────

pub struct RiskPositionManager {
    pub positions: HashMap<InstrumentId, PositionEntry>,
    pub pnl_limits: PnLLimits,
    pub stop_loss: StopLossConfig,
    pub daily_pnl: f64,
    pub daily_reset_ts: i64,
}

impl RiskPositionManager {
    pub fn new(pnl_limits: PnLLimits, stop_loss: StopLossConfig) -> Self {
        Self {
            positions: HashMap::new(),
            pnl_limits,
            stop_loss,
            daily_pnl: 0.0,
            daily_reset_ts: 0,
        }
    }

    pub fn update_position(
        &mut self,
        instrument: &InstrumentId,
        side: Side,
        qty: f64,
        price: f64,
        ts_ms: i64,
    ) {
        if let Some(existing) = self.positions.get_mut(instrument) {
            if existing.side == side {
                // Same side: average in
                let total_qty = existing.quantity + qty;
                existing.avg_price =
                    (existing.avg_price * existing.quantity + price * qty) / total_qty;
                existing.quantity = total_qty;
            } else {
                // Opposite side: reduce or flip
                if qty >= existing.quantity {
                    // Close entirely + open opposite if qty > existing
                    let realised = match existing.side {
                        Side::Buy => (price - existing.avg_price) * existing.quantity,
                        Side::Sell => (existing.avg_price - price) * existing.quantity,
                    };
                    self.daily_pnl += realised;
                    let remaining = qty - existing.quantity;
                    if remaining > 0.0 {
                        *existing =
                            PositionEntry::new(instrument.clone(), side, remaining, price, ts_ms);
                    } else {
                        self.positions.remove(instrument);
                        return;
                    }
                } else {
                    // Partial close
                    let realised = match existing.side {
                        Side::Buy => (price - existing.avg_price) * qty,
                        Side::Sell => (existing.avg_price - price) * qty,
                    };
                    self.daily_pnl += realised;
                    existing.quantity -= qty;
                }
            }
        } else {
            self.positions.insert(
                instrument.clone(),
                PositionEntry::new(instrument.clone(), side, qty, price, ts_ms),
            );
        }
    }

    /// Close position fully; returns realised PnL
    pub fn close_position(&mut self, instrument: &InstrumentId, price: f64) -> f64 {
        if let Some(entry) = self.positions.remove(instrument) {
            let pnl = match entry.side {
                Side::Buy => (price - entry.avg_price) * entry.quantity,
                Side::Sell => (entry.avg_price - price) * entry.quantity,
            };
            self.daily_pnl += pnl;
            pnl
        } else {
            0.0
        }
    }

    /// Update unrealised PnL for all positions
    pub fn update_prices(&mut self, prices: &HashMap<InstrumentId, f64>) {
        for (instrument, entry) in &mut self.positions {
            if let Some(&price) = prices.get(instrument) {
                entry.update_unrealised(price);
            }
        }
    }

    /// Returns list of instruments where stop loss is triggered
    pub fn check_stops(&self) -> Vec<InstrumentId> {
        let mut triggered = Vec::new();
        for (instrument, entry) in &self.positions {
            // Use avg_price as proxy for current price if we have unrealised_pnl
            let current_price = if entry.unrealised_pnl != 0.0 {
                match entry.side {
                    Side::Buy => entry.avg_price + entry.unrealised_pnl / entry.quantity,
                    Side::Sell => entry.avg_price - entry.unrealised_pnl / entry.quantity,
                }
            } else {
                entry.avg_price
            };
            if self.stop_loss.check_stop(entry, current_price) {
                triggered.push(instrument.clone());
            }
        }
        triggered
    }

    /// Reset daily PnL at start of new day
    pub fn reset_daily_pnl(&mut self, ts_ms: i64) {
        self.daily_pnl = 0.0;
        self.daily_reset_ts = ts_ms;
    }

    /// Total notional exposure across all positions
    pub fn total_exposure(&self) -> f64 {
        self.positions.values().map(|e| e.notional_value()).sum()
    }
}

// ── PositionRiskChecker ───────────────────────────────────────────────────

pub struct PositionRiskChecker {
    max_open_positions: u32,
    /// Daily PnL limit (negative)
    daily_loss_limit: f64,
}

impl PositionRiskChecker {
    pub fn new(max_open_positions: u32, daily_loss_limit: f64) -> Self {
        Self {
            max_open_positions,
            daily_loss_limit,
        }
    }
}

impl RiskChecker for PositionRiskChecker {
    fn check(&self, input: &RiskInput) -> Result<RiskDecision, RiskError> {
        // Check daily PnL limit
        if input.portfolio.daily_pnl < self.daily_loss_limit {
            return Ok(RiskDecision::Reject {
                reason: format!(
                    "Daily PnL {:.2} below limit {:.2}",
                    input.portfolio.daily_pnl, self.daily_loss_limit
                ),
                risk_score: 100.0,
            });
        }

        // Check open positions limit
        if input.portfolio.open_positions >= self.max_open_positions {
            return Ok(RiskDecision::Reject {
                reason: format!(
                    "Open positions {} at maximum {}",
                    input.portfolio.open_positions, self.max_open_positions
                ),
                risk_score: 80.0,
            });
        }

        Ok(RiskDecision::Approve)
    }

    fn name(&self) -> &str {
        "PositionRiskChecker"
    }

    fn priority(&self) -> u32 {
        30
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{MarketContext, OrderContext, OrderType, PortfolioContext, RiskInput};
    use domain::Venue;

    fn make_pnl_limits() -> PnLLimits {
        PnLLimits {
            daily_loss_limit: -5_000.0,
            position_loss_limit: -1_000.0,
            max_drawdown_pct: 0.10,
        }
    }

    fn make_stop_config() -> StopLossConfig {
        StopLossConfig {
            hard_stop_pct: 0.05,     // 5% loss
            trailing_stop_pct: 0.03, // 3% from peak
        }
    }

    fn btc() -> InstrumentId {
        InstrumentId::new(Venue::Crypto, "BTC-USD")
    }

    #[test]
    fn position_tracking_buy() {
        let mut mgr = RiskPositionManager::new(make_pnl_limits(), make_stop_config());
        mgr.update_position(&btc(), Side::Buy, 2.0, 50_000.0, 1_000);
        let pos = &mgr.positions[&btc()];
        assert_eq!(pos.quantity, 2.0);
        assert_eq!(pos.avg_price, 50_000.0);
    }

    #[test]
    fn position_averaging() {
        let mut mgr = RiskPositionManager::new(make_pnl_limits(), make_stop_config());
        mgr.update_position(&btc(), Side::Buy, 1.0, 50_000.0, 1_000);
        mgr.update_position(&btc(), Side::Buy, 1.0, 52_000.0, 2_000);
        let pos = &mgr.positions[&btc()];
        assert_eq!(pos.quantity, 2.0);
        assert_eq!(pos.avg_price, 51_000.0);
    }

    #[test]
    fn close_position_realises_pnl() {
        let mut mgr = RiskPositionManager::new(make_pnl_limits(), make_stop_config());
        mgr.update_position(&btc(), Side::Buy, 1.0, 50_000.0, 1_000);
        let pnl = mgr.close_position(&btc(), 55_000.0);
        assert!((pnl - 5_000.0).abs() < 0.01);
        assert!(mgr.positions.is_empty());
    }

    #[test]
    fn hard_stop_triggers() {
        let stop = make_stop_config();
        let mut entry = PositionEntry::new(btc(), Side::Buy, 1.0, 50_000.0, 0);
        entry.peak_value = entry.notional_value();
        // 10% loss — exceeds 5% hard stop
        let current_price = 44_000.0;
        assert!(stop.check_stop(&entry, current_price));
    }

    #[test]
    fn hard_stop_not_triggered_within_limit() {
        let stop = make_stop_config();
        let entry = PositionEntry::new(btc(), Side::Buy, 1.0, 50_000.0, 0);
        // Only 2% loss — within 5% hard stop
        let current_price = 49_000.0;
        assert!(!stop.check_stop(&entry, current_price));
    }

    #[test]
    fn trailing_stop_follows_peak() {
        let stop = make_stop_config();
        let mut entry = PositionEntry::new(btc(), Side::Buy, 1.0, 50_000.0, 0);
        // Price rises to 60k — peak should update
        entry.update_unrealised(60_000.0);
        assert_eq!(entry.peak_value, 60_000.0);
        // Price falls back 5% from peak (60k * 0.97 = 58.2k) — trailing stop triggers at >3% from peak
        let current_price = 57_000.0; // 5% below peak → exceeds 3% trailing stop
        assert!(stop.check_stop(&entry, current_price));
    }

    #[test]
    fn trailing_stop_not_triggered_within_limit() {
        let stop = make_stop_config();
        let mut entry = PositionEntry::new(btc(), Side::Buy, 1.0, 50_000.0, 0);
        entry.update_unrealised(60_000.0); // peak = 60k
        // Only 1% below peak — within 3% trailing stop
        let current_price = 59_400.0;
        assert!(!stop.check_stop(&entry, current_price));
    }

    #[test]
    fn daily_pnl_reset() {
        let mut mgr = RiskPositionManager::new(make_pnl_limits(), make_stop_config());
        mgr.daily_pnl = -1_000.0;
        mgr.reset_daily_pnl(86_400_000);
        assert_eq!(mgr.daily_pnl, 0.0);
        assert_eq!(mgr.daily_reset_ts, 86_400_000);
    }

    #[test]
    fn position_risk_checker_rejects_daily_pnl_breach() {
        let checker = PositionRiskChecker::new(10, -5_000.0);
        let input = RiskInput {
            order: OrderContext {
                instrument: btc(),
                side: Side::Buy,
                quantity: 1.0,
                limit_price: Some(50_000.0),
                order_type: OrderType::Limit,
                strategy_id: "test".into(),
                submitted_ts_ms: 0,
            },
            market: MarketContext {
                instrument: btc(),
                mid_price: 50_000.0,
                bid: 49_990.0,
                ask: 50_010.0,
                volume_24h: 1_000_000.0,
                volatility: 0.02,
                ts_ms: 0,
            },
            portfolio: PortfolioContext {
                total_capital: 100_000.0,
                available_capital: 80_000.0,
                total_exposure: 20_000.0,
                open_positions: 2,
                daily_pnl: -6_000.0, // below -5000 limit
                daily_pnl_limit: -5_000.0,
            },
        };
        let result = checker.check(&input).unwrap();
        assert!(matches!(result, RiskDecision::Reject { .. }));
    }

    #[test]
    fn position_risk_checker_rejects_max_positions() {
        let checker = PositionRiskChecker::new(3, -5_000.0);
        let input = RiskInput {
            order: OrderContext {
                instrument: btc(),
                side: Side::Buy,
                quantity: 1.0,
                limit_price: Some(50_000.0),
                order_type: OrderType::Limit,
                strategy_id: "test".into(),
                submitted_ts_ms: 0,
            },
            market: MarketContext {
                instrument: btc(),
                mid_price: 50_000.0,
                bid: 49_990.0,
                ask: 50_010.0,
                volume_24h: 1_000_000.0,
                volatility: 0.02,
                ts_ms: 0,
            },
            portfolio: PortfolioContext {
                total_capital: 100_000.0,
                available_capital: 80_000.0,
                total_exposure: 20_000.0,
                open_positions: 3, // at maximum
                daily_pnl: 500.0,
                daily_pnl_limit: -5_000.0,
            },
        };
        let result = checker.check(&input).unwrap();
        assert!(matches!(result, RiskDecision::Reject { .. }));
    }

    #[test]
    fn total_exposure_calculation() {
        let mut mgr = RiskPositionManager::new(make_pnl_limits(), make_stop_config());
        mgr.update_position(&btc(), Side::Buy, 2.0, 50_000.0, 1_000);
        mgr.update_position(
            &InstrumentId::new(Venue::Crypto, "ETH-USD"),
            Side::Buy,
            10.0,
            3_000.0,
            1_000,
        );
        let exposure = mgr.total_exposure();
        assert!((exposure - 130_000.0).abs() < 0.01);
    }
}
