use broker::BrokerKind;
use runtime::{CancellationFlag, LiveRuntime, LiveRuntimeSettings};
use rust_decimal::Decimal;
use storage::{Db, RuntimePositionSnapshotCommand, SystemLogFilter};

#[tokio::test]
async fn live_runtime_starts_reports_broker_status_and_stops_without_orders() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let settings = LiveRuntimeSettings {
        run_id: "live-1".to_string(),
        broker_kind: BrokerKind::Futu,
        account_id: "live-account".to_string(),
        base_currency: "USD".to_string(),
        initial_cash: dec("25000"),
        broker_snapshot_interval_ms: None,
    };
    let live = LiveRuntime::new(db.clone(), settings);
    let cancel = CancellationFlag::default();
    let task_cancel = cancel.clone();

    let handle = tokio::spawn(async move { live.run(task_cancel).await.unwrap() });

    wait_for_status(&db, "live-1", "running").await;
    let status = LiveRuntime::new(
        db.clone(),
        LiveRuntimeSettings {
            run_id: "live-1".to_string(),
            broker_kind: BrokerKind::Futu,
            account_id: "live-account".to_string(),
            base_currency: "USD".to_string(),
            initial_cash: dec("25000"),
            broker_snapshot_interval_ms: None,
        },
    )
    .broker_status()
    .await
    .unwrap();
    assert_eq!(status.kind, BrokerKind::Futu);
    assert!(status.connected);
    assert!(db.list_orders("live-1").await.unwrap().is_empty());

    let events = db.list_events_by_source("live-1").await.unwrap();
    assert!(events.iter().any(|event| event.category == "live.started"));
    let cash_snapshot = db
        .get_latest_cash_snapshot("live-1", Some("USD"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(cash_snapshot.cash, "25000");
    assert_eq!(cash_snapshot.available_cash, "25000");

    cancel.cancel();
    handle.await.unwrap();

    let run = db.get_strategy_run("live-1").await.unwrap().unwrap();
    assert_eq!(run.status, "stopped");
    let events = db.list_events_by_source("live-1").await.unwrap();
    assert!(events.iter().any(|event| event.category == "live.stopped"));
}

#[tokio::test]
async fn live_runtime_periodically_records_broker_reported_cash_snapshot() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let live = LiveRuntime::new(
        db.clone(),
        LiveRuntimeSettings {
            run_id: "live-broker-snapshot".to_string(),
            broker_kind: BrokerKind::Binance,
            account_id: "live-account".to_string(),
            base_currency: "USDT".to_string(),
            initial_cash: dec("25000"),
            broker_snapshot_interval_ms: Some(5),
        },
    );
    let cancel = CancellationFlag::default();
    let task_cancel = cancel.clone();
    let handle = tokio::spawn(async move { live.run(task_cancel).await.unwrap() });

    wait_for_latest_cash(&db, "live-broker-snapshot", "USDT", "100000").await;

    cancel.cancel();
    handle.await.unwrap();

    let snapshots = db
        .list_cash_snapshots("live-broker-snapshot")
        .await
        .unwrap();
    assert!(snapshots.len() >= 2);
}

#[tokio::test]
async fn live_runtime_periodically_records_broker_reported_position_snapshot() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let live = LiveRuntime::new(
        db.clone(),
        LiveRuntimeSettings {
            run_id: "live-broker-position".to_string(),
            broker_kind: BrokerKind::Binance,
            account_id: "live-account".to_string(),
            base_currency: "USDT".to_string(),
            initial_cash: dec("25000"),
            broker_snapshot_interval_ms: Some(5),
        },
    );
    let cancel = CancellationFlag::default();
    let task_cancel = cancel.clone();
    let handle = tokio::spawn(async move { live.run(task_cancel).await.unwrap() });

    wait_for_latest_position(
        &db,
        "live-broker-position",
        "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP",
        "long",
        "0.5",
    )
    .await;

    cancel.cancel();
    handle.await.unwrap();

    let snapshot = db
        .get_latest_position_snapshot(
            "live-broker-position",
            "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP",
            Some("long"),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.mark_price.as_deref(), Some("65000"));
    assert_eq!(snapshot.unrealized_pnl.as_deref(), Some("12.5"));
}

#[tokio::test]
async fn live_runtime_emits_reconciliation_drift_when_broker_cash_differs_from_runtime_cash() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let live = LiveRuntime::new(
        db.clone(),
        LiveRuntimeSettings {
            run_id: "live-cash-drift".to_string(),
            broker_kind: BrokerKind::Binance,
            account_id: "live-account".to_string(),
            base_currency: "USDT".to_string(),
            initial_cash: dec("25000"),
            broker_snapshot_interval_ms: Some(5),
        },
    );
    let cancel = CancellationFlag::default();
    let task_cancel = cancel.clone();
    let handle = tokio::spawn(async move { live.run(task_cancel).await.unwrap() });

    wait_for_risk_event(&db, "live-cash-drift", "reconciliation_drift").await;

    cancel.cancel();
    handle.await.unwrap();

    let drift_events = db.list_risk_events("live-cash-drift").await.unwrap();
    let cash_drift = drift_events
        .iter()
        .find(|event| event.reason.as_deref() == Some("cash_total_drift"))
        .unwrap();
    assert_eq!(cash_drift.account_id.as_deref(), Some("live-account"));
    assert_eq!(cash_drift.risk_type, "reconciliation_drift");
    assert_eq!(cash_drift.decision, "rejected");
    assert_eq!(cash_drift.threshold.as_deref(), Some("0"));
    assert_eq!(cash_drift.observed_value.as_deref(), Some("75000"));
}

#[tokio::test]
async fn live_runtime_emits_reconciliation_drift_when_broker_position_is_missing_from_runtime() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let live = LiveRuntime::new(
        db.clone(),
        LiveRuntimeSettings {
            run_id: "live-position-drift".to_string(),
            broker_kind: BrokerKind::Binance,
            account_id: "live-account".to_string(),
            base_currency: "USDT".to_string(),
            initial_cash: dec("100000"),
            broker_snapshot_interval_ms: Some(5),
        },
    );
    let cancel = CancellationFlag::default();
    let task_cancel = cancel.clone();
    let handle = tokio::spawn(async move { live.run(task_cancel).await.unwrap() });

    wait_for_risk_event(&db, "live-position-drift", "reconciliation_drift").await;

    cancel.cancel();
    handle.await.unwrap();

    let drift_events = db.list_risk_events("live-position-drift").await.unwrap();
    assert_eq!(drift_events.len(), 1);
    assert_eq!(drift_events[0].account_id.as_deref(), Some("live-account"));
    assert_eq!(
        drift_events[0].symbol.as_deref(),
        Some("CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP")
    );
    assert_eq!(drift_events[0].risk_type, "reconciliation_drift");
    assert_eq!(drift_events[0].decision, "rejected");
    assert_eq!(
        drift_events[0].reason.as_deref(),
        Some("position_missing_runtime")
    );
    assert_eq!(drift_events[0].threshold.as_deref(), Some("0"));
    assert_eq!(drift_events[0].observed_value.as_deref(), Some("0.5"));
}

#[tokio::test]
async fn live_runtime_emits_reconciliation_drift_when_runtime_position_qty_differs_from_broker() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let symbol = "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP";
    let live = LiveRuntime::new(
        db.clone(),
        LiveRuntimeSettings {
            run_id: "live-position-qty-drift".to_string(),
            broker_kind: BrokerKind::Binance,
            account_id: "live-account".to_string(),
            base_currency: "USDT".to_string(),
            initial_cash: dec("100000"),
            broker_snapshot_interval_ms: Some(100),
        },
    );
    let cancel = CancellationFlag::default();
    let task_cancel = cancel.clone();
    let handle = tokio::spawn(async move { live.run(task_cancel).await.unwrap() });

    wait_for_status(&db, "live-position-qty-drift", "running").await;
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let runtime_position_ts_ms = chrono::Utc::now().timestamp_millis();
    db.record_runtime_position_snapshot(RuntimePositionSnapshotCommand {
        run_id: "live-position-qty-drift".to_string(),
        ts_ms: runtime_position_ts_ms,
        symbol: symbol.to_string(),
        position_side: "long".to_string(),
        qty: dec("0.25"),
        available_qty: dec("0.25"),
        avg_price: dec("65000"),
        mark_price: Some(dec("65000")),
        currency: "USDT".to_string(),
    })
    .await
    .unwrap();

    wait_for_risk_event_reason(
        &db,
        "live-position-qty-drift",
        "reconciliation_drift",
        "position_qty_drift",
    )
    .await;

    cancel.cancel();
    handle.await.unwrap();

    let drift_events = db
        .list_risk_events("live-position-qty-drift")
        .await
        .unwrap();
    let qty_drift = drift_events
        .iter()
        .find(|event| event.reason.as_deref() == Some("position_qty_drift"))
        .unwrap();
    assert_eq!(qty_drift.account_id.as_deref(), Some("live-account"));
    assert_eq!(qty_drift.symbol.as_deref(), Some(symbol));
    assert_eq!(qty_drift.reason.as_deref(), Some("position_qty_drift"));
    assert_eq!(qty_drift.threshold.as_deref(), Some("0"));
    assert_eq!(qty_drift.observed_value.as_deref(), Some("0.25"));
}

#[tokio::test]
async fn live_runtime_records_source_system_logs_for_snapshots_and_reconciliation() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let live = LiveRuntime::new(
        db.clone(),
        LiveRuntimeSettings {
            run_id: "live-system-logs".to_string(),
            broker_kind: BrokerKind::Binance,
            account_id: "live-account".to_string(),
            base_currency: "USDT".to_string(),
            initial_cash: dec("25000"),
            broker_snapshot_interval_ms: Some(5),
        },
    );
    let cancel = CancellationFlag::default();
    let task_cancel = cancel.clone();
    let handle = tokio::spawn(async move { live.run(task_cancel).await.unwrap() });

    wait_for_system_log(&db, "live-system-logs", "runtime.reconciliation").await;

    cancel.cancel();
    handle.await.unwrap();

    let logs = db.list_system_logs(Some("live-system-logs")).await.unwrap();
    assert!(logs.iter().any(|log| {
        log.level == "INFO" && log.target == "runtime.live" && log.message == "live.started"
    }));
    assert!(logs.iter().any(|log| {
        log.level == "INFO" && log.target == "runtime.live" && log.message == "live.stopped"
    }));
    assert!(logs.iter().any(|log| {
        log.level == "INFO"
            && log.target == "runtime.broker_snapshot"
            && log.message == "broker.snapshot.cash"
    }));
    assert!(logs.iter().any(|log| {
        log.level == "INFO"
            && log.target == "runtime.broker_snapshot"
            && log.message == "broker.snapshot.position"
    }));
    assert!(logs.iter().any(|log| {
        log.level == "WARN"
            && log.target == "runtime.reconciliation"
            && log.message == "reconciliation.drift"
            && log
                .fields_json
                .as_deref()
                .is_some_and(|fields| fields.contains("cash_total_drift"))
    }));
}

fn dec(value: &str) -> Decimal {
    value.parse().unwrap()
}

async fn wait_for_status(db: &Db, run_id: &str, expected: &str) {
    for _ in 0..50 {
        if let Some(run) = db.get_strategy_run(run_id).await.unwrap()
            && run.status == expected
        {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("run {run_id} did not reach {expected}");
}

async fn wait_for_latest_cash(db: &Db, run_id: &str, currency: &str, expected_cash: &str) {
    for _ in 0..50 {
        if let Some(snapshot) = db
            .get_latest_cash_snapshot(run_id, Some(currency))
            .await
            .unwrap()
            && snapshot.cash == expected_cash
        {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("{run_id} latest {currency} cash did not reach {expected_cash}");
}

async fn wait_for_latest_position(
    db: &Db,
    run_id: &str,
    symbol: &str,
    position_side: &str,
    expected_qty: &str,
) {
    for _ in 0..50 {
        if let Some(snapshot) = db
            .get_latest_position_snapshot(run_id, symbol, Some(position_side))
            .await
            .unwrap()
            && snapshot.qty == expected_qty
        {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("{run_id} latest {symbol} {position_side} position did not reach {expected_qty}");
}

async fn wait_for_risk_event(db: &Db, run_id: &str, risk_type: &str) {
    for _ in 0..50 {
        if db
            .list_risk_events(run_id)
            .await
            .unwrap()
            .iter()
            .any(|event| event.risk_type == risk_type)
        {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("{run_id} did not emit risk event {risk_type}");
}

async fn wait_for_risk_event_reason(db: &Db, run_id: &str, risk_type: &str, reason: &str) {
    for _ in 0..50 {
        if db
            .list_risk_events(run_id)
            .await
            .unwrap()
            .iter()
            .any(|event| event.risk_type == risk_type && event.reason.as_deref() == Some(reason))
        {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("{run_id} did not emit risk event {risk_type} reason {reason}");
}

async fn wait_for_system_log(db: &Db, run_id: &str, target: &str) {
    for _ in 0..50 {
        if !db
            .list_system_logs_filtered(SystemLogFilter {
                run_id: Some(run_id.to_string()),
                target: Some(target.to_string()),
                ..SystemLogFilter::default()
            })
            .await
            .unwrap()
            .is_empty()
        {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("{run_id} did not emit system log target {target}");
}
