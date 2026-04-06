use std::sync::{Arc, Mutex};

use domain::{InstrumentId, Side, Venue};
use exec::api::grpc::{ExecGrpcService, ExecServiceRequest};
use exec::api::health::{HealthChecker, HealthStatus};
use exec::api::http::ExecHttpHandler;
use exec::api::ws::{WsEvent, WsEventBus, WsEventKind};
use exec::adapters::longbridge::{LongbridgeAdapter, LongbridgeConfig};
use exec::core::order::OrderManager;
use exec::core::types::{OrderKind, OrderRequest, TimeInForce};

fn make_order_request(client_id: &str) -> OrderRequest {
    OrderRequest {
        client_order_id: client_id.to_string(),
        instrument: InstrumentId::new(Venue::Crypto, "BTC-USD"),
        side: Side::Buy,
        quantity: 1.0,
        kind: OrderKind::Market,
        tif: TimeInForce::GTC,
        flags: vec![],
        strategy_id: "s1".to_string(),
        submitted_ts_ms: 1000,
    }
}

// ─── 1. gRPC service: submit via JSON → order created ────────────────────────

#[test]
fn grpc_submit_creates_order() {
    let mgr = Arc::new(Mutex::new(OrderManager::new()));
    let svc = ExecGrpcService::new(mgr.clone());

    let payload = serde_json::to_value(make_order_request("c1")).unwrap();
    let req = ExecServiceRequest { action: "submit".to_string(), payload };
    let resp = svc.handle(&req);

    assert!(resp.success, "expected success, got error: {:?}", resp.error);
    assert!(resp.data.get("order_id").is_some());

    // Verify order is in the manager
    let lock = mgr.lock().unwrap();
    assert_eq!(lock.open_orders().len(), 1);
}

// ─── 2. HTTP handler: list returns empty initially, submit + list returns 1 ──

#[test]
fn http_list_then_submit() {
    let handler = ExecHttpHandler::new(Arc::new(Mutex::new(OrderManager::new())));

    // Initially empty
    let list = handler.handle_list_orders();
    let v: serde_json::Value = serde_json::from_str(&list).unwrap();
    assert_eq!(v["orders"].as_array().unwrap().len(), 0);

    // Submit one
    let body = serde_json::to_string(&make_order_request("c2")).unwrap();
    let result = handler.handle_submit(&body, 1000);
    assert!(result.is_ok(), "submit failed: {:?}", result.err());

    // Now list has 1
    let list2 = handler.handle_list_orders();
    let v2: serde_json::Value = serde_json::from_str(&list2).unwrap();
    assert_eq!(v2["orders"].as_array().unwrap().len(), 1);
}

// ─── 3. WebSocket bus: publish event received by subscriber ──────────────────

#[test]
fn websocket_bus_publish_receive() {
    let mut bus = WsEventBus::new();
    let rx = bus.subscribe();

    let event = WsEvent {
        kind: WsEventKind::OrderSubmitted,
        payload: serde_json::json!({ "order_id": "o1" }),
        ts_ms: 1000,
    };
    bus.publish(event);

    let received = rx.try_recv().expect("expected event");
    assert_eq!(received.kind, WsEventKind::OrderSubmitted);
    assert_eq!(received.payload["order_id"], "o1");
    assert_eq!(received.ts_ms, 1000);
}

// ─── 4. LongbridgeAdapter: connect + submit returns order ID ─────────────────

#[test]
fn longbridge_connect_and_submit() {
    let config = LongbridgeConfig {
        api_key: "api_key".to_string(),
        app_key: "app_key".to_string(),
        region: "HK".to_string(),
    };
    let mut adapter = LongbridgeAdapter::new(config);

    assert!(!adapter.is_connected());
    adapter.connect().expect("connect should succeed");
    assert!(adapter.is_connected());

    let req = make_order_request("c3");
    let order_id = adapter.submit_order(&req).expect("submit should succeed");
    assert!(!order_id.is_empty(), "order_id should not be empty");
    assert!(order_id.contains("c3"), "order_id should reference client_order_id");
}
