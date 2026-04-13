use std::sync::Arc;

use domain::{InstrumentId, Side, Venue};
use exec::core::types::{OrderKind, TimeInForce};
use exec::core::{
    ExecPositionManager, FillRecord, OrderEvent, OrderManager, OrderRequest, OrderState,
    TaxLotMethod,
};
use tokio::sync::Mutex;

fn btc() -> InstrumentId {
    InstrumentId::new(Venue::Crypto, "BTC-USD")
}

fn make_req(client_id: &str, qty: f64) -> OrderRequest {
    OrderRequest {
        client_order_id: client_id.to_string(),
        instrument: btc(),
        side: Side::Buy,
        quantity: qty,
        kind: OrderKind::Market,
        tif: TimeInForce::GTC,
        flags: vec![],
        strategy_id: "test".to_string(),
        submitted_ts_ms: 1000,
    }
}

/// Test 1: Full lifecycle — submit → partial fill → full fill → position updated
#[test]
fn full_lifecycle() {
    let mut mgr = OrderManager::new();
    let mut pos_mgr = ExecPositionManager::new(TaxLotMethod::Fifo);

    let req = make_req("lifecycle_1", 10.0);
    let order_id = mgr.submit(req, 1000).unwrap();

    // Submit → Submitted
    mgr.apply_event(&order_id, OrderEvent::Submit, 1001)
        .unwrap();
    assert_eq!(mgr.get(&order_id).unwrap().state, OrderState::Submitted);

    // Partial fill
    mgr.apply_event(
        &order_id,
        OrderEvent::PartialFill {
            qty: 4.0,
            price: 100.0,
        },
        1002,
    )
    .unwrap();
    let order = mgr.get(&order_id).unwrap();
    assert!(
        matches!(order.state, OrderState::PartiallyFilled { filled_qty } if (filled_qty - 4.0).abs() < 1e-9)
    );

    let fill1 = FillRecord {
        order_id: order_id.clone(),
        instrument: btc(),
        side: Side::Buy,
        qty: 4.0,
        price: 100.0,
        commission: 0.0,
        ts_ms: 1002,
    };
    pos_mgr.apply_fill(&fill1);

    // Full fill
    mgr.apply_event(
        &order_id,
        OrderEvent::Fill {
            qty: 6.0,
            price: 105.0,
        },
        1003,
    )
    .unwrap();
    assert_eq!(mgr.get(&order_id).unwrap().state, OrderState::Filled);

    let fill2 = FillRecord {
        order_id: order_id.clone(),
        instrument: btc(),
        side: Side::Buy,
        qty: 6.0,
        price: 105.0,
        commission: 0.0,
        ts_ms: 1003,
    };
    pos_mgr.apply_fill(&fill2);

    // Verify position
    let pos = pos_mgr.get(&btc()).unwrap();
    assert!((pos.net_qty - 10.0).abs() < 1e-9);
    // avg_cost = (4*100 + 6*105) / 10 = (400+630)/10 = 103
    assert!(
        (pos.avg_cost - 103.0).abs() < 1e-6,
        "avg_cost={}",
        pos.avg_cost
    );
}

/// Test 2: Concurrent order submission using Arc<Mutex<OrderManager>>
#[tokio::test]
async fn concurrent_order_submission() {
    let mgr = Arc::new(Mutex::new(OrderManager::new()));
    let mut handles = Vec::new();

    for i in 0..4 {
        let mgr_clone = Arc::clone(&mgr);
        let handle = tokio::spawn(async move {
            let req = make_req(&format!("concurrent_{}", i), 1.0);
            let mut guard = mgr_clone.lock().await;
            guard.submit(req, 1000 + i as i64).unwrap()
        });
        handles.push(handle);
    }

    let mut ids = Vec::new();
    for h in handles {
        ids.push(h.await.unwrap());
    }

    // All 4 orders should have unique IDs
    let unique: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(unique.len(), 4);

    let guard = mgr.lock().await;
    assert_eq!(guard.open_orders().len(), 4);
}

/// Test 3: Idempotency — same client_order_id submitted twice returns same order_id
#[test]
fn idempotency_same_client_order_id() {
    let mut mgr = OrderManager::new();
    let req1 = make_req("idem_client", 1.0);
    let req2 = make_req("idem_client", 1.0);
    let id1 = mgr.submit(req1, 1000).unwrap();
    let id2 = mgr.submit(req2, 1001).unwrap();
    assert_eq!(id1, id2, "Idempotent submit should return same order id");
    assert_eq!(mgr.open_orders().len(), 1);
}

/// Test 4: Cancel after partial fill correctly closes position (stops further fills)
#[test]
fn cancel_after_partial_fill() {
    let mut mgr = OrderManager::new();
    let mut pos_mgr = ExecPositionManager::new(TaxLotMethod::Fifo);

    let req = make_req("cancel_partial", 10.0);
    let order_id = mgr.submit(req, 1000).unwrap();
    mgr.apply_event(&order_id, OrderEvent::Submit, 1001)
        .unwrap();
    mgr.apply_event(
        &order_id,
        OrderEvent::PartialFill {
            qty: 3.0,
            price: 100.0,
        },
        1002,
    )
    .unwrap();

    // Apply fill to position
    pos_mgr.apply_fill(&FillRecord {
        order_id: order_id.clone(),
        instrument: btc(),
        side: Side::Buy,
        qty: 3.0,
        price: 100.0,
        commission: 0.0,
        ts_ms: 1002,
    });

    // Cancel the order
    mgr.cancel(&order_id, 1003).unwrap();
    assert_eq!(mgr.get(&order_id).unwrap().state, OrderState::Cancelled);
    assert!(mgr.get(&order_id).unwrap().state.is_terminal());

    // No further fills can be applied
    let result = mgr.apply_event(
        &order_id,
        OrderEvent::Fill {
            qty: 7.0,
            price: 100.0,
        },
        1004,
    );
    assert!(result.is_err());

    // Position reflects only partial fill
    let pos = pos_mgr.get(&btc()).unwrap();
    assert!((pos.net_qty - 3.0).abs() < 1e-9);
}
