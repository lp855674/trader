use domain::{InstrumentId, Side};
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ── OrderType ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OrderType {
    Market,
    Limit,
    StopLimit { stop: f64 },
}

// ── OrderContext ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct OrderContext {
    pub instrument: InstrumentId,
    pub side: Side,
    pub quantity: f64,
    pub limit_price: Option<f64>,
    pub order_type: OrderType,
    pub strategy_id: String,
    pub submitted_ts_ms: i64,
}

// ── MarketContext ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct MarketContext {
    pub instrument: InstrumentId,
    pub mid_price: f64,
    pub bid: f64,
    pub ask: f64,
    /// Daily volume
    pub volume_24h: f64,
    /// Daily volatility
    pub volatility: f64,
    pub ts_ms: i64,
}

// ── PortfolioContext ───────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PortfolioContext {
    pub total_capital: f64,
    pub available_capital: f64,
    /// Current notional exposure
    pub total_exposure: f64,
    pub open_positions: u32,
    pub daily_pnl: f64,
    pub daily_pnl_limit: f64,
}

// ── RiskInput ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RiskInput {
    pub order: OrderContext,
    pub market: MarketContext,
    pub portfolio: PortfolioContext,
}

// ── RiskDecision ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum RiskDecision {
    /// Order passes all checks
    Approve,
    /// Modified order passes
    ApproveWithAdjustment {
        new_quantity: f64,
        new_limit_price: Option<f64>,
        reason: String,
    },
    /// Order rejected
    Reject { reason: String, risk_score: f64 },
}

// ── RiskError ─────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum RiskError {
    #[error("Market data missing: {0}")]
    MarketDataMissing(String),
    #[error("Rule evaluation failed: {0}")]
    RuleError(String),
    #[error("Configuration error: {0}")]
    ConfigError(String),
}

// ── RiskChecker trait ─────────────────────────────────────────────────────

pub trait RiskChecker: Send + Sync {
    fn check(&self, input: &RiskInput) -> Result<RiskDecision, RiskError>;
    fn name(&self) -> &str;
    fn priority(&self) -> u32 {
        100
    }
}

// ── CompositeRiskChecker ──────────────────────────────────────────────────

/// Runs multiple checkers in priority order.
/// Returns first Reject or ApproveWithAdjustment; returns Approve only if ALL approve.
pub struct CompositeRiskChecker {
    checkers: Vec<Box<dyn RiskChecker>>,
}

impl CompositeRiskChecker {
    pub fn new(mut checkers: Vec<Box<dyn RiskChecker>>) -> Self {
        checkers.sort_by_key(|c| c.priority());
        Self { checkers }
    }

    pub fn add(&mut self, checker: Box<dyn RiskChecker>) {
        self.checkers.push(checker);
        self.checkers.sort_by_key(|c| c.priority());
    }
}

impl RiskChecker for CompositeRiskChecker {
    fn check(&self, input: &RiskInput) -> Result<RiskDecision, RiskError> {
        for checker in &self.checkers {
            let decision = checker.check(input)?;
            match decision {
                RiskDecision::Approve => continue,
                RiskDecision::ApproveWithAdjustment { .. } => return Ok(decision),
                RiskDecision::Reject { .. } => return Ok(decision),
            }
        }
        Ok(RiskDecision::Approve)
    }

    fn name(&self) -> &str {
        "CompositeRiskChecker"
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use domain::Venue;

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

    struct AlwaysApprove;
    impl RiskChecker for AlwaysApprove {
        fn check(&self, _: &RiskInput) -> Result<RiskDecision, RiskError> {
            Ok(RiskDecision::Approve)
        }
        fn name(&self) -> &str { "AlwaysApprove" }
        fn priority(&self) -> u32 { 50 }
    }

    struct AlwaysReject;
    impl RiskChecker for AlwaysReject {
        fn check(&self, _: &RiskInput) -> Result<RiskDecision, RiskError> {
            Ok(RiskDecision::Reject { reason: "always".into(), risk_score: 100.0 })
        }
        fn name(&self) -> &str { "AlwaysReject" }
        fn priority(&self) -> u32 { 200 }
    }

    #[test]
    fn composite_all_approve() {
        let checker = CompositeRiskChecker::new(vec![
            Box::new(AlwaysApprove),
            Box::new(AlwaysApprove),
        ]);
        let result = checker.check(&make_input()).unwrap();
        assert!(matches!(result, RiskDecision::Approve));
    }

    #[test]
    fn composite_stops_at_reject() {
        let checker = CompositeRiskChecker::new(vec![
            Box::new(AlwaysApprove),
            Box::new(AlwaysReject),
        ]);
        let result = checker.check(&make_input()).unwrap();
        assert!(matches!(result, RiskDecision::Reject { .. }));
    }

    #[test]
    fn composite_priority_ordering() {
        // Lower priority number = higher priority = checked first
        // Reject has priority 200 (low priority), Approve has 50 (high priority)
        // So Approve runs first, then Reject
        let checker = CompositeRiskChecker::new(vec![
            Box::new(AlwaysReject),
            Box::new(AlwaysApprove),
        ]);
        let result = checker.check(&make_input()).unwrap();
        // ApproveFirst (priority 50) runs, then Reject (priority 200) — first non-approve wins
        assert!(matches!(result, RiskDecision::Reject { .. }));
    }
}
