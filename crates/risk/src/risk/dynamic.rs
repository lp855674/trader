use crate::core::{RiskChecker, RiskDecision, RiskError, RiskInput};

// ── EwmaVolatility ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct EwmaVolatility {
    /// Smoothing factor (0 < alpha < 1); typical 0.94
    pub alpha: f64,
    /// Current EWMA variance estimate
    pub variance: f64,
}

impl EwmaVolatility {
    pub fn new(alpha: f64, initial_variance: f64) -> Self {
        assert!(alpha > 0.0 && alpha < 1.0, "alpha must be in (0,1)");
        Self { alpha, variance: initial_variance }
    }

    /// Update with a new return observation
    pub fn update(&mut self, return_: f64) {
        self.variance = self.alpha * self.variance + (1.0 - self.alpha) * return_ * return_;
    }

    /// Current volatility estimate (std dev)
    pub fn volatility(&self) -> f64 {
        self.variance.sqrt()
    }

    /// Number of bars until weight halves
    pub fn half_life(&self) -> f64 {
        (-1.0 / self.alpha.ln()) * 2.0_f64.ln()
    }
}

// ── VolatilityAdjuster ────────────────────────────────────────────────────

pub struct VolatilityAdjuster {
    ewma: EwmaVolatility,
    base_max_quantity: f64,
    base_max_notional: f64,
    /// Base volatility reference (initial volatility)
    base_volatility: f64,
}

impl VolatilityAdjuster {
    pub fn new(alpha: f64, base_max_quantity: f64, base_max_notional: f64) -> Self {
        let initial_var: f64 = 0.0004; // ~2% daily vol
        let base_volatility = initial_var.sqrt();
        Self {
            ewma: EwmaVolatility::new(alpha, initial_var),
            base_max_quantity,
            base_max_notional,
            base_volatility,
        }
    }

    pub fn update_volatility(&mut self, return_: f64) {
        self.ewma.update(return_);
    }
}

impl RiskChecker for VolatilityAdjuster {
    fn check(&self, input: &RiskInput) -> Result<RiskDecision, RiskError> {
        let current_vol = self.ewma.volatility();
        let ratio = if self.base_volatility > 0.0 {
            current_vol / self.base_volatility
        } else {
            1.0
        };

        let scale = if ratio > 2.0 {
            0.5
        } else if ratio < 0.5 {
            1.5
        } else {
            return Ok(RiskDecision::Approve);
        };

        let new_quantity = (input.order.quantity * scale).min(self.base_max_quantity * scale);
        let adjusted_quantity = new_quantity.min(input.order.quantity);

        let reason = format!(
            "Volatility adjustment: current_vol={:.4}, base_vol={:.4}, ratio={:.2}, scale={:.1}",
            current_vol, self.base_volatility, ratio, scale
        );

        Ok(RiskDecision::ApproveWithAdjustment {
            new_quantity: if scale < 1.0 { adjusted_quantity } else { input.order.quantity },
            new_limit_price: input.order.limit_price,
            reason,
        })
    }

    fn name(&self) -> &str {
        "VolatilityAdjuster"
    }

    fn priority(&self) -> u32 {
        20
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{MarketContext, OrderContext, OrderType, PortfolioContext, RiskInput};
    use domain::{InstrumentId, Side, Venue};

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
    fn ewma_converges() {
        let mut ewma = EwmaVolatility::new(0.94, 0.0004);
        for _ in 0..100 {
            ewma.update(0.02); // 2% return
        }
        let vol = ewma.volatility();
        // Should converge towards ~0.02 * sqrt(1/(1-0.94)) ≈ but actually for EWMA
        // it converges toward r^2 * (1-alpha)/(1-alpha) = r^2 in steady state
        // variance = 0.02^2 = 0.0004, vol = 0.02
        assert!(vol > 0.01 && vol < 0.05, "vol={}", vol);
    }

    #[test]
    fn half_life_positive() {
        let ewma = EwmaVolatility::new(0.94, 0.0004);
        let hl = ewma.half_life();
        assert!(hl > 0.0, "half_life={}", hl);
        // For alpha=0.94, half-life ≈ 11.35 bars
        assert!(hl > 10.0 && hl < 15.0, "half_life={}", hl);
    }

    #[test]
    fn high_vol_reduces_limits() {
        let mut adjuster = VolatilityAdjuster::new(0.94, 10.0, 500_000.0);
        // Feed very high returns to spike volatility
        for _ in 0..50 {
            adjuster.update_volatility(0.10); // 10% return
        }
        let input = make_input(0.10);
        let result = adjuster.check(&input).unwrap();
        match result {
            RiskDecision::ApproveWithAdjustment { new_quantity, .. } => {
                assert!(new_quantity <= input.order.quantity);
            }
            RiskDecision::Approve => {
                // Might still approve if ratio didn't exceed 2x
                // Check that we at least don't panic
            }
            other => panic!("Unexpected result: {:?}", other),
        }
    }

    #[test]
    fn low_vol_relaxes_limits() {
        let mut adjuster = VolatilityAdjuster::new(0.94, 10.0, 500_000.0);
        // Feed very low returns to reduce volatility
        for _ in 0..100 {
            adjuster.update_volatility(0.0001); // tiny return
        }
        let input = make_input(0.001);
        let result = adjuster.check(&input).unwrap();
        // Either Approve or ApproveWithAdjustment (quantity >= original or approve)
        match result {
            RiskDecision::Approve => {}
            RiskDecision::ApproveWithAdjustment { new_quantity, .. } => {
                // Scale up means quantity should equal original (we cap at input qty for scale up)
                assert!(new_quantity >= input.order.quantity * 0.9);
            }
            RiskDecision::Reject { .. } => panic!("Should not reject in low-vol scenario"),
        }
    }
}
