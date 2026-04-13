use crate::core::{OrderType, RiskChecker, RiskDecision, RiskError, RiskInput};

// ── OrderRiskConfig ────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OrderRiskConfig {
    /// Max % deviation from mid-price (e.g. 0.05 = 5%)
    pub max_price_deviation_pct: f64,
    pub max_quantity: f64,
    pub min_quantity: f64,
    /// Max order value (qty * price)
    pub max_notional: f64,
    /// Whitelist of allowed order types
    pub allowed_order_types: Vec<OrderType>,
    /// Halt if market volatility exceeds this
    pub circuit_breaker_volatility: f64,
    /// Multiplier when adjusting limits based on volatility
    pub dynamic_limit_scale: f64,
}

impl Default for OrderRiskConfig {
    fn default() -> Self {
        Self {
            max_price_deviation_pct: 0.05,
            max_quantity: 100.0,
            min_quantity: 0.001,
            max_notional: 1_000_000.0,
            allowed_order_types: vec![OrderType::Market, OrderType::Limit],
            circuit_breaker_volatility: 0.15,
            dynamic_limit_scale: 1.0,
        }
    }
}

// ── RiskScore ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct RiskScore {
    /// 0-100 (100 = max risk)
    pub total: f64,
    pub price_score: f64,
    pub quantity_score: f64,
    pub notional_score: f64,
    pub volatility_score: f64,
    pub rejection_reasons: Vec<String>,
    pub adjustment_suggestions: Vec<String>,
}

impl RiskScore {
    fn compute_total(&mut self) {
        self.total =
            (self.price_score + self.quantity_score + self.notional_score + self.volatility_score)
                / 4.0;
        self.total = self.total.clamp(0.0, 100.0);
    }
}

// ── OrderRiskChecker ──────────────────────────────────────────────────────

pub struct OrderRiskChecker {
    config: OrderRiskConfig,
}

impl OrderRiskChecker {
    pub fn new(config: OrderRiskConfig) -> Self {
        Self { config }
    }

    fn order_type_matches(a: &OrderType, b: &OrderType) -> bool {
        match (a, b) {
            (OrderType::Market, OrderType::Market) => true,
            (OrderType::Limit, OrderType::Limit) => true,
            (OrderType::StopLimit { .. }, OrderType::StopLimit { .. }) => true,
            _ => false,
        }
    }
}

impl RiskChecker for OrderRiskChecker {
    fn check(&self, input: &RiskInput) -> Result<RiskDecision, RiskError> {
        let order = &input.order;
        let market = &input.market;
        let cfg = &self.config;

        let mut score = RiskScore::default();

        // 1. Check order type allowlist
        let type_allowed = cfg
            .allowed_order_types
            .iter()
            .any(|t| Self::order_type_matches(t, &order.order_type));
        if !type_allowed {
            return Ok(RiskDecision::Reject {
                reason: format!("Order type {:?} not allowed", order.order_type),
                risk_score: 100.0,
            });
        }

        // 2. Circuit breaker – check volatility first
        score.volatility_score =
            (market.volatility / cfg.circuit_breaker_volatility * 100.0).min(100.0);
        if market.volatility > cfg.circuit_breaker_volatility {
            score.rejection_reasons.push(format!(
                "Circuit breaker: market volatility {:.4} exceeds threshold {:.4}",
                market.volatility, cfg.circuit_breaker_volatility
            ));
            score.compute_total();
            return Ok(RiskDecision::Reject {
                reason: score.rejection_reasons.join("; "),
                risk_score: score.total,
            });
        }

        // 3. Quantity limits
        score.quantity_score = if order.quantity > cfg.max_quantity {
            100.0
        } else if order.quantity < cfg.min_quantity {
            100.0
        } else {
            (order.quantity / cfg.max_quantity * 100.0).clamp(0.0, 100.0)
        };
        if order.quantity < cfg.min_quantity {
            score.rejection_reasons.push(format!(
                "Quantity {:.6} below minimum {:.6}",
                order.quantity, cfg.min_quantity
            ));
        }
        if order.quantity > cfg.max_quantity {
            score.rejection_reasons.push(format!(
                "Quantity {:.6} exceeds maximum {:.6}",
                order.quantity, cfg.max_quantity
            ));
        }

        // 4. Notional limit
        let effective_price = order.limit_price.unwrap_or(market.mid_price);
        let notional = order.quantity * effective_price;
        score.notional_score = (notional / cfg.max_notional * 100.0).clamp(0.0, 100.0);
        if notional > cfg.max_notional {
            score.rejection_reasons.push(format!(
                "Notional {:.2} exceeds maximum {:.2}",
                notional, cfg.max_notional
            ));
        }

        // 5. Price deviation
        let mut adjusted_limit: Option<f64> = order.limit_price;
        let mut price_adjustment_reason: Option<String> = None;
        if let Some(limit_price) = order.limit_price {
            let deviation = (limit_price - market.mid_price).abs() / market.mid_price;
            score.price_score = (deviation / cfg.max_price_deviation_pct * 100.0).clamp(0.0, 100.0);
            if deviation > cfg.max_price_deviation_pct {
                // Clamp to max deviation
                let clamped = if limit_price > market.mid_price {
                    market.mid_price * (1.0 + cfg.max_price_deviation_pct)
                } else {
                    market.mid_price * (1.0 - cfg.max_price_deviation_pct)
                };
                price_adjustment_reason = Some(format!(
                    "Price deviation {:.2}% exceeds max {:.2}%, adjusted to {:.4}",
                    deviation * 100.0,
                    cfg.max_price_deviation_pct * 100.0,
                    clamped
                ));
                score
                    .adjustment_suggestions
                    .push(price_adjustment_reason.clone().unwrap());
                adjusted_limit = Some(clamped);
            }
        }

        score.compute_total();

        // Decide outcome
        if !score.rejection_reasons.is_empty() {
            return Ok(RiskDecision::Reject {
                reason: score.rejection_reasons.join("; "),
                risk_score: score.total,
            });
        }

        if let Some(reason) = price_adjustment_reason {
            return Ok(RiskDecision::ApproveWithAdjustment {
                new_quantity: order.quantity,
                new_limit_price: adjusted_limit,
                reason,
            });
        }

        Ok(RiskDecision::Approve)
    }

    fn name(&self) -> &str {
        "OrderRiskChecker"
    }

    fn priority(&self) -> u32 {
        10
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{MarketContext, OrderContext, PortfolioContext, RiskInput};
    use domain::{InstrumentId, Side, Venue};

    fn make_base_input() -> RiskInput {
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

    fn default_checker() -> OrderRiskChecker {
        OrderRiskChecker::new(OrderRiskConfig::default())
    }

    #[test]
    fn approves_valid_order() {
        let checker = default_checker();
        let input = make_base_input();
        let result = checker.check(&input).unwrap();
        assert!(matches!(result, RiskDecision::Approve));
    }

    #[test]
    fn rejects_quantity_below_min() {
        let checker = default_checker();
        let mut input = make_base_input();
        input.order.quantity = 0.0000001;
        let result = checker.check(&input).unwrap();
        assert!(matches!(result, RiskDecision::Reject { .. }));
    }

    #[test]
    fn rejects_quantity_above_max() {
        let checker = default_checker();
        let mut input = make_base_input();
        input.order.quantity = 10_000.0;
        let result = checker.check(&input).unwrap();
        assert!(matches!(result, RiskDecision::Reject { .. }));
    }

    #[test]
    fn adjusts_excessive_price_deviation() {
        let checker = default_checker();
        let mut input = make_base_input();
        // 10% above mid — exceeds 5% max
        input.order.limit_price = Some(55_000.0);
        let result = checker.check(&input).unwrap();
        match result {
            RiskDecision::ApproveWithAdjustment {
                new_limit_price, ..
            } => {
                let expected = 50_000.0 * 1.05;
                assert!((new_limit_price.unwrap() - expected).abs() < 0.01);
            }
            other => panic!("Expected ApproveWithAdjustment, got {:?}", other),
        }
    }

    #[test]
    fn circuit_breaker_triggers() {
        let checker = default_checker();
        let mut input = make_base_input();
        input.market.volatility = 0.20; // > 0.15 threshold
        let result = checker.check(&input).unwrap();
        assert!(matches!(result, RiskDecision::Reject { .. }));
    }

    #[test]
    fn notional_limit_rejects() {
        let checker = default_checker();
        let mut input = make_base_input();
        // 100 BTC @ 50k = 5M notional > 1M limit
        input.order.quantity = 100.0;
        let result = checker.check(&input).unwrap();
        assert!(matches!(result, RiskDecision::Reject { .. }));
    }

    #[test]
    fn rejects_disallowed_order_type() {
        let checker = default_checker();
        let mut input = make_base_input();
        input.order.order_type = OrderType::StopLimit { stop: 49_000.0 };
        let result = checker.check(&input).unwrap();
        assert!(matches!(result, RiskDecision::Reject { .. }));
    }
}
