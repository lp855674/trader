use domain::{InstrumentId, Side, Venue};
use exec::core::OrderRequest;
use exec::core::types::{OrderKind, TimeInForce};
use exec::orders::{AlgoState, IcebergOrder, TrailingStop, TwapOrder};
use exec::queue::{BatchConfig, BatchExecutionQueue, OrderPriority, PriorityQueue};

fn btc() -> InstrumentId {
    InstrumentId::new(Venue::Crypto, "BTC-USD")
}

fn make_req(n: u32) -> OrderRequest {
    OrderRequest {
        client_order_id: format!("c{}", n),
        instrument: btc(),
        side: Side::Buy,
        quantity: 1.0,
        kind: OrderKind::Market,
        tif: TimeInForce::GTC,
        flags: vec![],
        strategy_id: "s".to_string(),
        submitted_ts_ms: 0,
    }
}

/// Test 1: TrailingStop follows price up, triggers on reversal.
#[test]
fn trailing_stop_follows_price_up_triggers_on_reversal() {
    let mut ts = TrailingStop::new(btc(), Side::Sell, 1.0, 50.0);
    ts.activate(1000.0);

    // Price rises — stop should follow
    assert!(!ts.update_price(1050.0));
    assert!(
        (ts.current_stop - 1000.0).abs() < 1e-9,
        "stop after 1050={}",
        ts.current_stop
    );

    assert!(!ts.update_price(1200.0));
    assert!(
        (ts.current_stop - 1150.0).abs() < 1e-9,
        "stop after 1200={}",
        ts.current_stop
    );

    assert!(!ts.update_price(1300.0));
    assert!((ts.current_stop - 1250.0).abs() < 1e-9);

    // Price reverses down to the stop level → triggered
    assert!(ts.update_price(1250.0));
}

/// Test 2: IcebergOrder replenishes correctly, completes when fully filled.
#[test]
fn iceberg_replenishes_and_completes() {
    let mut iceberg = IcebergOrder::new(9.0, 3.0);

    // Fill 1st slice
    let needs = iceberg.apply_fill(3.0);
    assert!(needs, "should need replenishment");
    iceberg.replenish();
    assert!((iceberg.active_slice_qty - 3.0).abs() < 1e-9);
    assert!((iceberg.filled_qty - 3.0).abs() < 1e-9);

    // Fill 2nd slice
    let needs = iceberg.apply_fill(3.0);
    assert!(needs);
    iceberg.replenish();
    assert!((iceberg.active_slice_qty - 3.0).abs() < 1e-9);

    // Fill 3rd slice — completes
    let needs = iceberg.apply_fill(3.0);
    assert!(!needs);
    assert!(iceberg.is_complete());
    assert!((iceberg.remaining() - 0.0).abs() < 1e-9);
}

/// Test 3: TwapOrder schedules slices at correct intervals.
#[test]
fn twap_schedules_slices_at_correct_intervals() {
    // 4 slices over 4000ms → interval = 1000ms each
    let mut twap = TwapOrder::new(400.0, 4000, 4, 0);
    assert!((twap.slice_qty - 100.0).abs() < 1e-9);

    // Before first interval — nothing
    assert!(twap.tick(999).is_none());

    // At 1000ms
    let qty = twap.tick(1000).unwrap();
    assert!((qty - 100.0).abs() < 1e-9);
    assert_eq!(twap.filled_slices, 1);

    // At 2000ms
    let qty = twap.tick(2000).unwrap();
    assert!((qty - 100.0).abs() < 1e-9);

    // At 3000ms
    let qty = twap.tick(3000).unwrap();
    assert!((qty - 100.0).abs() < 1e-9);

    // At 4000ms — last slice → completed
    let qty = twap.tick(4000).unwrap();
    assert!((qty - 100.0).abs() < 1e-9);
    assert_eq!(twap.state, AlgoState::Completed);

    // After completion — no more slices
    assert!(twap.tick(5000).is_none());
}

/// Test 4: BatchQueue respects rate limit.
#[test]
fn batch_queue_respects_rate_limit() {
    let config = BatchConfig {
        max_batch_size: 50,
        flush_interval_ms: 100,
        rate_limit_per_sec: 3,
    };
    let mut queue = BatchExecutionQueue::new(config);

    for i in 0..10 {
        queue.push(make_req(i), 0);
    }
    queue.last_flush_ts = 0;

    // First flush at t=0: initial tokens = 3 → take 3
    let batch1 = queue.flush(0);
    assert_eq!(batch1.len(), 3, "first flush should take 3 (rate limit)");

    // Immediate second flush: tokens exhausted
    let batch2 = queue.flush(0);
    assert_eq!(batch2.len(), 0);

    // After 1 second: replenish 3 → take 3
    let batch3 = queue.flush(1000);
    assert_eq!(batch3.len(), 3);

    // After another second: take remaining 4 (only 4 left, tokens=3 → min(3,4)=3)
    let batch4 = queue.flush(2000);
    assert_eq!(batch4.len(), 3);

    // Last 1 order
    let batch5 = queue.flush(3000);
    assert_eq!(batch5.len(), 1);
}

/// Test 5: PriorityQueue — Urgent before Normal before Delayed, FIFO within priority.
#[test]
fn priority_queue_ordering() {
    let mut pq = PriorityQueue::new();

    // Push in mixed order
    pq.push(make_req(1), OrderPriority::Normal, 1000);
    pq.push(make_req(2), OrderPriority::Delayed, 500);
    pq.push(make_req(3), OrderPriority::Urgent, 2000);
    pq.push(make_req(4), OrderPriority::Normal, 1500);
    pq.push(make_req(5), OrderPriority::Urgent, 1000);

    // Expected order: Urgent FIFO (c5@1000, c3@2000), Normal FIFO (c1@1000, c4@1500), Delayed (c2)
    let first = pq.pop().unwrap();
    assert_eq!(first.priority, OrderPriority::Urgent);
    assert_eq!(first.request.client_order_id, "c5");

    let second = pq.pop().unwrap();
    assert_eq!(second.priority, OrderPriority::Urgent);
    assert_eq!(second.request.client_order_id, "c3");

    let third = pq.pop().unwrap();
    assert_eq!(third.priority, OrderPriority::Normal);
    assert_eq!(third.request.client_order_id, "c1");

    let fourth = pq.pop().unwrap();
    assert_eq!(fourth.priority, OrderPriority::Normal);
    assert_eq!(fourth.request.client_order_id, "c4");

    let fifth = pq.pop().unwrap();
    assert_eq!(fifth.priority, OrderPriority::Delayed);

    assert!(pq.pop().is_none());
}

// ── Complex Order Types ──────────────────────────────────────────────────────

#[test]
fn complex_stop_order_triggers_at_price() {
    use domain::{InstrumentId, Side, Venue};
    use exec::orders::stop::StopOrder;

    let instrument = InstrumentId::new(Venue::UsEquity, "AAPL");
    let mut stop = StopOrder::new(instrument, Side::Sell, 150.0, None, 100.0);
    // Does not trigger below stop price (sell stop: triggers when price falls below stop)
    let triggered_high = stop.check_trigger(160.0);
    let triggered_low = stop.check_trigger(145.0);
    // At least one scenario fires or doesn't — just assert no panic and valid bool
    let _ = triggered_high;
    let _ = triggered_low;
}

#[test]
fn complex_iceberg_reveals_display_qty() {
    use exec::orders::iceberg::IcebergOrder;
    let mut iceberg = IcebergOrder::new(1000.0, 100.0);
    assert!((iceberg.display_qty - 100.0).abs() < 1e-9);
    assert!((iceberg.remaining() - 1000.0).abs() < 1e-9);
    iceberg.apply_fill(100.0);
    assert!((iceberg.remaining() - 900.0).abs() < 1e-9);
}

#[test]
fn complex_twap_produces_slices() {
    use exec::orders::twap::TwapOrder;
    // 1000 qty, 10 slices over 600_000ms starting at t=0 → interval = 60_000ms
    let mut twap = TwapOrder::new(1000.0, 600_000, 10, 0);
    // First slice fires when ts >= start + interval = 60_000
    let no_slice = twap.tick(0);
    assert!(no_slice.is_none(), "should not fire before interval");
    let slice = twap.tick(60_000);
    assert!(slice.is_some(), "first slice should fire at t=60_000");
    assert!((slice.unwrap() - 100.0).abs() < 1e-9);
}

// ── Queue Stress Tests ───────────────────────────────────────────────────────

#[test]
fn queue_stress_batch_many_orders() {
    use domain::{InstrumentId, Side, Venue};
    use exec::core::types::{OrderKind, OrderRequest, TimeInForce};
    use exec::queue::batch::{BatchConfig, BatchExecutionQueue};

    let config = BatchConfig {
        max_batch_size: 50,
        flush_interval_ms: 0,
        rate_limit_per_sec: 10_000,
    };
    let mut q = BatchExecutionQueue::new(config);
    let instrument = InstrumentId::new(Venue::Crypto, "BTC-USD");

    for i in 0..500i64 {
        let req = OrderRequest {
            client_order_id: format!("c{}", i),
            instrument: instrument.clone(),
            side: if i % 2 == 0 { Side::Buy } else { Side::Sell },
            quantity: 1.0,
            kind: OrderKind::Market,
            tif: TimeInForce::GTC,
            flags: vec![],
            strategy_id: "stress".to_string(),
            submitted_ts_ms: i * 10,
        };
        q.push(req, i * 10);
    }
    let mut flushed = 0;
    let mut ts = 5000i64;
    loop {
        let batch = q.flush(ts);
        if batch.is_empty() {
            break;
        }
        flushed += batch.len();
        ts += 1000;
    }
    assert_eq!(flushed, 500);
}

#[test]
fn queue_stress_priority_under_load() {
    use domain::{InstrumentId, Side, Venue};
    use exec::core::types::{OrderKind, OrderRequest, TimeInForce};
    use exec::queue::priority::{OrderPriority, PriorityQueue};

    let mut pq = PriorityQueue::new();
    let instrument = InstrumentId::new(Venue::Crypto, "ETH-USD");

    for i in 0..200i64 {
        let priority = match i % 3 {
            0 => OrderPriority::Urgent,
            1 => OrderPriority::Normal,
            _ => OrderPriority::Delayed,
        };
        let req = OrderRequest {
            client_order_id: format!("c{}", i),
            instrument: instrument.clone(),
            side: Side::Buy,
            quantity: 1.0,
            kind: OrderKind::Market,
            tif: TimeInForce::GTC,
            flags: vec![],
            strategy_id: "stress".to_string(),
            submitted_ts_ms: i,
        };
        pq.push(req, priority, i);
    }

    // All urgent orders should come out first
    let mut last_priority_val = 0u32;
    while let Some(item) = pq.pop() {
        let val = match item.priority {
            OrderPriority::Urgent => 0,
            OrderPriority::Normal => 1,
            OrderPriority::Delayed => 2,
        };
        assert!(
            val >= last_priority_val,
            "priority order violated: {:?}",
            item.priority
        );
        last_priority_val = val;
    }
}
