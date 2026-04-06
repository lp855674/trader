// In-process gRPC-style risk check service (no tonic)

use std::sync::Arc;
use serde::{Deserialize, Serialize};
use crate::core::{RiskChecker, RiskDecision, RiskInput, MarketContext, OrderContext, PortfolioContext, OrderType};
use domain::{InstrumentId, Side, Venue};

// ── RiskServiceRequest ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskServiceRequest {
    pub order: serde_json::Value,
    pub market: serde_json::Value,
    pub portfolio: serde_json::Value,
}

// ── RiskServiceResponse ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskServiceResponse {
    pub decision: String,
    pub risk_score: f64,
    pub reason: Option<String>,
    pub adjusted_qty: Option<f64>,
}

// ── RiskCheckService ──────────────────────────────────────────────────────────

pub struct RiskCheckService {
    checker: Arc<dyn RiskChecker>,
}

impl RiskCheckService {
    pub fn new(checker: Arc<dyn RiskChecker>) -> Self {
        Self { checker }
    }

    pub fn check_risk(&self, req: &RiskServiceRequest) -> RiskServiceResponse {
        // Try to deserialize a RiskInput from the JSON fields
        match self.parse_risk_input(req) {
            Ok(input) => {
                match self.checker.check(&input) {
                    Ok(decision) => decision_to_response(decision),
                    Err(e) => RiskServiceResponse {
                        decision: "Error".to_string(),
                        risk_score: 0.0,
                        reason: Some(e.to_string()),
                        adjusted_qty: None,
                    },
                }
            }
            Err(e) => RiskServiceResponse {
                decision: "Error".to_string(),
                risk_score: 0.0,
                reason: Some(format!("Failed to parse request: {}", e)),
                adjusted_qty: None,
            },
        }
    }

    pub fn check_portfolio(&self, _portfolio_json: &str) -> serde_json::Value {
        serde_json::json!({ "status": "ok" })
    }

    fn parse_risk_input(&self, req: &RiskServiceRequest) -> Result<RiskInput, String> {
        // Parse order
        let instrument_str = req.order["instrument"]
            .as_str()
            .unwrap_or("CRYPTO:BTC-USD")
            .to_string();
        let instrument = parse_instrument(&instrument_str);

        let side_str = req.order["side"].as_str().unwrap_or("Buy");
        let side = if side_str == "Sell" { Side::Sell } else { Side::Buy };

        let quantity = req.order["quantity"].as_f64().unwrap_or(1.0);
        let limit_price = req.order["limit_price"].as_f64();
        let submitted_ts_ms = req.order["submitted_ts_ms"].as_i64().unwrap_or(0);

        // Parse market
        let mid_price = req.market["mid_price"].as_f64().unwrap_or(50_000.0);
        let bid = req.market["bid"].as_f64().unwrap_or(mid_price - 10.0);
        let ask = req.market["ask"].as_f64().unwrap_or(mid_price + 10.0);
        let volume_24h = req.market["volume_24h"].as_f64().unwrap_or(1_000_000.0);
        let volatility = req.market["volatility"].as_f64().unwrap_or(0.02);

        // Parse portfolio
        let total_capital = req.portfolio["total_capital"].as_f64().unwrap_or(100_000.0);
        let available_capital = req.portfolio["available_capital"].as_f64().unwrap_or(80_000.0);
        let total_exposure = req.portfolio["total_exposure"].as_f64().unwrap_or(20_000.0);
        let open_positions = req.portfolio["open_positions"].as_u64().unwrap_or(2) as u32;
        let daily_pnl = req.portfolio["daily_pnl"].as_f64().unwrap_or(0.0);
        let daily_pnl_limit = req.portfolio["daily_pnl_limit"].as_f64().unwrap_or(-5_000.0);

        Ok(RiskInput {
            order: OrderContext {
                instrument: instrument.clone(),
                side,
                quantity,
                limit_price,
                order_type: OrderType::Limit,
                strategy_id: "grpc".to_string(),
                submitted_ts_ms,
            },
            market: MarketContext {
                instrument,
                mid_price,
                bid,
                ask,
                volume_24h,
                volatility,
                ts_ms: submitted_ts_ms,
            },
            portfolio: PortfolioContext {
                total_capital,
                available_capital,
                total_exposure,
                open_positions,
                daily_pnl,
                daily_pnl_limit,
            },
        })
    }
}

fn parse_instrument(s: &str) -> InstrumentId {
    if let Some(pos) = s.find(':') {
        let venue_str = &s[..pos];
        let symbol = &s[pos + 1..];
        let venue = Venue::parse(venue_str).unwrap_or(Venue::Crypto);
        InstrumentId::new(venue, symbol)
    } else {
        InstrumentId::new(Venue::Crypto, s)
    }
}

fn decision_to_response(decision: RiskDecision) -> RiskServiceResponse {
    match decision {
        RiskDecision::Approve => RiskServiceResponse {
            decision: "Approve".to_string(),
            risk_score: 0.0,
            reason: None,
            adjusted_qty: None,
        },
        RiskDecision::ApproveWithAdjustment { new_quantity, reason, .. } => {
            RiskServiceResponse {
                decision: "ApproveWithAdjustment".to_string(),
                risk_score: 10.0,
                reason: Some(reason),
                adjusted_qty: Some(new_quantity),
            }
        }
        RiskDecision::Reject { reason, risk_score } => RiskServiceResponse {
            decision: "Reject".to_string(),
            risk_score,
            reason: Some(reason),
            adjusted_qty: None,
        },
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    struct AlwaysApprove;
    impl RiskChecker for AlwaysApprove {
        fn check(&self, _: &RiskInput) -> Result<RiskDecision, crate::core::RiskError> {
            Ok(RiskDecision::Approve)
        }
        fn name(&self) -> &str { "AlwaysApprove" }
    }

    fn valid_request() -> RiskServiceRequest {
        RiskServiceRequest {
            order: serde_json::json!({
                "instrument": "CRYPTO:BTC-USD",
                "side": "Buy",
                "quantity": 1.0,
                "limit_price": 50000.0,
                "submitted_ts_ms": 0
            }),
            market: serde_json::json!({
                "mid_price": 50000.0,
                "bid": 49990.0,
                "ask": 50010.0,
                "volume_24h": 1000000.0,
                "volatility": 0.02
            }),
            portfolio: serde_json::json!({
                "total_capital": 100000.0,
                "available_capital": 80000.0,
                "total_exposure": 20000.0,
                "open_positions": 2,
                "daily_pnl": 500.0,
                "daily_pnl_limit": -5000.0
            }),
        }
    }

    #[test]
    fn service_approves_valid_order() {
        let service = RiskCheckService::new(Arc::new(AlwaysApprove));
        let req = valid_request();
        let resp = service.check_risk(&req);
        assert_eq!(resp.decision, "Approve");
    }

    #[test]
    fn check_portfolio_returns_ok() {
        let service = RiskCheckService::new(Arc::new(AlwaysApprove));
        let result = service.check_portfolio("{}");
        assert_eq!(result["status"], "ok");
    }
}
