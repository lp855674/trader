// HTTP handler for risk service (no actual HTTP server — in-process handler)

use std::sync::Arc;
use serde::{Deserialize, Serialize};
use crate::core::RiskChecker;
use crate::api::grpc::{RiskCheckService, RiskServiceRequest};

// ── RiskHealthResponse ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskHealthResponse {
    pub status: String,
    pub checks_per_second: f64,
    pub active_alerts: u32,
}

// ── RiskHttpHandler ───────────────────────────────────────────────────────────

pub struct RiskHttpHandler {
    checker: Arc<dyn RiskChecker>,
}

impl RiskHttpHandler {
    pub fn new(checker: Arc<dyn RiskChecker>) -> Self {
        Self { checker }
    }

    pub fn health(&self) -> RiskHealthResponse {
        RiskHealthResponse {
            status: "ok".to_string(),
            checks_per_second: 0.0,
            active_alerts: 0,
        }
    }

    pub fn check_order(&self, order_json: &str) -> Result<String, String> {
        let req: RiskServiceRequest = serde_json::from_str(order_json)
            .map_err(|e| format!("JSON parse error: {}", e))?;

        let service = RiskCheckService::new(Arc::clone(&self.checker));
        let resp = service.check_risk(&req);
        serde_json::to_string(&resp).map_err(|e| e.to_string())
    }

    pub fn get_metrics(&self) -> String {
        r#"{"metrics": "ok"}"#.to_string()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{RiskDecision, RiskError, RiskInput};

    struct AlwaysApprove;
    impl RiskChecker for AlwaysApprove {
        fn check(&self, _: &RiskInput) -> Result<RiskDecision, RiskError> {
            Ok(RiskDecision::Approve)
        }
        fn name(&self) -> &str { "AlwaysApprove" }
    }

    #[test]
    fn health_returns_ok() {
        let handler = RiskHttpHandler::new(Arc::new(AlwaysApprove));
        let health = handler.health();
        assert_eq!(health.status, "ok");
    }

    #[test]
    fn check_order_returns_json() {
        let handler = RiskHttpHandler::new(Arc::new(AlwaysApprove));
        let order_json = serde_json::json!({
            "order": {
                "instrument": "CRYPTO:BTC-USD",
                "side": "Buy",
                "quantity": 1.0,
                "limit_price": 50000.0,
                "submitted_ts_ms": 0
            },
            "market": {
                "mid_price": 50000.0,
                "bid": 49990.0,
                "ask": 50010.0,
                "volume_24h": 1000000.0,
                "volatility": 0.02
            },
            "portfolio": {
                "total_capital": 100000.0,
                "available_capital": 80000.0,
                "total_exposure": 20000.0,
                "open_positions": 2,
                "daily_pnl": 500.0,
                "daily_pnl_limit": -5000.0
            }
        })
        .to_string();

        let result = handler.check_order(&order_json);
        assert!(result.is_ok(), "check_order should succeed");
        let json_str = result.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed["decision"], "Approve");
    }

    #[test]
    fn get_metrics_returns_ok_json() {
        let handler = RiskHttpHandler::new(Arc::new(AlwaysApprove));
        let metrics = handler.get_metrics();
        assert!(metrics.contains("ok"));
    }
}
