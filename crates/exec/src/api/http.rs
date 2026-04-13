use std::sync::{Arc, Mutex};

use crate::core::order::OrderManager;
use crate::core::types::OrderRequest;

#[derive(Debug, Clone)]
pub struct ExecApiError {
    pub code: u16,
    pub message: String,
}

impl ExecApiError {
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self {
            code: 404,
            message: msg.into(),
        }
    }

    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self {
            code: 400,
            message: msg.into(),
        }
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self {
            code: 500,
            message: msg.into(),
        }
    }
}

pub struct ExecHttpHandler {
    manager: Arc<Mutex<OrderManager>>,
}

impl ExecHttpHandler {
    pub fn new(manager: Arc<Mutex<OrderManager>>) -> Self {
        Self { manager }
    }

    pub fn handle_health(&self) -> String {
        r#"{"status":"ok"}"#.to_string()
    }

    pub fn handle_list_orders(&self) -> String {
        let mgr = self.manager.lock().unwrap();
        let orders: Vec<serde_json::Value> = mgr
            .open_orders()
            .iter()
            .map(|o| {
                serde_json::json!({
                    "id": o.id,
                    "instrument": o.request.instrument.to_string(),
                    "state": format!("{:?}", o.state),
                    "filled_qty": o.filled_qty,
                })
            })
            .collect();
        serde_json::json!({ "orders": orders }).to_string()
    }

    pub fn handle_cancel(&self, order_id: &str, ts_ms: i64) -> Result<String, ExecApiError> {
        let mut mgr = self.manager.lock().unwrap();
        mgr.cancel(order_id, ts_ms)
            .map(|_| serde_json::json!({ "cancelled": order_id }).to_string())
            .map_err(|e| ExecApiError::not_found(e.to_string()))
    }

    pub fn handle_submit(&self, body: &str, ts_ms: i64) -> Result<String, ExecApiError> {
        let req: OrderRequest = serde_json::from_str(body)
            .map_err(|e| ExecApiError::bad_request(format!("invalid JSON: {}", e)))?;
        let mut mgr = self.manager.lock().unwrap();
        mgr.submit(req, ts_ms)
            .map(|id| serde_json::json!({ "order_id": id }).to_string())
            .map_err(|e| ExecApiError::bad_request(e.to_string()))
    }

    /// GET /positions — returns current position summary grouped by instrument.
    pub fn handle_positions(&self) -> String {
        let mgr = self.manager.lock().unwrap();
        let mut positions: std::collections::HashMap<String, f64> =
            std::collections::HashMap::new();
        for o in mgr.open_orders() {
            *positions
                .entry(o.request.instrument.to_string())
                .or_insert(0.0) += o.request.quantity;
        }
        serde_json::json!({ "positions": positions }).to_string()
    }

    /// POST /webhooks/register — register a webhook URL for order events (stub).
    pub fn handle_register_webhook(&self, body: &str) -> Result<String, ExecApiError> {
        let val: serde_json::Value = serde_json::from_str(body)
            .map_err(|e| ExecApiError::bad_request(format!("invalid JSON: {}", e)))?;
        let url = val
            .get("url")
            .and_then(|u| u.as_str())
            .ok_or_else(|| ExecApiError::bad_request("missing 'url' field"))?;
        // In production: persist URL to webhook registry and deliver events.
        Ok(serde_json::json!({ "registered": url, "status": "ok" }).to_string())
    }

    /// POST /webhooks/deliver — deliver a webhook payload to a registered URL (stub).
    pub fn handle_deliver_webhook(&self, event: &str, order_id: &str) -> String {
        // In production: POST to registered URLs with retry logic.
        serde_json::json!({ "event": event, "order_id": order_id, "delivered": true }).to_string()
    }
}

#[cfg(test)]
mod tests {
    use domain::{InstrumentId, Side, Venue};

    use super::*;
    use crate::core::types::{OrderKind, TimeInForce};

    fn make_handler() -> ExecHttpHandler {
        ExecHttpHandler::new(Arc::new(Mutex::new(OrderManager::new())))
    }

    fn order_json() -> String {
        let req = OrderRequest {
            client_order_id: "c1".to_string(),
            instrument: InstrumentId::new(Venue::Crypto, "BTC-USD"),
            side: Side::Buy,
            quantity: 1.0,
            kind: OrderKind::Market,
            tif: TimeInForce::GTC,
            flags: vec![],
            strategy_id: "s1".to_string(),
            submitted_ts_ms: 1000,
        };
        serde_json::to_string(&req).unwrap()
    }

    #[test]
    fn health_returns_ok() {
        let h = make_handler();
        assert_eq!(h.handle_health(), r#"{"status":"ok"}"#);
    }

    #[test]
    fn list_empty_initially() {
        let h = make_handler();
        let resp = h.handle_list_orders();
        let v: serde_json::Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["orders"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn submit_then_list() {
        let h = make_handler();
        h.handle_submit(&order_json(), 1000).unwrap();
        let resp = h.handle_list_orders();
        let v: serde_json::Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["orders"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn cancel_not_found_error() {
        let h = make_handler();
        let err = h.handle_cancel("nonexistent", 1000).unwrap_err();
        assert_eq!(err.code, 404);
    }
}
