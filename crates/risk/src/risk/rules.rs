use crate::core::{RiskChecker, RiskDecision, RiskError, RiskInput};
use serde::{Deserialize, Serialize};

// ── RuleCondition ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RuleCondition {
    QuantityExceeds {
        threshold: f64,
    },
    NotionalExceeds {
        threshold: f64,
    },
    VolatilityAbove {
        threshold: f64,
    },
    /// Triggers when daily_pnl < threshold
    DailyPnLBelow {
        threshold: f64,
    },
    OpenPositionsAbove {
        count: u32,
    },
    And(Vec<RuleCondition>),
    Or(Vec<RuleCondition>),
    Not(Box<RuleCondition>),
}

// ── RuleAction ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RuleAction {
    Reject {
        reason: String,
    },
    ScaleQuantity {
        factor: f64,
    },
    SetMaxNotional {
        value: f64,
    },
    /// Just logs, doesn't block
    Log {
        message: String,
    },
}

// ── RiskRule ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskRule {
    pub id: String,
    pub name: String,
    pub priority: u32,
    pub condition: RuleCondition,
    pub action: RuleAction,
    pub enabled: bool,
}

// ── RuleEngineConfig ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleEngineConfig {
    pub rules: Vec<RiskRule>,
    pub default_action: RuleAction,
}

// ── RuleEngine ────────────────────────────────────────────────────────────

pub struct RuleEngine {
    rules: Vec<RiskRule>,
}

impl RuleEngine {
    pub fn new(mut rules: Vec<RiskRule>) -> Self {
        rules.sort_by_key(|r| r.priority);
        Self { rules }
    }

    pub fn evaluate_condition(cond: &RuleCondition, input: &RiskInput) -> bool {
        match cond {
            RuleCondition::QuantityExceeds { threshold } => input.order.quantity > *threshold,
            RuleCondition::NotionalExceeds { threshold } => {
                let price = input.order.limit_price.unwrap_or(input.market.mid_price);
                input.order.quantity * price > *threshold
            }
            RuleCondition::VolatilityAbove { threshold } => input.market.volatility > *threshold,
            RuleCondition::DailyPnLBelow { threshold } => input.portfolio.daily_pnl < *threshold,
            RuleCondition::OpenPositionsAbove { count } => input.portfolio.open_positions > *count,
            RuleCondition::And(conditions) => conditions
                .iter()
                .all(|c| Self::evaluate_condition(c, input)),
            RuleCondition::Or(conditions) => conditions
                .iter()
                .any(|c| Self::evaluate_condition(c, input)),
            RuleCondition::Not(condition) => !Self::evaluate_condition(condition, input),
        }
    }

    /// Evaluate rules in priority order, stop at first Reject
    pub fn apply(&self, input: &RiskInput) -> Result<RiskDecision, RiskError> {
        for rule in &self.rules {
            if !rule.enabled {
                continue;
            }
            if Self::evaluate_condition(&rule.condition, input) {
                match &rule.action {
                    RuleAction::Reject { reason } => {
                        return Ok(RiskDecision::Reject {
                            reason: format!("[Rule:{}] {}", rule.id, reason),
                            risk_score: 100.0,
                        });
                    }
                    RuleAction::ScaleQuantity { factor } => {
                        return Ok(RiskDecision::ApproveWithAdjustment {
                            new_quantity: input.order.quantity * factor,
                            new_limit_price: input.order.limit_price,
                            reason: format!("[Rule:{}] Scale quantity by {}", rule.id, factor),
                        });
                    }
                    RuleAction::SetMaxNotional { value } => {
                        let price = input.order.limit_price.unwrap_or(input.market.mid_price);
                        let max_qty = if price > 0.0 {
                            value / price
                        } else {
                            input.order.quantity
                        };
                        let new_qty = input.order.quantity.min(max_qty);
                        return Ok(RiskDecision::ApproveWithAdjustment {
                            new_quantity: new_qty,
                            new_limit_price: input.order.limit_price,
                            reason: format!("[Rule:{}] Set max notional to {}", rule.id, value),
                        });
                    }
                    RuleAction::Log { message } => {
                        tracing::info!("[RuleEngine] Rule '{}' triggered: {}", rule.name, message);
                        // Continue evaluating
                    }
                }
            }
        }
        Ok(RiskDecision::Approve)
    }

    /// Deserialize a Vec<RiskRule> from JSON
    pub fn load_from_json(json: &str) -> Result<Vec<RiskRule>, RiskError> {
        serde_json::from_str(json).map_err(|e| RiskError::RuleError(e.to_string()))
    }

    /// Atomically replace rule set
    pub fn hot_reload(&mut self, mut new_rules: Vec<RiskRule>) {
        new_rules.sort_by_key(|r| r.priority);
        self.rules = new_rules;
    }
}

impl RiskChecker for RuleEngine {
    fn check(&self, input: &RiskInput) -> Result<RiskDecision, RiskError> {
        self.apply(input)
    }

    fn name(&self) -> &str {
        "RuleEngine"
    }

    fn priority(&self) -> u32 {
        50
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{MarketContext, OrderContext, OrderType, PortfolioContext, RiskInput};
    use domain::{InstrumentId, Side, Venue};

    fn make_input() -> RiskInput {
        RiskInput {
            order: OrderContext {
                instrument: InstrumentId::new(Venue::Crypto, "BTC-USD"),
                side: Side::Buy,
                quantity: 5.0,
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
                open_positions: 3,
                daily_pnl: -1_000.0,
                daily_pnl_limit: -5_000.0,
            },
        }
    }

    fn make_rule(id: &str, priority: u32, cond: RuleCondition, action: RuleAction) -> RiskRule {
        RiskRule {
            id: id.into(),
            name: id.into(),
            priority,
            condition: cond,
            action,
            enabled: true,
        }
    }

    #[test]
    fn and_condition_both_true() {
        let cond = RuleCondition::And(vec![
            RuleCondition::QuantityExceeds { threshold: 4.0 },
            RuleCondition::VolatilityAbove { threshold: 0.01 },
        ]);
        assert!(RuleEngine::evaluate_condition(&cond, &make_input()));
    }

    #[test]
    fn and_condition_one_false() {
        let cond = RuleCondition::And(vec![
            RuleCondition::QuantityExceeds { threshold: 4.0 },
            RuleCondition::VolatilityAbove { threshold: 0.10 }, // vol is 0.02, not above 0.10
        ]);
        assert!(!RuleEngine::evaluate_condition(&cond, &make_input()));
    }

    #[test]
    fn or_condition_one_true() {
        let cond = RuleCondition::Or(vec![
            RuleCondition::QuantityExceeds { threshold: 100.0 }, // false
            RuleCondition::VolatilityAbove { threshold: 0.01 },  // true
        ]);
        assert!(RuleEngine::evaluate_condition(&cond, &make_input()));
    }

    #[test]
    fn not_condition() {
        let cond = RuleCondition::Not(Box::new(RuleCondition::QuantityExceeds {
            threshold: 100.0,
        }));
        assert!(RuleEngine::evaluate_condition(&cond, &make_input())); // quantity 5 not > 100, NOT(false) = true
    }

    #[test]
    fn priority_ordering_stops_at_first_reject() {
        let engine = RuleEngine::new(vec![
            make_rule(
                "low-priority-reject",
                200,
                RuleCondition::QuantityExceeds { threshold: 4.0 },
                RuleAction::Reject {
                    reason: "qty too high".into(),
                },
            ),
            make_rule(
                "high-priority-scale",
                10,
                RuleCondition::QuantityExceeds { threshold: 4.0 },
                RuleAction::ScaleQuantity { factor: 0.5 },
            ),
        ]);
        let result = engine.apply(&make_input()).unwrap();
        // High priority (10) runs first → ScaleQuantity
        assert!(matches!(result, RiskDecision::ApproveWithAdjustment { .. }));
    }

    #[test]
    fn load_from_json() {
        let json = r#"[
            {
                "id": "r1",
                "name": "Test Rule",
                "priority": 100,
                "condition": {"type": "quantity_exceeds", "threshold": 10.0},
                "action": {"type": "reject", "reason": "too much"},
                "enabled": true
            }
        ]"#;
        let rules = RuleEngine::load_from_json(json).unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].id, "r1");
    }

    #[test]
    fn hot_reload_swaps_rules() {
        let mut engine = RuleEngine::new(vec![make_rule(
            "old-rule",
            100,
            RuleCondition::QuantityExceeds { threshold: 4.0 },
            RuleAction::Reject {
                reason: "old".into(),
            },
        )]);

        // Before reload: rejects
        let result = engine.apply(&make_input()).unwrap();
        assert!(matches!(result, RiskDecision::Reject { .. }));

        // After reload: no rules → approves
        engine.hot_reload(vec![]);
        let result = engine.apply(&make_input()).unwrap();
        assert!(matches!(result, RiskDecision::Approve));
    }

    #[test]
    fn daily_pnl_below_triggers() {
        let cond = RuleCondition::DailyPnLBelow { threshold: -500.0 }; // daily_pnl = -1000 < -500
        assert!(RuleEngine::evaluate_condition(&cond, &make_input()));
    }

    #[test]
    fn open_positions_above_triggers() {
        let cond = RuleCondition::OpenPositionsAbove { count: 2 }; // open_positions = 3 > 2
        assert!(RuleEngine::evaluate_condition(&cond, &make_input()));
    }
}
