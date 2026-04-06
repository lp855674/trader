use std::collections::HashMap;

use domain::{InstrumentId, Side, Venue};
use exec::core::order::{Order, OrderEvent, OrderManager};
use exec::core::position::{ExecPositionManager, FillRecord, TaxLotMethod};
use exec::core::types::{OrderKind, OrderRequest, TimeInForce};
use exec::persistence::fills::FillRepository;
use exec::persistence::orders::OrderRepository;
use exec::persistence::positions::PositionRepository;
use exec::persistence::snapshot::SnapshotManager;
use exec::persistence::wal::{WalEntry, WalLog, WalRecovery};
use exec::persistence::index::QueryIndex;

fn btc() -> InstrumentId {
    InstrumentId::new(Venue::Crypto, "BTC-USD")
}

fn make_order(client_id: &str) -> Order {
    let req = OrderRequest {
        client_order_id: client_id.to_string(),
        instrument: btc(),
        side: Side::Buy,
        quantity: 1.0,
        kind: OrderKind::Market,
        tif: TimeInForce::GTC,
        flags: vec![],
        strategy_id: "s1".to_string(),
        submitted_ts_ms: 1000,
    };
    let mut o = Order::new(req, 1000);
    let _ = o.transition(OrderEvent::Submit, 1001);
    o
}

fn fill(order_id: &str, ts_ms: i64) -> FillRecord {
    FillRecord {
        order_id: order_id.to_string(),
        instrument: btc(),
        side: Side::Buy,
        qty: 1.0,
        price: 100.0,
        commission: 0.5,
        ts_ms,
    }
}

// ─── 1. WAL replay reconstructs OrderManager state ───────────────────────────

#[test]
fn wal_replay_reconstructs_order_manager() {
    let mut wal = WalLog::new();
    let o1 = make_order("c1");
    let o2 = make_order("c2");
    let oid1 = o1.id.clone();
    let oid2 = o2.id.clone();

    wal.append(WalEntry::OrderSubmitted(o1));
    wal.append(WalEntry::OrderSubmitted(o2));
    wal.append(WalEntry::OrderCancelled(oid2.clone()));

    let (order_mgr, _) = WalRecovery::replay(&wal.entries);
    assert!(order_mgr.get(&oid1).is_some(), "order1 should exist");
    assert!(order_mgr.get(&oid2).is_some(), "order2 should exist");
    let o2_state = &order_mgr.get(&oid2).unwrap().state;
    assert!(
        matches!(o2_state, exec::core::order::OrderState::Cancelled),
        "order2 should be cancelled"
    );
}

// ─── 2. Snapshot → JSON → restore produces identical OrderManager ─────────────

#[test]
fn snapshot_json_restore_identical() {
    let mut order_mgr = OrderManager::new();
    let o = make_order("c3");
    let oid = o.id.clone();
    order_mgr.orders.insert(oid.clone(), o);

    let pos_mgr = ExecPositionManager::new(TaxLotMethod::Fifo);
    let mut snap_mgr = SnapshotManager::new(10);
    snap_mgr.take(&order_mgr, &pos_mgr, 5000);

    let snap = snap_mgr.latest().unwrap();
    let json = SnapshotManager::to_json(snap).unwrap();
    let restored_snap = SnapshotManager::from_json(&json).unwrap();
    let restored_mgr = SnapshotManager::restore_orders(&restored_snap);

    assert!(restored_mgr.get(&oid).is_some(), "order should exist after restore");
    assert_eq!(restored_mgr.orders.len(), order_mgr.orders.len());
}

// ─── 3. FillRepository dedup prevents double-insert ──────────────────────────

#[test]
fn fill_repository_dedup() {
    let mut repo = FillRepository::new();
    let f = fill("o1", 1000);
    assert!(repo.insert(f.clone()), "first insert should succeed");
    assert!(!repo.insert(f), "duplicate insert should fail");
    assert_eq!(repo.count(), 1);
}

// ─── 4. PositionRepository history_since returns correct slice ────────────────

#[test]
fn position_repository_history_since() {
    let mut repo = PositionRepository::new(10);
    let mgr = ExecPositionManager::new(TaxLotMethod::Fifo);
    repo.take_snapshot(&mgr, 1000);
    repo.take_snapshot(&mgr, 2000);
    repo.take_snapshot(&mgr, 3000);
    repo.take_snapshot(&mgr, 4000);

    let hist = repo.history_since(2000);
    assert_eq!(hist.len(), 3, "should return snapshots at 2000, 3000, 4000");
    assert_eq!(hist[0].0, 2000);
    assert_eq!(hist[1].0, 3000);
    assert_eq!(hist[2].0, 4000);
}

// ─── 5. QueryIndex lookup_by_instrument returns correct order IDs ─────────────

#[test]
fn query_index_lookup_by_instrument() {
    let mut idx = QueryIndex::new();
    let o1 = make_order("c4");
    let o2 = make_order("c5");
    let id1 = o1.id.clone();
    let id2 = o2.id.clone();
    idx.index_order(&o1);
    idx.index_order(&o2);

    let ids = idx.lookup_by_instrument("CRYPTO:BTC-USD");
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&id1));
    assert!(ids.contains(&id2));

    // Non-existent instrument returns empty
    let empty = idx.lookup_by_instrument("UNKNOWN:XYZ");
    assert!(empty.is_empty());
}

// ── Data Corruption Handling ────────────────────────────────────────────────

#[test]
fn data_corruption_handling_invalid_json() {
    use exec::persistence::snapshot::SnapshotManager;
    let result = SnapshotManager::from_json("{ this is not valid json }");
    assert!(result.is_err(), "corrupted JSON should return Err");
}

#[test]
fn snapshot_verify_all_passes_for_valid_snapshots() {
    use domain::{InstrumentId, Side, Venue};
    use exec::core::order::OrderManager;
    use exec::core::types::{OrderKind, OrderRequest, TimeInForce};
    use exec::persistence::snapshot::SnapshotManager;

    use exec::core::position::{ExecPositionManager, TaxLotMethod};
    let mut mgr = OrderManager::new();
    let pos_mgr = ExecPositionManager::new(TaxLotMethod::Fifo);
    let req = OrderRequest {
        client_order_id: "c1".to_string(),
        instrument: InstrumentId::new(Venue::Crypto, "BTC-USD"),
        side: Side::Buy,
        quantity: 1.0,
        kind: OrderKind::Limit { price: 100.0 },
        tif: TimeInForce::GTC,
        flags: vec![],
        strategy_id: "s1".to_string(),
        submitted_ts_ms: 1000,
    };
    mgr.submit(req, 1000).unwrap();

    let mut sm = SnapshotManager::new(5);
    sm.take(&mgr, &pos_mgr, 1000);
    sm.take(&mgr, &pos_mgr, 2000);
    let errors = sm.verify_all();
    assert!(errors.is_empty(), "all valid snapshots should pass verification: {:?}", errors);
}

#[test]
fn incremental_backup_returns_only_new_snapshots() {
    use exec::core::order::OrderManager;
    use exec::core::position::{ExecPositionManager, TaxLotMethod};
    use exec::persistence::snapshot::SnapshotManager;

    let mgr = OrderManager::new();
    let pos_mgr = ExecPositionManager::new(TaxLotMethod::Fifo);
    let mut sm = SnapshotManager::new(10);
    sm.take(&mgr, &pos_mgr, 1000);
    sm.take(&mgr, &pos_mgr, 2000);
    sm.take(&mgr, &pos_mgr, 3000);

    let incremental = sm.incremental_since(1500);
    assert_eq!(incremental.len(), 2);
    assert!(incremental.iter().all(|s| s.ts_ms > 1500));
}

#[test]
fn wal_durability_status() {
    use domain::{InstrumentId, Side, Venue};
    use exec::core::order::OrderManager;
    use exec::core::types::{OrderKind, OrderRequest, TimeInForce};
    use exec::persistence::wal::{WalEntry, WalLog};

    let mut wal = WalLog::new();
    // Build a real Order to use with WalEntry::OrderSubmitted
    let mut mgr = OrderManager::new();
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
    let id = mgr.submit(req.clone(), 1000).unwrap();
    let order = mgr.get(&id).unwrap().clone();
    wal.append(WalEntry::OrderSubmitted(order));
    wal.append(WalEntry::OrderCancelled("c99_1000".to_string()));
    let (total, unchecked, durable) = wal.durability_status();
    assert_eq!(total, 2);
    assert_eq!(unchecked, 2);
    assert!(durable);
    wal.checkpoint(2000);
    let (_, unchecked2, _) = wal.durability_status();
    assert_eq!(unchecked2, 0);
}
