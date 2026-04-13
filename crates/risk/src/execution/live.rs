// Live execution mode with circuit breaker

use crate::core::{RiskChecker, RiskDecision, RiskError, RiskInput};
use std::sync::Arc;

// ── LiveConfig ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct LiveConfig {
    pub max_order_latency_ms: u64,
    /// Number of rejections in window that trips the circuit breaker
    pub circuit_breaker_threshold: u32,
    pub circuit_breaker_window_ms: u64,
    pub fallback_to_paper: bool,
}

impl Default for LiveConfig {
    fn default() -> Self {
        Self {
            max_order_latency_ms: 100,
            circuit_breaker_threshold: 5,
            circuit_breaker_window_ms: 60_000,
            fallback_to_paper: true,
        }
    }
}

// ── LiveExecutionMode ─────────────────────────────────────────────────────────

pub struct LiveExecutionMode {
    checker: Arc<dyn RiskChecker>,
    config: LiveConfig,
    rejection_count: u32,
    window_start_ms: i64,
    circuit_breaker_open: bool,
}

impl LiveExecutionMode {
    pub fn new(checker: Arc<dyn RiskChecker>, config: LiveConfig) -> Self {
        Self {
            checker,
            config,
            rejection_count: 0,
            window_start_ms: 0,
            circuit_breaker_open: false,
        }
    }

    pub fn check_and_submit(
        &mut self,
        input: &RiskInput,
        ts_ms: i64,
    ) -> Result<RiskDecision, RiskError> {
        // Reset window if expired
        self.reset_circuit_breaker(ts_ms);

        // Check circuit breaker
        if self.circuit_breaker_open {
            return Ok(RiskDecision::Reject {
                reason: "circuit breaker open".to_string(),
                risk_score: 100.0,
            });
        }

        let decision = self.checker.check(input)?;

        match &decision {
            RiskDecision::Reject { .. } => {
                self.rejection_count += 1;
                if self.rejection_count >= self.config.circuit_breaker_threshold {
                    self.circuit_breaker_open = true;
                    tracing::warn!(
                        count = self.rejection_count,
                        threshold = self.config.circuit_breaker_threshold,
                        "Circuit breaker tripped"
                    );
                }
            }
            _ => {}
        }

        Ok(decision)
    }

    /// Reset window and counter if the circuit breaker window has passed.
    pub fn reset_circuit_breaker(&mut self, ts_ms: i64) {
        let elapsed = (ts_ms - self.window_start_ms).unsigned_abs();
        if elapsed >= self.config.circuit_breaker_window_ms {
            self.rejection_count = 0;
            self.window_start_ms = ts_ms;
            self.circuit_breaker_open = false;
        }
    }

    pub fn is_circuit_breaker_open(&self) -> bool {
        self.circuit_breaker_open
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{MarketContext, OrderContext, OrderType, PortfolioContext, RiskInput};
    use domain::{InstrumentId, Side, Venue};

    struct AlwaysReject;
    impl RiskChecker for AlwaysReject {
        fn check(&self, _: &RiskInput) -> Result<RiskDecision, RiskError> {
            Ok(RiskDecision::Reject {
                reason: "always".into(),
                risk_score: 100.0,
            })
        }
        fn name(&self) -> &str {
            "AlwaysReject"
        }
    }

    struct AlwaysApprove;
    impl RiskChecker for AlwaysApprove {
        fn check(&self, _: &RiskInput) -> Result<RiskDecision, RiskError> {
            Ok(RiskDecision::Approve)
        }
        fn name(&self) -> &str {
            "AlwaysApprove"
        }
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
                submitted_ts_ms: 0,
            },
            market: MarketContext {
                instrument: InstrumentId::new(Venue::Crypto, "BTC-USD"),
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
                daily_pnl: 500.0,
                daily_pnl_limit: -5_000.0,
            },
        }
    }

    #[test]
    fn circuit_breaker_trips_after_n_rejections() {
        let config = LiveConfig {
            circuit_breaker_threshold: 3,
            circuit_breaker_window_ms: 60_000,
            ..LiveConfig::default()
        };
        let mut live = LiveExecutionMode::new(Arc::new(AlwaysReject), config);
        let input = make_input();

        for i in 0..3 {
            let _ = live.check_and_submit(&input, i * 1000);
        }

        assert!(
            live.is_circuit_breaker_open(),
            "Circuit breaker should be open after 3 rejections"
        );

        // Next call should return circuit breaker reject
        let result = live.check_and_submit(&input, 5_000).unwrap();
        assert!(
            matches!(result, RiskDecision::Reject { reason, .. } if reason.contains("circuit breaker"))
        );
    }

    #[test]
    fn circuit_breaker_resets_after_window() {
        let config = LiveConfig {
            circuit_breaker_threshold: 2,
            circuit_breaker_window_ms: 1_000,
            ..LiveConfig::default()
        };
        let mut live = LiveExecutionMode::new(Arc::new(AlwaysApprove), config);
        // Manually set high rejection count and open CB
        live.rejection_count = 5;
        live.circuit_breaker_open = true;
        live.window_start_ms = 0;

        // After window expires, reset
        live.reset_circuit_breaker(5_000);
        assert!(
            !live.is_circuit_breaker_open(),
            "Circuit breaker should reset after window"
        );
        assert_eq!(live.rejection_count, 0);
    }
}
