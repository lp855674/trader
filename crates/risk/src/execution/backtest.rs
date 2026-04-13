// Backtest execution mode: historical risk simulation with slippage model

use crate::core::{RiskChecker, RiskDecision, RiskError, RiskInput};
use std::sync::Arc;

// ── BacktestSlippage ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum BacktestSlippage {
    None,
    Fixed(f64),
    VolumeBased { impact: f64 },
}

// ── BacktestExecConfig ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct BacktestExecConfig {
    pub reject_on_high_vol: bool,
    pub vol_threshold: f64,
    pub slippage_model: BacktestSlippage,
}

impl Default for BacktestExecConfig {
    fn default() -> Self {
        Self {
            reject_on_high_vol: false,
            vol_threshold: 0.05,
            slippage_model: BacktestSlippage::Fixed(0.0001),
        }
    }
}

// ── BacktestExecutionMode ─────────────────────────────────────────────────────

pub struct BacktestExecutionMode {
    checker: Arc<dyn RiskChecker>,
    config: BacktestExecConfig,
}

impl BacktestExecutionMode {
    pub fn new(checker: Arc<dyn RiskChecker>, config: BacktestExecConfig) -> Self {
        Self { checker, config }
    }

    /// Process a bar: run risk check and apply slippage to fill price.
    /// Returns (decision, fill_price)
    pub fn process_bar(
        &self,
        input: &RiskInput,
        volume: f64,
    ) -> Result<(RiskDecision, f64), RiskError> {
        // Optional high-vol rejection before passing to checker
        if self.config.reject_on_high_vol && input.market.volatility > self.config.vol_threshold {
            return Ok((
                RiskDecision::Reject {
                    reason: format!(
                        "High volatility {:.4} exceeds backtest threshold {:.4}",
                        input.market.volatility, self.config.vol_threshold
                    ),
                    risk_score: 85.0,
                },
                input.market.mid_price,
            ));
        }

        let decision = self.checker.check(input)?;

        let base_price = input.market.mid_price;
        let fill_price = match &self.config.slippage_model {
            BacktestSlippage::None => base_price,
            BacktestSlippage::Fixed(bps) => base_price * (1.0 + bps),
            BacktestSlippage::VolumeBased { impact } => {
                // Market impact proportional to order size / market volume
                let vol = volume.max(1.0);
                let order_qty = input.order.quantity;
                let impact_fraction = impact * order_qty / vol;
                base_price * (1.0 + impact_fraction)
            }
        };

        Ok((decision, fill_price))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{MarketContext, OrderContext, OrderType, PortfolioContext};
    use domain::{InstrumentId, Side, Venue};

    struct AlwaysApprove;
    impl RiskChecker for AlwaysApprove {
        fn check(&self, _: &RiskInput) -> Result<RiskDecision, RiskError> {
            Ok(RiskDecision::Approve)
        }
        fn name(&self) -> &str {
            "AlwaysApprove"
        }
    }

    fn make_input(vol: f64) -> RiskInput {
        RiskInput {
            order: OrderContext {
                instrument: InstrumentId::new(Venue::Crypto, "BTC-USD"),
                side: Side::Buy,
                quantity: 1.0,
                limit_price: Some(50_000.0),
                order_type: OrderType::Limit,
                strategy_id: "test".into(),
                submitted_ts_ms: 0,
            },
            market: MarketContext {
                instrument: InstrumentId::new(Venue::Crypto, "BTC-USD"),
                mid_price: 50_000.0,
                bid: 49_990.0,
                ask: 50_010.0,
                volume_24h: 1_000_000.0,
                volatility: vol,
                ts_ms: 0,
            },
            portfolio: PortfolioContext {
                total_capital: 100_000.0,
                available_capital: 80_000.0,
                total_exposure: 20_000.0,
                open_positions: 2,
                daily_pnl: 500.0,
                daily_pnl_limit: -5_000.0,
            },
        }
    }

    #[test]
    fn backtest_rejects_on_high_vol() {
        let config = BacktestExecConfig {
            reject_on_high_vol: true,
            vol_threshold: 0.03,
            slippage_model: BacktestSlippage::None,
        };
        let bt = BacktestExecutionMode::new(Arc::new(AlwaysApprove), config);
        let input = make_input(0.05); // vol > threshold
        let (decision, _) = bt.process_bar(&input, 1_000_000.0).unwrap();
        assert!(matches!(decision, RiskDecision::Reject { .. }));
    }

    #[test]
    fn backtest_approves_normal_vol() {
        let config = BacktestExecConfig {
            reject_on_high_vol: true,
            vol_threshold: 0.10,
            slippage_model: BacktestSlippage::None,
        };
        let bt = BacktestExecutionMode::new(Arc::new(AlwaysApprove), config);
        let input = make_input(0.02); // vol < threshold
        let (decision, _) = bt.process_bar(&input, 1_000_000.0).unwrap();
        assert!(matches!(decision, RiskDecision::Approve));
    }

    #[test]
    fn fixed_slippage_applied() {
        let config = BacktestExecConfig {
            reject_on_high_vol: false,
            vol_threshold: 0.10,
            slippage_model: BacktestSlippage::Fixed(0.001), // 10bps
        };
        let bt = BacktestExecutionMode::new(Arc::new(AlwaysApprove), config);
        let input = make_input(0.02);
        let (_, fill_price) = bt.process_bar(&input, 1_000_000.0).unwrap();
        let expected = 50_000.0 * 1.001;
        assert!((fill_price - expected).abs() < 0.01);
    }
}
