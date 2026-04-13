use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::core::order::OrderManager;
use crate::core::types::OrderRequest;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecServiceRequest {
    pub action: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecServiceResponse {
    pub success: bool,
    pub data: serde_json::Value,
    pub error: Option<String>,
}

pub struct ExecGrpcService {
    manager: Arc<Mutex<OrderManager>>,
}

impl ExecGrpcService {
    pub fn new(manager: Arc<Mutex<OrderManager>>) -> Self {
        Self { manager }
    }

    pub fn handle(&self, req: &ExecServiceRequest) -> ExecServiceResponse {
        match req.action.as_str() {
            "submit" => {
                let order_req: OrderRequest = match serde_json::from_value(req.payload.clone()) {
                    Ok(r) => r,
                    Err(e) => {
                        return ExecServiceResponse {
                            success: false,
                            data: serde_json::Value::Null,
                            error: Some(format!("invalid payload: {}", e)),
                        };
                    }
                };
                let ts_ms = order_req.submitted_ts_ms;
                let mut mgr = self.manager.lock().unwrap();
                match mgr.submit(order_req, ts_ms) {
                    Ok(id) => ExecServiceResponse {
                        success: true,
                        data: serde_json::json!({ "order_id": id }),
                        error: None,
                    },
                    Err(e) => ExecServiceResponse {
                        success: false,
                        data: serde_json::Value::Null,
                        error: Some(e.to_string()),
                    },
                }
            }
            "cancel" => {
                let order_id = match req.payload.get("order_id").and_then(|v| v.as_str()) {
                    Some(id) => id.to_string(),
                    None => {
                        return ExecServiceResponse {
                            success: false,
                            data: serde_json::Value::Null,
                            error: Some("missing order_id".to_string()),
                        };
                    }
                };
                let ts_ms = req
                    .payload
                    .get("ts_ms")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let mut mgr = self.manager.lock().unwrap();
                match mgr.cancel(&order_id, ts_ms) {
                    Ok(()) => ExecServiceResponse {
                        success: true,
                        data: serde_json::json!({ "cancelled": order_id }),
                        error: None,
                    },
                    Err(e) => ExecServiceResponse {
                        success: false,
                        data: serde_json::Value::Null,
                        error: Some(e.to_string()),
                    },
                }
            }
            "list" => {
                let mgr = self.manager.lock().unwrap();
                let orders: Vec<serde_json::Value> = mgr
                    .open_orders()
                    .iter()
                    .map(|o| serde_json::json!({ "id": o.id, "state": format!("{:?}", o.state) }))
                    .collect();
                ExecServiceResponse {
                    success: true,
                    data: serde_json::json!({ "orders": orders }),
                    error: None,
                }
            }
            "positions" => {
                // Position queries: return current open positions summary.
                let mgr = self.manager.lock().unwrap();
                let open = mgr.open_orders();
                let positions: std::collections::HashMap<String, f64> = {
                    let mut map = std::collections::HashMap::new();
                    for o in &open {
                        *map.entry(o.request.instrument.to_string()).or_insert(0.0) +=
                            o.request.quantity;
                    }
                    map
                };
                ExecServiceResponse {
                    success: true,
                    data: serde_json::json!({ "positions": positions }),
                    error: None,
                }
            }
            other => ExecServiceResponse {
                success: false,
                data: serde_json::Value::Null,
                error: Some(format!("unknown action: {}", other)),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use domain::{InstrumentId, Side, Venue};

    use super::*;
    use crate::core::types::{OrderKind, TimeInForce};

    fn make_service() -> ExecGrpcService {
        ExecGrpcService::new(Arc::new(Mutex::new(OrderManager::new())))
    }

    fn order_payload() -> serde_json::Value {
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
        serde_json::to_value(req).unwrap()
    }

    #[test]
    fn submit_creates_order() {
        let svc = make_service();
        let req = ExecServiceRequest {
            action: "submit".to_string(),
            payload: order_payload(),
        };
        let resp = svc.handle(&req);
        assert!(resp.success, "{:?}", resp.error);
        assert!(resp.data.get("order_id").is_some());
    }

    #[test]
    fn list_returns_open_orders() {
        let svc = make_service();
        let submit = ExecServiceRequest {
            action: "submit".to_string(),
            payload: order_payload(),
        };
        svc.handle(&submit);
        let list = ExecServiceRequest {
            action: "list".to_string(),
            payload: serde_json::Value::Null,
        };
        let resp = svc.handle(&list);
        assert!(resp.success);
        let orders = resp.data["orders"].as_array().unwrap();
        assert_eq!(orders.len(), 1);
    }

    #[test]
    fn unknown_action_returns_error() {
        let svc = make_service();
        let req = ExecServiceRequest {
            action: "unknown".to_string(),
            payload: serde_json::Value::Null,
        };
        let resp = svc.handle(&req);
        assert!(!resp.success);
    }
}
