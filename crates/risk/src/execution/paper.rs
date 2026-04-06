// Paper execution mode: simulated fills with slippage

use std::sync::Arc;
use crate::core::{RiskChecker, RiskDecision, RiskError, RiskInput};

// ── PaperExecConfig ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PaperExecConfig {
    pub apply_slippage: bool,
    /// Slippage in basis points
    pub slippage_bps: f64,
    /// Probability of fill (0.0 – 1.0)
    pub fill_probability: f64,
}

impl Default for PaperExecConfig {
    fn default() -> Self {
        Self {
            apply_slippage: true,
            slippage_bps: 5.0,
            fill_probability: 0.95,
        }
    }
}

// ── LCG (for fill probability simulation) ────────────────────────────────────

struct Lcg {
    state: u64,
}

impl Lcg {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.state
    }

    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

// ── PaperExecutionMode ────────────────────────────────────────────────────────

pub struct PaperExecutionMode {
    checker: Arc<dyn RiskChecker>,
    config: PaperExecConfig,
}

impl PaperExecutionMode {
    pub fn new(checker: Arc<dyn RiskChecker>, config: PaperExecConfig) -> Self {
        Self { checker, config }
    }

    /// Run risk check; if approved, simulate fill with slippage and fill probability.
    /// Returns (decision, fill_price_option)
    pub fn check_and_fill(
        &self,
        input: &RiskInput,
    ) -> Result<(RiskDecision, Option<f64>), RiskError> {
        let decision = self.checker.check(input)?;

        match &decision {
            RiskDecision::Approve | RiskDecision::ApproveWithAdjustment { .. } => {
                // Use order timestamp as seed for deterministic fill probability
                let seed = input.order.submitted_ts_ms as u64 ^ 0xdeadbeef;
                let mut rng = Lcg::new(seed);
                let roll = rng.next_f64();

                if roll < self.config.fill_probability {
                    let base_price = input.market.mid_price;
                    let fill_price = if self.config.apply_slippage {
                        let slippage_factor = self.config.slippage_bps / 10_000.0;
                        base_price * (1.0 + slippage_factor)
                    } else {
                        base_price
                    };
                    Ok((decision, Some(fill_price)))
                } else {
                    Ok((decision, None))
                }
            }
            RiskDecision::Reject { .. } => Ok((decision, None)),
        }
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
        fn name(&self) -> &str { "AlwaysApprove" }
    }

    fn make_input() -> RiskInput {
        RiskInput {
            order: OrderContext {
                instrument: InstrumentId::new(Venue::Crypto, "BTC-USD"),
                side: Side::Buy,
                quantity: 1.0,
                limit_price: Some(50_000.0),
                order_type: OrderType::Limit,
                strategy_id: "test".into(),
                submitted_ts_ms: 12345,
            },
            market: MarketContext {
                instrument: InstrumentId::new(Venue::Crypto, "BTC-USD"),
                mid_price: 50_000.0,
                bid: 49_990.0,
                ask: 50_010.0,
                volume_24h: 1_000_000.0,
                volatility: 0.02,
                ts_ms: 12345,
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
    fn paper_mode_applies_slippage() {
        let config = PaperExecConfig {
            apply_slippage: true,
            slippage_bps: 10.0,
            fill_probability: 1.0, // always fill
        };
        let paper = PaperExecutionMode::new(Arc::new(AlwaysApprove), config);
        let input = make_input();
        let (decision, fill) = paper.check_and_fill(&input).unwrap();
        assert!(matches!(decision, RiskDecision::Approve));
        let fill_price = fill.expect("Should have a fill price");
        let expected = 50_000.0 * (1.0 + 10.0 / 10_000.0);
        assert!((fill_price - expected).abs() < 0.01, "Fill price should include slippage");
    }

    #[test]
    fn paper_mode_no_fill_when_rejected() {
        struct AlwaysReject;
        impl RiskChecker for AlwaysReject {
            fn check(&self, _: &RiskInput) -> Result<RiskDecision, RiskError> {
                Ok(RiskDecision::Reject { reason: "test".into(), risk_score: 100.0 })
            }
            fn name(&self) -> &str { "AlwaysReject" }
        }

        let paper = PaperExecutionMode::new(Arc::new(AlwaysReject), PaperExecConfig::default());
        let (decision, fill) = paper.check_and_fill(&make_input()).unwrap();
        assert!(matches!(decision, RiskDecision::Reject { .. }));
        assert!(fill.is_none(), "Rejected order should not have fill price");
    }
}
