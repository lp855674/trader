use async_trait::async_trait;
use broker::{
    Broker, BrokerAccountSnapshot, BrokerCashBalance, BrokerContractMetadata, BrokerError,
    BrokerExecution, BrokerKind, BrokerOpenOrder, BrokerOrder, BrokerPositionSide,
    BrokerPositionSnapshot, BrokerSnapshotBundle, BrokerStatus, PlaceOrderResponse,
};
use runtime::{
    AlertSinkSettings, CancellationFlag, LiveRuntime, LiveRuntimeSettings,
    StartupRecoveryUnmatchedOpenOrdersPolicy,
};
use rust_decimal::Decimal;
use std::sync::Arc;
use storage::{
    Db, ExternalFillCommand, ExternalOrderCommand, PaperPortfolioSnapshotCommand,
    RuntimePositionSnapshotCommand, SystemLogFilter,
};
use trader_core::{OrderRequest, OrderSide, OrderType};

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
        alert_sink: AlertSinkSettings::Noop,
        logging: Default::default(),
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
            alert_sink: AlertSinkSettings::Noop,
            logging: Default::default(),
        },
    )
    .broker_status()
    .await
    .unwrap();
    assert_eq!(status.kind, BrokerKind::Futu);
    assert!(status.connected);
    assert!(db.list_orders("live-1").await.unwrap().is_empty());

    wait_for_runtime_event_category(&db, "live-1", "live.started").await;
    wait_for_latest_cash(&db, "live-1", "USD", "25000").await;
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
            alert_sink: AlertSinkSettings::Noop,
            logging: Default::default(),
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
async fn live_runtime_records_production_reconciliation_audit_and_broker_balances() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let live = LiveRuntime::new_with_broker(
        db.clone(),
        LiveRuntimeSettings {
            run_id: "live-production-reconciliation".to_string(),
            broker_kind: BrokerKind::InteractiveBrokers,
            account_id: "live-account".to_string(),
            base_currency: "USD".to_string(),
            initial_cash: dec("25000"),
            broker_snapshot_interval_ms: Some(5),
            alert_sink: AlertSinkSettings::Noop,
            logging: Default::default(),
        },
        Arc::new(StaticSnapshotBroker),
    );
    let cancel = CancellationFlag::default();
    let task_cancel = cancel.clone();
    let handle = tokio::spawn(async move { live.run(task_cancel).await.unwrap() });

    wait_for_reconciliation_audit(&db, "live-production-reconciliation").await;

    cancel.cancel();
    handle.await.unwrap();

    let balances = db
        .list_broker_account_balances("live-production-reconciliation")
        .await
        .unwrap();
    assert!(!balances.is_empty());
    assert_eq!(balances[0].account_id, "live-account");
    assert_eq!(balances[0].currency, "USD");
    assert_eq!(balances[0].cash, "123456");

    let audits = db
        .list_reconciliation_audits("live-production-reconciliation")
        .await
        .unwrap();
    assert!(!audits.is_empty());
    assert_eq!(audits[0].severity, "error");
    assert_eq!(audits[0].cash_drift_count, 1);
    assert_eq!(audits[0].position_drift_count, 1);
}

#[tokio::test]
async fn live_runtime_reconciliation_audit_detects_runtime_position_missing_from_broker() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let live = LiveRuntime::new_with_broker(
        db.clone(),
        LiveRuntimeSettings {
            run_id: "live-runtime-position-missing-broker".to_string(),
            broker_kind: BrokerKind::InteractiveBrokers,
            account_id: "live-account".to_string(),
            base_currency: "USD".to_string(),
            initial_cash: dec("100000"),
            broker_snapshot_interval_ms: Some(5),
            alert_sink: AlertSinkSettings::Noop,
            logging: Default::default(),
        },
        Arc::new(EmptyPositionSnapshotBroker),
    );
    let cancel = CancellationFlag::default();
    let task_cancel = cancel.clone();
    let handle = tokio::spawn(async move { live.run(task_cancel).await.unwrap() });

    wait_for_status(&db, "live-runtime-position-missing-broker", "running").await;
    db.record_runtime_position_snapshot(RuntimePositionSnapshotCommand {
        run_id: "live-runtime-position-missing-broker".to_string(),
        ts_ms: chrono::Utc::now().timestamp_millis(),
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        position_side: "long".to_string(),
        qty: dec("2"),
        available_qty: dec("2"),
        avg_price: dec("180"),
        mark_price: Some(dec("180")),
        currency: "USD".to_string(),
        contract_metadata_json: None,
    })
    .await
    .unwrap();
    wait_for_reconciliation_position_drift(&db, "live-runtime-position-missing-broker").await;
    wait_for_system_log_message_contains(
        &db,
        "live-runtime-position-missing-broker",
        "runtime.reconciliation",
        "reconciliation.drift",
        "position_missing_broker",
    )
    .await;
    wait_for_system_log_message_contains(
        &db,
        "live-runtime-position-missing-broker",
        "runtime.alert",
        "reconciliation_drift.alert",
        "position_missing_broker",
    )
    .await;

    cancel.cancel();
    handle.await.unwrap();

    let audits = db
        .list_reconciliation_audits("live-runtime-position-missing-broker")
        .await
        .unwrap();
    let audit = audits
        .iter()
        .find(|audit| audit.position_drift_count == 1)
        .unwrap();
    assert_eq!(audit.severity, "error");
    assert_eq!(audit.cash_drift_count, 0);
    let payload: serde_json::Value = serde_json::from_str(&audit.payload_json).unwrap();
    assert_eq!(
        payload["position_drifts"][0]["reason"].as_str(),
        Some("position_missing_broker")
    );
    assert_eq!(
        payload["position_drifts"][0]["local_value"].as_str(),
        Some("2")
    );
}

#[tokio::test]
async fn live_runtime_reconciliation_audit_detects_runtime_open_order_missing_from_broker() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let run_id = "live-runtime-open-order-missing-broker";
    seed_external_order(
        &db,
        run_id,
        "local-order-1",
        "client-order-1",
        "broker-order-1",
        "US:NASDAQ:AAPL:EQUITY",
        "BUY",
        "1",
        "0",
        "SUBMITTED",
    )
    .await;
    let live = LiveRuntime::new_with_broker(
        db.clone(),
        LiveRuntimeSettings {
            run_id: run_id.to_string(),
            broker_kind: BrokerKind::InteractiveBrokers,
            account_id: "live-account".to_string(),
            base_currency: "USD".to_string(),
            initial_cash: dec("100000"),
            broker_snapshot_interval_ms: Some(5),
            alert_sink: AlertSinkSettings::Noop,
            logging: Default::default(),
        },
        Arc::new(EmptyPositionSnapshotBroker),
    );
    let cancel = CancellationFlag::default();
    let task_cancel = cancel.clone();
    let handle = tokio::spawn(async move { live.run(task_cancel).await.unwrap() });

    wait_for_reconciliation_open_order_drift(&db, run_id).await;
    wait_for_system_log_message_contains(
        &db,
        run_id,
        "runtime.reconciliation",
        "reconciliation.drift",
        "open_order_missing_broker",
    )
    .await;
    wait_for_system_log_message_contains(
        &db,
        run_id,
        "runtime.alert",
        "reconciliation_drift.alert",
        "open_order_missing_broker",
    )
    .await;

    cancel.cancel();
    handle.await.unwrap();

    let audits = db.list_reconciliation_audits(run_id).await.unwrap();
    let audit = audits
        .iter()
        .find(|audit| audit.open_order_drift_count == 1)
        .unwrap();
    assert_eq!(audit.severity, "error");
    let payload: serde_json::Value = serde_json::from_str(&audit.payload_json).unwrap();
    assert_eq!(
        payload["open_order_drifts"][0]["reason"].as_str(),
        Some("open_order_missing_broker")
    );
    assert_eq!(
        payload["open_order_drifts"][0]["local_value"].as_str(),
        Some("broker-order-1")
    );
    let risk_events = db.list_risk_events(run_id).await.unwrap();
    assert!(
        risk_events
            .iter()
            .all(|event| event.reason.as_deref() != Some("open_order_missing_broker")),
        "audit-only open-order drift must not create risk events"
    );
}

#[tokio::test]
async fn live_runtime_reconciliation_audit_deduplicates_open_order_missing_broker_alerts() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let run_id = "live-runtime-open-order-drift-dedup";
    seed_external_order(
        &db,
        run_id,
        "local-order-1",
        "client-order-1",
        "broker-order-1",
        "US:NASDAQ:AAPL:EQUITY",
        "BUY",
        "1",
        "0",
        "SUBMITTED",
    )
    .await;
    let live = LiveRuntime::new_with_broker(
        db.clone(),
        LiveRuntimeSettings {
            run_id: run_id.to_string(),
            broker_kind: BrokerKind::InteractiveBrokers,
            account_id: "live-account".to_string(),
            base_currency: "USD".to_string(),
            initial_cash: dec("100000"),
            broker_snapshot_interval_ms: Some(5),
            alert_sink: AlertSinkSettings::Noop,
            logging: Default::default(),
        },
        Arc::new(EmptyPositionSnapshotBroker),
    );
    let cancel = CancellationFlag::default();
    let task_cancel = cancel.clone();
    let handle = tokio::spawn(async move { live.run(task_cancel).await.unwrap() });

    wait_for_reconciliation_open_order_drift_count(&db, run_id, 2).await;

    cancel.cancel();
    handle.await.unwrap();

    let reconciliation_logs = db
        .list_system_logs_filtered(SystemLogFilter {
            run_id: Some(run_id.to_string()),
            target: Some("runtime.reconciliation".to_string()),
            ..SystemLogFilter::default()
        })
        .await
        .unwrap();
    let drift_log_count = reconciliation_logs
        .iter()
        .filter(|log| {
            log.message == "reconciliation.drift"
                && log
                    .fields_json
                    .as_deref()
                    .is_some_and(|fields| fields.contains("open_order_missing_broker"))
        })
        .count();
    assert_eq!(drift_log_count, 1);

    let alert_logs = db
        .list_system_logs_filtered(SystemLogFilter {
            run_id: Some(run_id.to_string()),
            target: Some("runtime.alert".to_string()),
            ..SystemLogFilter::default()
        })
        .await
        .unwrap();
    let alert_log_count = alert_logs
        .iter()
        .filter(|log| {
            log.message == "reconciliation_drift.alert"
                && log
                    .fields_json
                    .as_deref()
                    .is_some_and(|fields| fields.contains("open_order_missing_broker"))
        })
        .count();
    assert_eq!(alert_log_count, 1);
}

#[tokio::test]
async fn live_runtime_reconciliation_audit_keeps_multi_currency_cash_alerts_distinct() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let run_id = "live-runtime-multi-currency-cash-drift";
    let live = LiveRuntime::new_with_broker(
        db.clone(),
        LiveRuntimeSettings {
            run_id: run_id.to_string(),
            broker_kind: BrokerKind::InteractiveBrokers,
            account_id: "live-account".to_string(),
            base_currency: "USD".to_string(),
            initial_cash: dec("25000"),
            broker_snapshot_interval_ms: Some(5),
            alert_sink: AlertSinkSettings::Noop,
            logging: Default::default(),
        },
        Arc::new(MultiCurrencyCashSnapshotBroker),
    );
    let cancel = CancellationFlag::default();
    let task_cancel = cancel.clone();
    let handle = tokio::spawn(async move { live.run(task_cancel).await.unwrap() });

    wait_for_status(&db, run_id, "running").await;
    db.record_paper_portfolio_snapshot(PaperPortfolioSnapshotCommand {
        run_id: run_id.to_string(),
        account_id: "live-account".to_string(),
        ts_ms: chrono::Utc::now().timestamp_millis(),
        base_currency: "HKD".to_string(),
        cash: dec("500"),
        market_value: Decimal::ZERO,
        equity: dec("500"),
        realized_pnl: Decimal::ZERO,
        unrealized_pnl: Decimal::ZERO,
        positions: Vec::new(),
    })
    .await
    .unwrap();
    wait_for_reconciliation_cash_drift_count(&db, run_id, 3).await;
    wait_for_system_log_message_contains(
        &db,
        run_id,
        "runtime.alert",
        "reconciliation_drift.alert",
        "HKD",
    )
    .await;

    cancel.cancel();
    handle.await.unwrap();

    let reconciliation_logs = db
        .list_system_logs_filtered(SystemLogFilter {
            run_id: Some(run_id.to_string()),
            target: Some("runtime.reconciliation".to_string()),
            ..SystemLogFilter::default()
        })
        .await
        .unwrap();
    let drift_currencies = reconciliation_logs
        .iter()
        .filter_map(|log| {
            if log.message != "reconciliation.drift" {
                return None;
            }
            let fields = log.fields_json.as_deref()?;
            let fields = serde_json::from_str::<serde_json::Value>(fields).ok()?;
            (fields["reason"].as_str() == Some("cash_missing_broker"))
                .then(|| fields["currency"].as_str().map(str::to_string))?
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        drift_currencies,
        ["HKD".to_string(), "USD".to_string()].into_iter().collect()
    );

    let alert_logs = db
        .list_system_logs_filtered(SystemLogFilter {
            run_id: Some(run_id.to_string()),
            target: Some("runtime.alert".to_string()),
            ..SystemLogFilter::default()
        })
        .await
        .unwrap();
    let alert_currencies = alert_logs
        .iter()
        .filter_map(|log| {
            if log.message != "reconciliation_drift.alert" {
                return None;
            }
            let fields = log.fields_json.as_deref()?;
            let fields = serde_json::from_str::<serde_json::Value>(fields).ok()?;
            (fields["reason"].as_str() == Some("cash_missing_broker"))
                .then(|| fields["currency"].as_str().map(str::to_string))?
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        alert_currencies,
        ["HKD".to_string(), "USD".to_string()].into_iter().collect()
    );
}

#[tokio::test]
async fn live_runtime_reconciliation_audit_ignores_closed_local_orders_missing_from_broker() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let run_id = "live-runtime-closed-order-not-missing-broker";
    seed_external_order(
        &db,
        run_id,
        "local-order-1",
        "client-order-1",
        "broker-order-1",
        "US:NASDAQ:AAPL:EQUITY",
        "BUY",
        "1",
        "1",
        "FILLED",
    )
    .await;
    let live = LiveRuntime::new_with_broker(
        db.clone(),
        LiveRuntimeSettings {
            run_id: run_id.to_string(),
            broker_kind: BrokerKind::InteractiveBrokers,
            account_id: "live-account".to_string(),
            base_currency: "USD".to_string(),
            initial_cash: dec("100000"),
            broker_snapshot_interval_ms: Some(5),
            alert_sink: AlertSinkSettings::Noop,
            logging: Default::default(),
        },
        Arc::new(EmptyPositionSnapshotBroker),
    );
    let cancel = CancellationFlag::default();
    let task_cancel = cancel.clone();
    let handle = tokio::spawn(async move { live.run(task_cancel).await.unwrap() });

    wait_for_reconciliation_audit(&db, run_id).await;

    cancel.cancel();
    handle.await.unwrap();

    let audits = db.list_reconciliation_audits(run_id).await.unwrap();
    assert!(!audits.is_empty());
    assert!(audits.iter().all(|audit| audit.open_order_drift_count == 0));
    assert!(audits.iter().all(|audit| {
        let payload: serde_json::Value = serde_json::from_str(&audit.payload_json).unwrap();
        payload["open_order_drifts"].as_array().unwrap().is_empty()
    }));
}

#[tokio::test]
async fn live_runtime_reconciliation_audit_matches_broker_execution_by_order_metadata() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let run_id = "live-runtime-execution-matches-order-metadata";
    seed_external_order(
        &db,
        run_id,
        "local-order-1",
        "client-order-1",
        "broker-order-1",
        "US:NASDAQ:AAPL:EQUITY",
        "BUY",
        "1",
        "1",
        "FILLED",
    )
    .await;
    seed_external_fill(
        &db,
        run_id,
        "local-order-1",
        "local-fill-1",
        "US:NASDAQ:AAPL:EQUITY",
        "BUY",
        "180",
        "1",
        "1",
    )
    .await;
    let live = LiveRuntime::new_with_broker(
        db.clone(),
        LiveRuntimeSettings {
            run_id: run_id.to_string(),
            broker_kind: BrokerKind::InteractiveBrokers,
            account_id: "live-account".to_string(),
            base_currency: "USD".to_string(),
            initial_cash: dec("100000"),
            broker_snapshot_interval_ms: Some(5),
            alert_sink: AlertSinkSettings::Noop,
            logging: Default::default(),
        },
        Arc::new(MatchingExecutionSnapshotBroker),
    );
    let cancel = CancellationFlag::default();
    let task_cancel = cancel.clone();
    let handle = tokio::spawn(async move { live.run(task_cancel).await.unwrap() });

    wait_for_reconciliation_audit(&db, run_id).await;

    cancel.cancel();
    handle.await.unwrap();

    let audits = db.list_reconciliation_audits(run_id).await.unwrap();
    assert!(!audits.is_empty());
    assert!(audits.iter().all(|audit| audit.execution_drift_count == 0));
    assert!(audits.iter().all(|audit| {
        let payload: serde_json::Value = serde_json::from_str(&audit.payload_json).unwrap();
        payload["execution_drifts"].as_array().unwrap().is_empty()
    }));
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
            alert_sink: AlertSinkSettings::Noop,
            logging: Default::default(),
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
async fn live_runtime_records_snapshots_from_injected_broker() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let live = LiveRuntime::new_with_broker(
        db.clone(),
        LiveRuntimeSettings {
            run_id: "live-injected-broker".to_string(),
            broker_kind: BrokerKind::InteractiveBrokers,
            account_id: "DU12345".to_string(),
            base_currency: "USD".to_string(),
            initial_cash: dec("25000"),
            broker_snapshot_interval_ms: Some(5),
            alert_sink: AlertSinkSettings::Noop,
            logging: Default::default(),
        },
        Arc::new(StaticSnapshotBroker),
    );
    let cancel = CancellationFlag::default();
    let task_cancel = cancel.clone();
    let handle = tokio::spawn(async move { live.run(task_cancel).await.unwrap() });

    wait_for_latest_cash(&db, "live-injected-broker", "USD", "123456").await;
    wait_for_latest_position(
        &db,
        "live-injected-broker",
        "US:NASDAQ:AAPL:EQUITY",
        "long",
        "2",
    )
    .await;

    cancel.cancel();
    handle.await.unwrap();
}

#[tokio::test]
async fn live_runtime_recovers_open_orders_and_executions_on_startup() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    seed_external_order(
        &db,
        "live-startup-recovery",
        "order-recover",
        "client-recover",
        "broker-recover",
        "US:NASDAQ:AAPL:EQUITY",
        "BUY",
        "2",
        "0",
        "SUBMITTED",
    )
    .await;

    let live = LiveRuntime::new_with_broker(
        db.clone(),
        LiveRuntimeSettings {
            run_id: "live-startup-recovery".to_string(),
            broker_kind: BrokerKind::InteractiveBrokers,
            account_id: "live-account".to_string(),
            base_currency: "USD".to_string(),
            initial_cash: dec("100000"),
            broker_snapshot_interval_ms: None,
            alert_sink: AlertSinkSettings::Noop,
            logging: Default::default(),
        },
        Arc::new(StartupRecoveryBroker),
    );
    let cancel = CancellationFlag::default();
    let task_cancel = cancel.clone();
    let handle = tokio::spawn(async move { live.run(task_cancel).await.unwrap() });

    wait_for_system_log(&db, "live-startup-recovery", "runtime.startup_recovery").await;

    cancel.cancel();
    handle.await.unwrap();

    let recovered = db
        .recover_order_state("live-startup-recovery", "order-recover")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(recovered.status, "FILLED");
    assert_eq!(recovered.filled_qty, "2");
    let fills = db.list_fills("live-startup-recovery").await.unwrap();
    assert_eq!(fills.len(), 1);
    assert_eq!(fills[0].id, "broker-exec-1");
    assert_eq!(fills[0].price, "186");
    assert_eq!(fills[0].qty, "2");
    assert!(
        db.list_recoverable_orders("live-startup-recovery")
            .await
            .unwrap()
            .is_empty()
    );
    let order_events = db.list_order_events("live-startup-recovery").await.unwrap();
    let recovered_event = order_events
        .iter()
        .find(|event| event.event_type == "broker.order.recovered")
        .unwrap();
    assert_eq!(recovered_event.order_id.as_deref(), Some("order-recover"));
    assert_eq!(
        recovered_event.client_order_id.as_deref(),
        Some("client-recover")
    );
    assert_eq!(
        recovered_event.broker_order_id.as_deref(),
        Some("broker-recover")
    );
    assert_eq!(recovered_event.status, "FILLED");
    assert_eq!(
        recovered_event.message.as_deref(),
        Some("startup recovery matched broker order state")
    );
    let recovered_payload: serde_json::Value =
        serde_json::from_str(&recovered_event.payload_json).unwrap();
    assert_eq!(recovered_payload["recovery_source"], "startup");
    assert_eq!(recovered_payload["executions"], 1);
    assert_eq!(recovered_payload["filled_qty"], "2");
    assert_eq!(
        recovered_payload["message"],
        "startup recovery matched broker order state"
    );

    let recovery_logs = db
        .list_system_logs_filtered(SystemLogFilter {
            run_id: Some("live-startup-recovery".to_string()),
            target: Some("runtime.startup_recovery".to_string()),
            ..SystemLogFilter::default()
        })
        .await
        .unwrap();
    let recovery_log = recovery_logs
        .iter()
        .find(|log| log.message == "startup_recovery.orders")
        .unwrap();
    assert_eq!(recovery_log.level, "INFO");
    let recovery_fields: serde_json::Value =
        serde_json::from_str(recovery_log.fields_json.as_deref().unwrap()).unwrap();
    assert_eq!(recovery_fields["scanned"], 1);
    assert_eq!(recovery_fields["recovered"], 1);
    assert_eq!(recovery_fields["remaining"], 0);
    assert_eq!(recovery_fields["executions"], 1);
    assert_eq!(recovery_fields["unmatched_open_orders"], 0);
    assert_eq!(recovery_fields["unmatched_executions"], 0);
    assert!(
        recovery_fields["unmatched_open_order_ids"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        recovery_fields["unmatched_execution_ids"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn live_runtime_records_recovered_fills_with_local_order_symbol() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    seed_external_order(
        &db,
        "live-native-symbol-recovery",
        "order-native-symbol",
        "client-native-symbol",
        "broker-native-symbol",
        "CRYPTO:BINANCE:BTCUSDT:CRYPTO_SPOT",
        "SELL",
        "0.002",
        "0",
        "SUBMITTED",
    )
    .await;

    let live = LiveRuntime::new_with_broker(
        db.clone(),
        LiveRuntimeSettings {
            run_id: "live-native-symbol-recovery".to_string(),
            broker_kind: BrokerKind::Binance,
            account_id: "live-account".to_string(),
            base_currency: "USDT".to_string(),
            initial_cash: dec("100000"),
            broker_snapshot_interval_ms: None,
            alert_sink: AlertSinkSettings::Noop,
            logging: Default::default(),
        },
        Arc::new(NativeSymbolExecutionRecoveryBroker),
    );
    let cancel = CancellationFlag::default();
    let task_cancel = cancel.clone();
    let handle = tokio::spawn(async move { live.run(task_cancel).await.unwrap() });

    wait_for_system_log(
        &db,
        "live-native-symbol-recovery",
        "runtime.startup_recovery",
    )
    .await;

    cancel.cancel();
    handle.await.unwrap();

    let fills = db.list_fills("live-native-symbol-recovery").await.unwrap();
    assert_eq!(fills.len(), 1);
    assert_eq!(fills[0].symbol, "CRYPTO:BINANCE:BTCUSDT:CRYPTO_SPOT");
    assert_eq!(fills[0].side, "SELL");
}

#[tokio::test]
async fn live_runtime_adds_new_recovered_executions_to_existing_fills() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    seed_external_order(
        &db,
        "live-existing-fill-recovery",
        "order-existing-fill",
        "client-existing-fill",
        "broker-existing-fill",
        "US:NASDAQ:AAPL:EQUITY",
        "BUY",
        "2",
        "1",
        "PARTIALLY_FILLED",
    )
    .await;
    seed_external_fill(
        &db,
        "live-existing-fill-recovery",
        "order-existing-fill",
        "broker-exec-existing",
        "US:NASDAQ:AAPL:EQUITY",
        "BUY",
        "185",
        "1",
        "0.50",
    )
    .await;

    let live = LiveRuntime::new_with_broker(
        db.clone(),
        LiveRuntimeSettings {
            run_id: "live-existing-fill-recovery".to_string(),
            broker_kind: BrokerKind::InteractiveBrokers,
            account_id: "live-account".to_string(),
            base_currency: "USD".to_string(),
            initial_cash: dec("100000"),
            broker_snapshot_interval_ms: None,
            alert_sink: AlertSinkSettings::Noop,
            logging: Default::default(),
        },
        Arc::new(IncrementalExecutionRecoveryBroker),
    );
    let cancel = CancellationFlag::default();
    let task_cancel = cancel.clone();
    let handle = tokio::spawn(async move { live.run(task_cancel).await.unwrap() });

    wait_for_system_log(
        &db,
        "live-existing-fill-recovery",
        "runtime.startup_recovery",
    )
    .await;

    cancel.cancel();
    handle.await.unwrap();

    let recovered = db
        .recover_order_state("live-existing-fill-recovery", "order-existing-fill")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(recovered.status, "FILLED");
    assert_eq!(recovered.filled_qty, "2");
    let fills = db.list_fills("live-existing-fill-recovery").await.unwrap();
    assert_eq!(fills.len(), 2);
    assert!(fills.iter().any(|fill| fill.id == "broker-exec-existing"));
    assert!(fills.iter().any(|fill| fill.id == "broker-exec-new"));
}

#[tokio::test]
async fn live_runtime_does_not_decrease_local_filled_qty_when_recovery_lacks_executions() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    seed_external_order(
        &db,
        "live-local-partial-recovery",
        "order-local-partial",
        "client-local-partial",
        "broker-local-partial",
        "US:NASDAQ:AAPL:EQUITY",
        "BUY",
        "2",
        "1",
        "PARTIALLY_FILLED",
    )
    .await;

    let live = LiveRuntime::new_with_broker(
        db.clone(),
        LiveRuntimeSettings {
            run_id: "live-local-partial-recovery".to_string(),
            broker_kind: BrokerKind::InteractiveBrokers,
            account_id: "live-account".to_string(),
            base_currency: "USD".to_string(),
            initial_cash: dec("100000"),
            broker_snapshot_interval_ms: None,
            alert_sink: AlertSinkSettings::Noop,
            logging: Default::default(),
        },
        Arc::new(OpenOrderOnlyPartialRecoveryBroker),
    );
    let cancel = CancellationFlag::default();
    let task_cancel = cancel.clone();
    let handle = tokio::spawn(async move { live.run(task_cancel).await.unwrap() });

    wait_for_system_log(
        &db,
        "live-local-partial-recovery",
        "runtime.startup_recovery",
    )
    .await;

    cancel.cancel();
    handle.await.unwrap();

    let recovered = db
        .recover_order_state("live-local-partial-recovery", "order-local-partial")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(recovered.status, "PARTIALLY_FILLED");
    assert_eq!(recovered.filled_qty, "1");
}

#[tokio::test]
async fn live_runtime_fails_startup_when_remote_open_order_is_unmatched() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    seed_external_order(
        &db,
        "live-startup-unmatched",
        "order-known",
        "client-known",
        "broker-known",
        "US:NASDAQ:AAPL:EQUITY",
        "BUY",
        "1",
        "0",
        "SUBMITTED",
    )
    .await;

    let live = LiveRuntime::new_with_broker(
        db.clone(),
        LiveRuntimeSettings {
            run_id: "live-startup-unmatched".to_string(),
            broker_kind: BrokerKind::InteractiveBrokers,
            account_id: "live-account".to_string(),
            base_currency: "USD".to_string(),
            initial_cash: dec("100000"),
            broker_snapshot_interval_ms: None,
            alert_sink: AlertSinkSettings::Noop,
            logging: Default::default(),
        },
        Arc::new(UnmatchedStartupRecoveryBroker),
    );
    let error = tokio::time::timeout(
        std::time::Duration::from_millis(200),
        live.run(CancellationFlag::default()),
    )
    .await
    .expect("startup with unmatched remote open order should not enter main loop")
    .unwrap_err();
    assert!(error.to_string().contains("unmatched remote open orders"));
    assert!(error.to_string().contains("broker-unknown"));

    let run = db
        .get_strategy_run("live-startup-unmatched")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.status, "failed");
    assert!(
        run.error
            .as_deref()
            .unwrap()
            .contains("unmatched remote open orders")
    );

    let logs = db
        .list_system_logs_filtered(SystemLogFilter {
            run_id: Some("live-startup-unmatched".to_string()),
            target: Some("runtime.startup_recovery".to_string()),
            ..SystemLogFilter::default()
        })
        .await
        .unwrap();
    let log = logs
        .iter()
        .find(|log| log.message == "startup_recovery.orders")
        .unwrap();
    assert_eq!(log.level, "WARN");
    let fields = log.fields_json.as_deref().unwrap();
    assert!(fields.contains("\"unmatched_open_orders\":1"));
    assert!(fields.contains("\"unmatched_executions\":1"));
    assert!(fields.contains("\"broker-unknown\""));
    assert!(fields.contains("\"broker-exec-unknown\""));
    assert!(logs.iter().any(|log| {
        log.level == "ERROR"
            && log.message == "startup_recovery.failed"
            && log
                .fields_json
                .as_deref()
                .is_some_and(|fields| fields.contains("unmatched remote open orders"))
    }));
}

#[tokio::test]
async fn live_runtime_warns_but_continues_for_unmatched_remote_executions_on_startup() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    seed_external_order(
        &db,
        "live-startup-unmatched-execution",
        "order-known-exec-only",
        "client-known-exec-only",
        "broker-known-exec-only",
        "US:NASDAQ:AAPL:EQUITY",
        "BUY",
        "1",
        "0",
        "SUBMITTED",
    )
    .await;

    let live = LiveRuntime::new_with_broker(
        db.clone(),
        LiveRuntimeSettings {
            run_id: "live-startup-unmatched-execution".to_string(),
            broker_kind: BrokerKind::InteractiveBrokers,
            account_id: "live-account".to_string(),
            base_currency: "USD".to_string(),
            initial_cash: dec("100000"),
            broker_snapshot_interval_ms: None,
            alert_sink: AlertSinkSettings::Noop,
            logging: Default::default(),
        },
        Arc::new(UnmatchedExecutionOnlyStartupRecoveryBroker),
    );
    let cancel = CancellationFlag::default();
    let task_cancel = cancel.clone();
    let handle = tokio::spawn(async move { live.run(task_cancel).await.unwrap() });

    wait_for_system_log(
        &db,
        "live-startup-unmatched-execution",
        "runtime.startup_recovery",
    )
    .await;

    let run = db
        .get_strategy_run("live-startup-unmatched-execution")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.status, "running");

    cancel.cancel();
    handle.await.unwrap();

    let logs = db
        .list_system_logs_filtered(SystemLogFilter {
            run_id: Some("live-startup-unmatched-execution".to_string()),
            target: Some("runtime.startup_recovery".to_string()),
            ..SystemLogFilter::default()
        })
        .await
        .unwrap();
    let log = logs
        .iter()
        .find(|log| log.message == "startup_recovery.orders")
        .unwrap();
    assert_eq!(log.level, "WARN");
    let fields = log.fields_json.as_deref().unwrap();
    assert!(fields.contains("\"unmatched_open_orders\":0"));
    assert!(fields.contains("\"unmatched_executions\":1"));
    assert!(fields.contains("\"broker-exec-unknown\""));
}

#[tokio::test]
async fn live_runtime_can_warn_only_for_unmatched_remote_open_orders_when_configured() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    seed_external_order(
        &db,
        "live-startup-unmatched-warn-only",
        "order-warn-only",
        "client-known",
        "broker-known",
        "US:NASDAQ:AAPL:EQUITY",
        "BUY",
        "1",
        "0",
        "SUBMITTED",
    )
    .await;

    let live = LiveRuntime::new_with_broker(
        db.clone(),
        LiveRuntimeSettings {
            run_id: "live-startup-unmatched-warn-only".to_string(),
            broker_kind: BrokerKind::InteractiveBrokers,
            account_id: "live-account".to_string(),
            base_currency: "USD".to_string(),
            initial_cash: dec("100000"),
            broker_snapshot_interval_ms: None,
            alert_sink: AlertSinkSettings::Noop,
            logging: Default::default(),
        },
        Arc::new(UnmatchedStartupRecoveryBroker),
    )
    .with_startup_recovery_unmatched_open_orders_policy(
        StartupRecoveryUnmatchedOpenOrdersPolicy::WarnOnly,
    );
    let cancel = CancellationFlag::default();
    let task_cancel = cancel.clone();
    let handle = tokio::spawn(async move { live.run(task_cancel).await.unwrap() });

    wait_for_system_log(
        &db,
        "live-startup-unmatched-warn-only",
        "runtime.startup_recovery",
    )
    .await;

    let run = db
        .get_strategy_run("live-startup-unmatched-warn-only")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.status, "running");

    cancel.cancel();
    handle.await.unwrap();

    let logs = db
        .list_system_logs_filtered(SystemLogFilter {
            run_id: Some("live-startup-unmatched-warn-only".to_string()),
            target: Some("runtime.startup_recovery".to_string()),
            ..SystemLogFilter::default()
        })
        .await
        .unwrap();
    assert!(logs.iter().any(|log| {
        log.level == "WARN"
            && log.message == "startup_recovery.orders"
            && log.fields_json.as_deref().is_some_and(|fields| {
                fields.contains("\"unmatched_open_orders\":1")
                    && fields.contains("\"broker-unknown\"")
            })
    }));
    assert!(
        !logs
            .iter()
            .any(|log| log.message == "startup_recovery.failed")
    );
}

#[tokio::test]
async fn live_runtime_marks_run_failed_when_startup_recovery_fails() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    seed_external_order(
        &db,
        "live-startup-recovery-fail",
        "order-fail",
        "client-fail",
        "broker-fail",
        "US:NASDAQ:AAPL:EQUITY",
        "BUY",
        "1",
        "0",
        "SUBMITTED",
    )
    .await;

    let live = LiveRuntime::new_with_broker(
        db.clone(),
        LiveRuntimeSettings {
            run_id: "live-startup-recovery-fail".to_string(),
            broker_kind: BrokerKind::InteractiveBrokers,
            account_id: "live-account".to_string(),
            base_currency: "USD".to_string(),
            initial_cash: dec("100000"),
            broker_snapshot_interval_ms: None,
            alert_sink: AlertSinkSettings::Noop,
            logging: Default::default(),
        },
        Arc::new(StartupRecoveryFailureBroker),
    );

    let error = live.run(CancellationFlag::default()).await.unwrap_err();
    assert!(error.to_string().contains("gateway unavailable"));

    let run = db
        .get_strategy_run("live-startup-recovery-fail")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.status, "failed");
    assert!(
        run.error
            .as_deref()
            .unwrap()
            .contains("gateway unavailable")
    );

    let logs = db
        .list_system_logs_filtered(SystemLogFilter {
            run_id: Some("live-startup-recovery-fail".to_string()),
            target: Some("runtime.startup_recovery".to_string()),
            ..SystemLogFilter::default()
        })
        .await
        .unwrap();
    assert!(logs.iter().any(|log| {
        log.level == "ERROR"
            && log.message == "startup_recovery.failed"
            && log
                .fields_json
                .as_deref()
                .is_some_and(|fields| fields.contains("gateway unavailable"))
    }));
}

#[tokio::test]
async fn live_runtime_records_broker_position_currency_from_contract_metadata() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let run_id = "live-broker-position-contract-currency";
    let live = LiveRuntime::new_with_broker(
        db.clone(),
        LiveRuntimeSettings {
            run_id: run_id.to_string(),
            broker_kind: BrokerKind::InteractiveBrokers,
            account_id: "live-account".to_string(),
            base_currency: "USD".to_string(),
            initial_cash: dec("100000"),
            broker_snapshot_interval_ms: Some(5),
            alert_sink: AlertSinkSettings::Noop,
            logging: Default::default(),
        },
        Arc::new(ContractCurrencyPositionSnapshotBroker),
    );
    let cancel = CancellationFlag::default();
    let task_cancel = cancel.clone();
    let handle = tokio::spawn(async move { live.run(task_cancel).await.unwrap() });

    wait_for_system_log_message_contains(
        &db,
        run_id,
        "runtime.broker_snapshot",
        "broker.snapshot.position",
        "HKD",
    )
    .await;

    cancel.cancel();
    handle.await.unwrap();

    let snapshots = db.list_position_snapshots(run_id).await.unwrap();
    let snapshot = snapshots
        .iter()
        .find(|snapshot| snapshot.symbol == "HK:SEHK:0700:EQUITY")
        .unwrap();
    assert_eq!(snapshot.currency, "HKD");
    let contract_metadata: serde_json::Value =
        serde_json::from_str(snapshot.contract_metadata_json.as_deref().unwrap()).unwrap();
    assert_eq!(contract_metadata["currency"].as_str(), Some("HKD"));
    assert_eq!(contract_metadata["primary_exchange"].as_str(), Some("SEHK"));
}

#[tokio::test]
async fn live_runtime_reconciliation_matches_position_by_stored_contract_conid() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let run_id = "live-position-conid-match";
    let live = LiveRuntime::new_with_broker(
        db.clone(),
        LiveRuntimeSettings {
            run_id: run_id.to_string(),
            broker_kind: BrokerKind::InteractiveBrokers,
            account_id: "live-account".to_string(),
            base_currency: "USD".to_string(),
            initial_cash: dec("100000"),
            broker_snapshot_interval_ms: Some(5),
            alert_sink: AlertSinkSettings::Noop,
            logging: Default::default(),
        },
        Arc::new(ContractCurrencyPositionSnapshotBroker),
    );
    let cancel = CancellationFlag::default();
    let task_cancel = cancel.clone();
    let handle = tokio::spawn(async move { live.run(task_cancel).await.unwrap() });

    wait_for_status(&db, run_id, "running").await;
    db.record_runtime_position_snapshot(RuntimePositionSnapshotCommand {
        run_id: run_id.to_string(),
        ts_ms: chrono::Utc::now().timestamp_millis(),
        symbol: "HK:SEHK:700:EQUITY".to_string(),
        position_side: "long".to_string(),
        qty: dec("100"),
        available_qty: dec("100"),
        avg_price: dec("320"),
        mark_price: Some(dec("320")),
        currency: "HKD".to_string(),
        contract_metadata_json: Some(
            serde_json::to_string(&BrokerContractMetadata {
                conid: Some(8068578),
                currency: Some("HKD".to_string()),
                primary_exchange: Some("SEHK".to_string()),
                local_symbol: Some("700".to_string()),
                ..BrokerContractMetadata::default()
            })
            .unwrap(),
        ),
    })
    .await
    .unwrap();
    wait_for_reconciliation_position_clean(&db, run_id).await;

    cancel.cancel();
    handle.await.unwrap();

    let audits = db.list_reconciliation_audits(run_id).await.unwrap();
    let audit = audits
        .iter()
        .find(|audit| audit.position_drift_count == 0)
        .unwrap();
    assert_eq!(audit.position_drift_count, 0);
    let payload: serde_json::Value = serde_json::from_str(&audit.payload_json).unwrap();
    assert_eq!(payload["position_drifts"].as_array().unwrap().len(), 0);
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
            alert_sink: AlertSinkSettings::Noop,
            logging: Default::default(),
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

struct StaticSnapshotBroker;

#[async_trait]
impl Broker for StaticSnapshotBroker {
    async fn place_order(&self, _request: OrderRequest) -> Result<PlaceOrderResponse, BrokerError> {
        Err(BrokerError::Rejected(
            "test broker does not place orders".to_string(),
        ))
    }

    async fn cancel_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError> {
        Err(BrokerError::OrderNotFound(broker_order_id.to_string()))
    }

    async fn query_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError> {
        Err(BrokerError::OrderNotFound(broker_order_id.to_string()))
    }

    async fn account_snapshot(
        &self,
        _account_id: &str,
    ) -> Result<BrokerAccountSnapshot, BrokerError> {
        Err(BrokerError::Rejected(
            "runtime should use snapshot_bundle".to_string(),
        ))
    }

    async fn position_snapshots(
        &self,
        _account_id: &str,
    ) -> Result<Vec<BrokerPositionSnapshot>, BrokerError> {
        Err(BrokerError::Rejected(
            "runtime should use snapshot_bundle".to_string(),
        ))
    }

    async fn snapshot_bundle(
        &self,
        account_id: &str,
        _execution_symbols: &[String],
    ) -> Result<BrokerSnapshotBundle, BrokerError> {
        Ok(BrokerSnapshotBundle {
            account: BrokerAccountSnapshot {
                account_id: account_id.to_string(),
                cash: dec("123456"),
                equity: dec("123456"),
                buying_power: dec("123456"),
                margin_used: Decimal::ZERO,
                cash_balances: Vec::new(),
            },
            positions: vec![BrokerPositionSnapshot {
                account_id: account_id.to_string(),
                exchange: "IBKR".to_string(),
                symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
                position_side: BrokerPositionSide::Long,
                qty: dec("2"),
                avg_price: dec("185.25"),
                mark_price: None,
                margin_used: Decimal::ZERO,
                unrealized_pnl: Decimal::ZERO,
                ts_ms: 1_700_000_000_000,
                contract: None,
                liquidation_price: None,
                open_interest: None,
            }],
            open_orders: vec![BrokerOpenOrder {
                broker_order_id: "static-open-order".to_string(),
                client_order_id: "static-client-order".to_string(),
                account_id: account_id.to_string(),
                symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
                side: OrderSide::Buy,
                order_type: OrderType::Limit,
                price: Some(dec("185.25")),
                qty: dec("1"),
                filled_qty: Decimal::ZERO,
                status: "SUBMITTED".to_string(),
            }],
            executions: vec![BrokerExecution {
                trade_id: "static-execution".to_string(),
                broker_order_id: "static-open-order".to_string(),
                client_order_id: Some("static-client-order".to_string()),
                account_id: account_id.to_string(),
                symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
                side: OrderSide::Buy,
                price: dec("185.25"),
                qty: dec("1"),
                fee: Decimal::ZERO,
                ts_ms: 1_700_000_000_001,
            }],
        })
    }

    async fn open_orders(&self, _account_id: &str) -> Result<Vec<BrokerOpenOrder>, BrokerError> {
        Err(BrokerError::Rejected(
            "runtime should use snapshot_bundle open orders".to_string(),
        ))
    }

    async fn executions(
        &self,
        _account_id: &str,
        _symbol: Option<&str>,
    ) -> Result<Vec<BrokerExecution>, BrokerError> {
        Err(BrokerError::Rejected(
            "runtime should use snapshot_bundle executions".to_string(),
        ))
    }

    async fn status(&self) -> Result<BrokerStatus, BrokerError> {
        Ok(BrokerStatus {
            kind: BrokerKind::InteractiveBrokers,
            connected: true,
            trading_enabled: false,
            capabilities: broker::BrokerCapabilities {
                market_data: true,
                order_submit: false,
                order_cancel: true,
                paper_trading: true,
                live_trading: false,
            },
        })
    }
}

struct MultiCurrencyCashSnapshotBroker;

#[async_trait]
impl Broker for MultiCurrencyCashSnapshotBroker {
    async fn place_order(&self, _request: OrderRequest) -> Result<PlaceOrderResponse, BrokerError> {
        Err(BrokerError::Rejected(
            "test broker does not place orders".to_string(),
        ))
    }

    async fn cancel_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError> {
        Err(BrokerError::OrderNotFound(broker_order_id.to_string()))
    }

    async fn query_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError> {
        Err(BrokerError::OrderNotFound(broker_order_id.to_string()))
    }

    async fn account_snapshot(
        &self,
        account_id: &str,
    ) -> Result<BrokerAccountSnapshot, BrokerError> {
        Ok(BrokerAccountSnapshot {
            account_id: account_id.to_string(),
            cash: dec("100000"),
            equity: dec("100000"),
            buying_power: dec("100000"),
            margin_used: Decimal::ZERO,
            cash_balances: vec![BrokerCashBalance {
                account_id: account_id.to_string(),
                currency: "EUR".to_string(),
                cash: dec("100000"),
                available_cash: dec("100000"),
                frozen_cash: Decimal::ZERO,
                equity: Some(dec("100000")),
                buying_power: Some(dec("100000")),
                margin_used: Some(Decimal::ZERO),
                source_ts_ms: 1_700_000_000_000,
            }],
        })
    }

    async fn position_snapshots(
        &self,
        _account_id: &str,
    ) -> Result<Vec<BrokerPositionSnapshot>, BrokerError> {
        Ok(Vec::new())
    }

    async fn status(&self) -> Result<BrokerStatus, BrokerError> {
        Ok(BrokerStatus {
            kind: BrokerKind::InteractiveBrokers,
            connected: true,
            trading_enabled: false,
            capabilities: broker::BrokerCapabilities {
                market_data: true,
                order_submit: false,
                order_cancel: true,
                paper_trading: true,
                live_trading: false,
            },
        })
    }
}

struct ContractCurrencyPositionSnapshotBroker;

#[async_trait]
impl Broker for ContractCurrencyPositionSnapshotBroker {
    async fn place_order(&self, _request: OrderRequest) -> Result<PlaceOrderResponse, BrokerError> {
        Err(BrokerError::Rejected(
            "test broker does not place orders".to_string(),
        ))
    }

    async fn cancel_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError> {
        Err(BrokerError::OrderNotFound(broker_order_id.to_string()))
    }

    async fn query_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError> {
        Err(BrokerError::OrderNotFound(broker_order_id.to_string()))
    }

    async fn account_snapshot(
        &self,
        account_id: &str,
    ) -> Result<BrokerAccountSnapshot, BrokerError> {
        Ok(BrokerAccountSnapshot {
            account_id: account_id.to_string(),
            cash: dec("100000"),
            equity: dec("100000"),
            buying_power: dec("100000"),
            margin_used: Decimal::ZERO,
            cash_balances: Vec::new(),
        })
    }

    async fn position_snapshots(
        &self,
        account_id: &str,
    ) -> Result<Vec<BrokerPositionSnapshot>, BrokerError> {
        Ok(vec![BrokerPositionSnapshot {
            account_id: account_id.to_string(),
            exchange: "IBKR".to_string(),
            symbol: "HK:SEHK:0700:EQUITY".to_string(),
            position_side: BrokerPositionSide::Long,
            qty: dec("100"),
            avg_price: dec("320"),
            mark_price: None,
            margin_used: Decimal::ZERO,
            unrealized_pnl: dec("10"),
            ts_ms: 1_700_000_000_000,
            contract: Some(BrokerContractMetadata {
                conid: Some(8068578),
                sec_type: Some("STK".to_string()),
                currency: Some("HKD".to_string()),
                exchange: Some("SMART".to_string()),
                primary_exchange: Some("SEHK".to_string()),
                multiplier: Some(dec("1")),
                expiry: None,
                right: None,
                strike: None,
                local_symbol: Some("0700".to_string()),
                trading_class: Some("700".to_string()),
            }),
            liquidation_price: None,
            open_interest: None,
        }])
    }

    async fn status(&self) -> Result<BrokerStatus, BrokerError> {
        Ok(BrokerStatus {
            kind: BrokerKind::InteractiveBrokers,
            connected: true,
            trading_enabled: false,
            capabilities: broker::BrokerCapabilities {
                market_data: true,
                order_submit: false,
                order_cancel: true,
                paper_trading: true,
                live_trading: false,
            },
        })
    }
}

struct TwoSidedPositionSnapshotBroker;

#[async_trait]
impl Broker for TwoSidedPositionSnapshotBroker {
    async fn place_order(&self, _request: OrderRequest) -> Result<PlaceOrderResponse, BrokerError> {
        Err(BrokerError::Rejected(
            "test broker does not place orders".to_string(),
        ))
    }

    async fn cancel_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError> {
        Err(BrokerError::OrderNotFound(broker_order_id.to_string()))
    }

    async fn query_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError> {
        Err(BrokerError::OrderNotFound(broker_order_id.to_string()))
    }

    async fn account_snapshot(
        &self,
        account_id: &str,
    ) -> Result<BrokerAccountSnapshot, BrokerError> {
        Ok(BrokerAccountSnapshot {
            account_id: account_id.to_string(),
            cash: dec("100000"),
            equity: dec("100000"),
            buying_power: dec("100000"),
            margin_used: Decimal::ZERO,
            cash_balances: Vec::new(),
        })
    }

    async fn position_snapshots(
        &self,
        account_id: &str,
    ) -> Result<Vec<BrokerPositionSnapshot>, BrokerError> {
        Ok(vec![
            BrokerPositionSnapshot {
                account_id: account_id.to_string(),
                exchange: "IBKR".to_string(),
                symbol: "US:NASDAQ:MSFT:EQUITY".to_string(),
                position_side: BrokerPositionSide::Long,
                qty: dec("1"),
                avg_price: dec("400"),
                mark_price: None,
                margin_used: Decimal::ZERO,
                unrealized_pnl: Decimal::ZERO,
                ts_ms: 1_700_000_000_000,
                contract: Some(BrokerContractMetadata {
                    currency: Some("USD".to_string()),
                    primary_exchange: Some("NASDAQ".to_string()),
                    ..BrokerContractMetadata::default()
                }),
                liquidation_price: None,
                open_interest: None,
            },
            BrokerPositionSnapshot {
                account_id: account_id.to_string(),
                exchange: "IBKR".to_string(),
                symbol: "US:NASDAQ:MSFT:EQUITY".to_string(),
                position_side: BrokerPositionSide::Short,
                qty: dec("1"),
                avg_price: dec("400"),
                mark_price: None,
                margin_used: Decimal::ZERO,
                unrealized_pnl: Decimal::ZERO,
                ts_ms: 1_700_000_000_000,
                contract: Some(BrokerContractMetadata {
                    currency: Some("USD".to_string()),
                    primary_exchange: Some("NASDAQ".to_string()),
                    ..BrokerContractMetadata::default()
                }),
                liquidation_price: None,
                open_interest: None,
            },
        ])
    }

    async fn status(&self) -> Result<BrokerStatus, BrokerError> {
        Ok(BrokerStatus {
            kind: BrokerKind::InteractiveBrokers,
            connected: true,
            trading_enabled: false,
            capabilities: broker::BrokerCapabilities {
                market_data: true,
                order_submit: false,
                order_cancel: true,
                paper_trading: true,
                live_trading: false,
            },
        })
    }
}

struct EmptyPositionSnapshotBroker;

#[async_trait]
impl Broker for EmptyPositionSnapshotBroker {
    async fn place_order(&self, _request: OrderRequest) -> Result<PlaceOrderResponse, BrokerError> {
        Err(BrokerError::Rejected(
            "test broker does not place orders".to_string(),
        ))
    }

    async fn cancel_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError> {
        Err(BrokerError::OrderNotFound(broker_order_id.to_string()))
    }

    async fn query_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError> {
        Err(BrokerError::OrderNotFound(broker_order_id.to_string()))
    }

    async fn account_snapshot(
        &self,
        account_id: &str,
    ) -> Result<BrokerAccountSnapshot, BrokerError> {
        Ok(BrokerAccountSnapshot {
            account_id: account_id.to_string(),
            cash: dec("100000"),
            equity: dec("100000"),
            buying_power: dec("100000"),
            margin_used: Decimal::ZERO,
            cash_balances: Vec::new(),
        })
    }

    async fn position_snapshots(
        &self,
        _account_id: &str,
    ) -> Result<Vec<BrokerPositionSnapshot>, BrokerError> {
        Ok(Vec::new())
    }

    async fn status(&self) -> Result<BrokerStatus, BrokerError> {
        Ok(BrokerStatus {
            kind: BrokerKind::InteractiveBrokers,
            connected: true,
            trading_enabled: false,
            capabilities: broker::BrokerCapabilities {
                market_data: true,
                order_submit: false,
                order_cancel: true,
                paper_trading: true,
                live_trading: false,
            },
        })
    }
}

struct MatchingExecutionSnapshotBroker;

#[async_trait]
impl Broker for MatchingExecutionSnapshotBroker {
    async fn place_order(&self, _request: OrderRequest) -> Result<PlaceOrderResponse, BrokerError> {
        Err(BrokerError::Rejected(
            "test broker does not place orders".to_string(),
        ))
    }

    async fn cancel_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError> {
        Err(BrokerError::OrderNotFound(broker_order_id.to_string()))
    }

    async fn query_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError> {
        Err(BrokerError::OrderNotFound(broker_order_id.to_string()))
    }

    async fn account_snapshot(
        &self,
        account_id: &str,
    ) -> Result<BrokerAccountSnapshot, BrokerError> {
        Ok(BrokerAccountSnapshot {
            account_id: account_id.to_string(),
            cash: dec("100000"),
            equity: dec("100000"),
            buying_power: dec("100000"),
            margin_used: Decimal::ZERO,
            cash_balances: Vec::new(),
        })
    }

    async fn position_snapshots(
        &self,
        _account_id: &str,
    ) -> Result<Vec<BrokerPositionSnapshot>, BrokerError> {
        Ok(Vec::new())
    }

    async fn executions(
        &self,
        _account_id: &str,
        symbol: Option<&str>,
    ) -> Result<Vec<BrokerExecution>, BrokerError> {
        if symbol != Some("US:NASDAQ:AAPL:EQUITY") {
            return Ok(Vec::new());
        }
        Ok(vec![BrokerExecution {
            trade_id: "broker-trade-1".to_string(),
            broker_order_id: "broker-order-1".to_string(),
            client_order_id: Some("client-order-1".to_string()),
            account_id: "live-account".to_string(),
            symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
            side: OrderSide::Buy,
            price: dec("180"),
            qty: dec("1"),
            fee: dec("1"),
            ts_ms: 2,
        }])
    }

    async fn status(&self) -> Result<BrokerStatus, BrokerError> {
        Ok(BrokerStatus {
            kind: BrokerKind::InteractiveBrokers,
            connected: true,
            trading_enabled: false,
            capabilities: broker::BrokerCapabilities {
                market_data: true,
                order_submit: false,
                order_cancel: true,
                paper_trading: true,
                live_trading: false,
            },
        })
    }
}

struct StartupRecoveryBroker;

#[async_trait]
impl Broker for StartupRecoveryBroker {
    async fn place_order(&self, _request: OrderRequest) -> Result<PlaceOrderResponse, BrokerError> {
        Err(BrokerError::Rejected(
            "test broker does not place orders".to_string(),
        ))
    }

    async fn cancel_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError> {
        Err(BrokerError::OrderNotFound(broker_order_id.to_string()))
    }

    async fn query_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError> {
        Err(BrokerError::OrderNotFound(broker_order_id.to_string()))
    }

    async fn account_snapshot(
        &self,
        account_id: &str,
    ) -> Result<BrokerAccountSnapshot, BrokerError> {
        Ok(BrokerAccountSnapshot {
            account_id: account_id.to_string(),
            cash: dec("100000"),
            equity: dec("100000"),
            buying_power: dec("100000"),
            margin_used: Decimal::ZERO,
            cash_balances: Vec::new(),
        })
    }

    async fn position_snapshots(
        &self,
        _account_id: &str,
    ) -> Result<Vec<BrokerPositionSnapshot>, BrokerError> {
        Ok(Vec::new())
    }

    async fn open_orders(&self, _account_id: &str) -> Result<Vec<BrokerOpenOrder>, BrokerError> {
        Ok(vec![BrokerOpenOrder {
            broker_order_id: "broker-recover".to_string(),
            client_order_id: "client-recover".to_string(),
            account_id: "live-account".to_string(),
            symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            price: Some(dec("185")),
            qty: dec("2"),
            filled_qty: Decimal::ZERO,
            status: "SUBMITTED".to_string(),
        }])
    }

    async fn executions(
        &self,
        _account_id: &str,
        _symbol: Option<&str>,
    ) -> Result<Vec<BrokerExecution>, BrokerError> {
        Ok(vec![BrokerExecution {
            trade_id: "broker-exec-1".to_string(),
            broker_order_id: "broker-recover".to_string(),
            client_order_id: Some("client-recover".to_string()),
            account_id: "live-account".to_string(),
            symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
            side: OrderSide::Buy,
            price: dec("186"),
            qty: dec("2"),
            fee: dec("1.25"),
            ts_ms: 2,
        }])
    }

    async fn status(&self) -> Result<BrokerStatus, BrokerError> {
        Ok(BrokerStatus {
            kind: BrokerKind::InteractiveBrokers,
            connected: true,
            trading_enabled: false,
            capabilities: broker::BrokerCapabilities {
                market_data: true,
                order_submit: false,
                order_cancel: true,
                paper_trading: true,
                live_trading: false,
            },
        })
    }
}

struct UnmatchedStartupRecoveryBroker;

struct NativeSymbolExecutionRecoveryBroker;

#[async_trait]
impl Broker for NativeSymbolExecutionRecoveryBroker {
    async fn place_order(&self, _request: OrderRequest) -> Result<PlaceOrderResponse, BrokerError> {
        Err(BrokerError::Rejected(
            "test broker does not place orders".to_string(),
        ))
    }

    async fn cancel_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError> {
        Err(BrokerError::OrderNotFound(broker_order_id.to_string()))
    }

    async fn query_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError> {
        Err(BrokerError::OrderNotFound(broker_order_id.to_string()))
    }

    async fn account_snapshot(
        &self,
        account_id: &str,
    ) -> Result<BrokerAccountSnapshot, BrokerError> {
        Ok(BrokerAccountSnapshot {
            account_id: account_id.to_string(),
            cash: dec("100000"),
            equity: dec("100000"),
            buying_power: dec("100000"),
            margin_used: Decimal::ZERO,
            cash_balances: Vec::new(),
        })
    }

    async fn position_snapshots(
        &self,
        _account_id: &str,
    ) -> Result<Vec<BrokerPositionSnapshot>, BrokerError> {
        Ok(Vec::new())
    }

    async fn open_orders(&self, _account_id: &str) -> Result<Vec<BrokerOpenOrder>, BrokerError> {
        Ok(Vec::new())
    }

    async fn executions(
        &self,
        _account_id: &str,
        symbol: Option<&str>,
    ) -> Result<Vec<BrokerExecution>, BrokerError> {
        if symbol != Some("CRYPTO:BINANCE:BTCUSDT:CRYPTO_SPOT") {
            return Ok(Vec::new());
        }
        Ok(vec![BrokerExecution {
            trade_id: "broker-native-exec".to_string(),
            broker_order_id: "broker-native-symbol".to_string(),
            client_order_id: Some("client-native-symbol".to_string()),
            account_id: "live-account".to_string(),
            symbol: "BTCUSDT".to_string(),
            side: OrderSide::Sell,
            price: dec("10001"),
            qty: dec("0.002"),
            fee: dec("0.01"),
            ts_ms: 2,
        }])
    }

    async fn status(&self) -> Result<BrokerStatus, BrokerError> {
        Ok(BrokerStatus {
            kind: BrokerKind::Binance,
            connected: true,
            trading_enabled: false,
            capabilities: broker::BrokerCapabilities {
                market_data: true,
                order_submit: false,
                order_cancel: true,
                paper_trading: true,
                live_trading: false,
            },
        })
    }
}

struct IncrementalExecutionRecoveryBroker;

#[async_trait]
impl Broker for IncrementalExecutionRecoveryBroker {
    async fn place_order(&self, _request: OrderRequest) -> Result<PlaceOrderResponse, BrokerError> {
        Err(BrokerError::Rejected(
            "test broker does not place orders".to_string(),
        ))
    }

    async fn cancel_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError> {
        Err(BrokerError::OrderNotFound(broker_order_id.to_string()))
    }

    async fn query_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError> {
        Err(BrokerError::OrderNotFound(broker_order_id.to_string()))
    }

    async fn account_snapshot(
        &self,
        account_id: &str,
    ) -> Result<BrokerAccountSnapshot, BrokerError> {
        Ok(BrokerAccountSnapshot {
            account_id: account_id.to_string(),
            cash: dec("100000"),
            equity: dec("100000"),
            buying_power: dec("100000"),
            margin_used: Decimal::ZERO,
            cash_balances: Vec::new(),
        })
    }

    async fn position_snapshots(
        &self,
        _account_id: &str,
    ) -> Result<Vec<BrokerPositionSnapshot>, BrokerError> {
        Ok(Vec::new())
    }

    async fn open_orders(&self, _account_id: &str) -> Result<Vec<BrokerOpenOrder>, BrokerError> {
        Ok(Vec::new())
    }

    async fn executions(
        &self,
        _account_id: &str,
        _symbol: Option<&str>,
    ) -> Result<Vec<BrokerExecution>, BrokerError> {
        Ok(vec![BrokerExecution {
            trade_id: "broker-exec-new".to_string(),
            broker_order_id: "broker-existing-fill".to_string(),
            client_order_id: Some("client-existing-fill".to_string()),
            account_id: "live-account".to_string(),
            symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
            side: OrderSide::Buy,
            price: dec("186"),
            qty: dec("1"),
            fee: dec("0.75"),
            ts_ms: 2,
        }])
    }

    async fn status(&self) -> Result<BrokerStatus, BrokerError> {
        Ok(BrokerStatus {
            kind: BrokerKind::InteractiveBrokers,
            connected: true,
            trading_enabled: false,
            capabilities: broker::BrokerCapabilities {
                market_data: true,
                order_submit: false,
                order_cancel: true,
                paper_trading: true,
                live_trading: false,
            },
        })
    }
}

struct OpenOrderOnlyPartialRecoveryBroker;

#[async_trait]
impl Broker for OpenOrderOnlyPartialRecoveryBroker {
    async fn place_order(&self, _request: OrderRequest) -> Result<PlaceOrderResponse, BrokerError> {
        Err(BrokerError::Rejected(
            "test broker does not place orders".to_string(),
        ))
    }

    async fn cancel_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError> {
        Err(BrokerError::OrderNotFound(broker_order_id.to_string()))
    }

    async fn query_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError> {
        Err(BrokerError::OrderNotFound(broker_order_id.to_string()))
    }

    async fn account_snapshot(
        &self,
        account_id: &str,
    ) -> Result<BrokerAccountSnapshot, BrokerError> {
        Ok(BrokerAccountSnapshot {
            account_id: account_id.to_string(),
            cash: dec("100000"),
            equity: dec("100000"),
            buying_power: dec("100000"),
            margin_used: Decimal::ZERO,
            cash_balances: Vec::new(),
        })
    }

    async fn position_snapshots(
        &self,
        _account_id: &str,
    ) -> Result<Vec<BrokerPositionSnapshot>, BrokerError> {
        Ok(Vec::new())
    }

    async fn open_orders(&self, _account_id: &str) -> Result<Vec<BrokerOpenOrder>, BrokerError> {
        Ok(vec![BrokerOpenOrder {
            broker_order_id: "broker-local-partial".to_string(),
            client_order_id: "client-local-partial".to_string(),
            account_id: "live-account".to_string(),
            symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            price: Some(dec("185")),
            qty: dec("2"),
            filled_qty: Decimal::ZERO,
            status: "SUBMITTED".to_string(),
        }])
    }

    async fn executions(
        &self,
        _account_id: &str,
        _symbol: Option<&str>,
    ) -> Result<Vec<BrokerExecution>, BrokerError> {
        Ok(Vec::new())
    }

    async fn status(&self) -> Result<BrokerStatus, BrokerError> {
        Ok(BrokerStatus {
            kind: BrokerKind::InteractiveBrokers,
            connected: true,
            trading_enabled: false,
            capabilities: broker::BrokerCapabilities {
                market_data: true,
                order_submit: false,
                order_cancel: true,
                paper_trading: true,
                live_trading: false,
            },
        })
    }
}

#[async_trait]
impl Broker for UnmatchedStartupRecoveryBroker {
    async fn place_order(&self, _request: OrderRequest) -> Result<PlaceOrderResponse, BrokerError> {
        Err(BrokerError::Rejected(
            "test broker does not place orders".to_string(),
        ))
    }

    async fn cancel_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError> {
        Err(BrokerError::OrderNotFound(broker_order_id.to_string()))
    }

    async fn query_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError> {
        Err(BrokerError::OrderNotFound(broker_order_id.to_string()))
    }

    async fn account_snapshot(
        &self,
        account_id: &str,
    ) -> Result<BrokerAccountSnapshot, BrokerError> {
        Ok(BrokerAccountSnapshot {
            account_id: account_id.to_string(),
            cash: dec("100000"),
            equity: dec("100000"),
            buying_power: dec("100000"),
            margin_used: Decimal::ZERO,
            cash_balances: Vec::new(),
        })
    }

    async fn position_snapshots(
        &self,
        _account_id: &str,
    ) -> Result<Vec<BrokerPositionSnapshot>, BrokerError> {
        Ok(Vec::new())
    }

    async fn open_orders(&self, _account_id: &str) -> Result<Vec<BrokerOpenOrder>, BrokerError> {
        Ok(vec![
            BrokerOpenOrder {
                broker_order_id: "broker-known".to_string(),
                client_order_id: "client-known".to_string(),
                account_id: "live-account".to_string(),
                symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
                side: OrderSide::Buy,
                order_type: OrderType::Limit,
                price: Some(dec("185")),
                qty: dec("1"),
                filled_qty: Decimal::ZERO,
                status: "SUBMITTED".to_string(),
            },
            BrokerOpenOrder {
                broker_order_id: "broker-unknown".to_string(),
                client_order_id: "client-unknown".to_string(),
                account_id: "live-account".to_string(),
                symbol: "US:NASDAQ:MSFT:EQUITY".to_string(),
                side: OrderSide::Sell,
                order_type: OrderType::Limit,
                price: Some(dec("300")),
                qty: dec("1"),
                filled_qty: Decimal::ZERO,
                status: "SUBMITTED".to_string(),
            },
        ])
    }

    async fn executions(
        &self,
        _account_id: &str,
        symbol: Option<&str>,
    ) -> Result<Vec<BrokerExecution>, BrokerError> {
        let executions = vec![
            BrokerExecution {
                trade_id: "broker-exec-known".to_string(),
                broker_order_id: "broker-known".to_string(),
                client_order_id: Some("client-known".to_string()),
                account_id: "live-account".to_string(),
                symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
                side: OrderSide::Buy,
                price: dec("186"),
                qty: dec("1"),
                fee: dec("1.25"),
                ts_ms: 2,
            },
            BrokerExecution {
                trade_id: "broker-exec-unknown".to_string(),
                broker_order_id: "broker-unknown".to_string(),
                client_order_id: Some("client-unknown".to_string()),
                account_id: "live-account".to_string(),
                symbol: "US:NASDAQ:MSFT:EQUITY".to_string(),
                side: OrderSide::Sell,
                price: dec("301"),
                qty: dec("1"),
                fee: dec("1.25"),
                ts_ms: 3,
            },
        ];
        Ok(executions
            .into_iter()
            .filter(|execution| symbol.is_none_or(|symbol| symbol == execution.symbol))
            .collect())
    }

    async fn status(&self) -> Result<BrokerStatus, BrokerError> {
        Ok(BrokerStatus {
            kind: BrokerKind::InteractiveBrokers,
            connected: true,
            trading_enabled: false,
            capabilities: broker::BrokerCapabilities {
                market_data: true,
                order_submit: false,
                order_cancel: true,
                paper_trading: true,
                live_trading: false,
            },
        })
    }
}

struct UnmatchedExecutionOnlyStartupRecoveryBroker;

#[async_trait]
impl Broker for UnmatchedExecutionOnlyStartupRecoveryBroker {
    async fn place_order(&self, _request: OrderRequest) -> Result<PlaceOrderResponse, BrokerError> {
        Err(BrokerError::Rejected(
            "test broker does not place orders".to_string(),
        ))
    }

    async fn cancel_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError> {
        Err(BrokerError::OrderNotFound(broker_order_id.to_string()))
    }

    async fn query_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError> {
        Err(BrokerError::OrderNotFound(broker_order_id.to_string()))
    }

    async fn account_snapshot(
        &self,
        account_id: &str,
    ) -> Result<BrokerAccountSnapshot, BrokerError> {
        Ok(BrokerAccountSnapshot {
            account_id: account_id.to_string(),
            cash: dec("100000"),
            equity: dec("100000"),
            buying_power: dec("100000"),
            margin_used: Decimal::ZERO,
            cash_balances: Vec::new(),
        })
    }

    async fn position_snapshots(
        &self,
        _account_id: &str,
    ) -> Result<Vec<BrokerPositionSnapshot>, BrokerError> {
        Ok(Vec::new())
    }

    async fn open_orders(&self, _account_id: &str) -> Result<Vec<BrokerOpenOrder>, BrokerError> {
        Ok(vec![BrokerOpenOrder {
            broker_order_id: "broker-known-exec-only".to_string(),
            client_order_id: "client-known-exec-only".to_string(),
            account_id: "live-account".to_string(),
            symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            price: Some(dec("185")),
            qty: dec("1"),
            filled_qty: Decimal::ZERO,
            status: "SUBMITTED".to_string(),
        }])
    }

    async fn executions(
        &self,
        _account_id: &str,
        _symbol: Option<&str>,
    ) -> Result<Vec<BrokerExecution>, BrokerError> {
        Ok(vec![
            BrokerExecution {
                trade_id: "broker-exec-known-only".to_string(),
                broker_order_id: "broker-known-exec-only".to_string(),
                client_order_id: Some("client-known-exec-only".to_string()),
                account_id: "live-account".to_string(),
                symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
                side: OrderSide::Buy,
                price: dec("186"),
                qty: dec("1"),
                fee: dec("1.25"),
                ts_ms: 2,
            },
            BrokerExecution {
                trade_id: "broker-exec-unknown".to_string(),
                broker_order_id: "broker-unknown-exec-only".to_string(),
                client_order_id: Some("client-unknown-exec-only".to_string()),
                account_id: "live-account".to_string(),
                symbol: "US:NASDAQ:MSFT:EQUITY".to_string(),
                side: OrderSide::Sell,
                price: dec("301"),
                qty: dec("1"),
                fee: dec("1.25"),
                ts_ms: 3,
            },
        ])
    }

    async fn status(&self) -> Result<BrokerStatus, BrokerError> {
        Ok(BrokerStatus {
            kind: BrokerKind::InteractiveBrokers,
            connected: true,
            trading_enabled: false,
            capabilities: broker::BrokerCapabilities {
                market_data: true,
                order_submit: false,
                order_cancel: true,
                paper_trading: true,
                live_trading: false,
            },
        })
    }
}

struct StartupRecoveryFailureBroker;

#[async_trait]
impl Broker for StartupRecoveryFailureBroker {
    async fn place_order(&self, _request: OrderRequest) -> Result<PlaceOrderResponse, BrokerError> {
        Err(BrokerError::Rejected(
            "test broker does not place orders".to_string(),
        ))
    }

    async fn cancel_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError> {
        Err(BrokerError::OrderNotFound(broker_order_id.to_string()))
    }

    async fn query_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError> {
        Err(BrokerError::OrderNotFound(broker_order_id.to_string()))
    }

    async fn account_snapshot(
        &self,
        account_id: &str,
    ) -> Result<BrokerAccountSnapshot, BrokerError> {
        Ok(BrokerAccountSnapshot {
            account_id: account_id.to_string(),
            cash: dec("100000"),
            equity: dec("100000"),
            buying_power: dec("100000"),
            margin_used: Decimal::ZERO,
            cash_balances: Vec::new(),
        })
    }

    async fn position_snapshots(
        &self,
        _account_id: &str,
    ) -> Result<Vec<BrokerPositionSnapshot>, BrokerError> {
        Ok(Vec::new())
    }

    async fn open_orders(&self, _account_id: &str) -> Result<Vec<BrokerOpenOrder>, BrokerError> {
        Err(BrokerError::Connection("gateway unavailable".to_string()))
    }

    async fn status(&self) -> Result<BrokerStatus, BrokerError> {
        Ok(BrokerStatus {
            kind: BrokerKind::InteractiveBrokers,
            connected: false,
            trading_enabled: false,
            capabilities: broker::BrokerCapabilities {
                market_data: false,
                order_submit: false,
                order_cancel: false,
                paper_trading: true,
                live_trading: false,
            },
        })
    }
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
            alert_sink: AlertSinkSettings::Noop,
            logging: Default::default(),
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
async fn live_runtime_keeps_position_reconciliation_alerts_distinct_by_side() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let run_id = "live-position-side-drift";
    let live = LiveRuntime::new_with_broker(
        db.clone(),
        LiveRuntimeSettings {
            run_id: run_id.to_string(),
            broker_kind: BrokerKind::InteractiveBrokers,
            account_id: "live-account".to_string(),
            base_currency: "USD".to_string(),
            initial_cash: dec("100000"),
            broker_snapshot_interval_ms: Some(5),
            alert_sink: AlertSinkSettings::Noop,
            logging: Default::default(),
        },
        Arc::new(TwoSidedPositionSnapshotBroker),
    );
    let cancel = CancellationFlag::default();
    let task_cancel = cancel.clone();
    let handle = tokio::spawn(async move { live.run(task_cancel).await.unwrap() });

    wait_for_reconciliation_position_drift_count(&db, run_id, 2).await;

    cancel.cancel();
    handle.await.unwrap();

    let reconciliation_logs = db
        .list_system_logs_filtered(SystemLogFilter {
            run_id: Some(run_id.to_string()),
            target: Some("runtime.reconciliation".to_string()),
            ..SystemLogFilter::default()
        })
        .await
        .unwrap();
    let drift_sides = reconciliation_logs
        .iter()
        .filter_map(|log| {
            if log.message != "reconciliation.drift" {
                return None;
            }
            let fields = log.fields_json.as_deref()?;
            let fields = serde_json::from_str::<serde_json::Value>(fields).ok()?;
            (fields["reason"].as_str() == Some("position_missing_runtime"))
                .then(|| fields["position_side"].as_str().map(str::to_string))?
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        drift_sides,
        ["long".to_string(), "short".to_string()]
            .into_iter()
            .collect()
    );

    let alert_logs = db
        .list_system_logs_filtered(SystemLogFilter {
            run_id: Some(run_id.to_string()),
            target: Some("runtime.alert".to_string()),
            ..SystemLogFilter::default()
        })
        .await
        .unwrap();
    let alert_sides = alert_logs
        .iter()
        .filter_map(|log| {
            if log.message != "reconciliation_drift.alert" {
                return None;
            }
            let fields = log.fields_json.as_deref()?;
            let fields = serde_json::from_str::<serde_json::Value>(fields).ok()?;
            (fields["reason"].as_str() == Some("position_missing_runtime"))
                .then(|| fields["position_side"].as_str().map(str::to_string))?
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        alert_sides,
        ["long".to_string(), "short".to_string()]
            .into_iter()
            .collect()
    );
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
            alert_sink: AlertSinkSettings::Noop,
            logging: Default::default(),
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
        contract_metadata_json: None,
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
            alert_sink: AlertSinkSettings::Noop,
            logging: Default::default(),
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
        log.level == "INFO"
            && log.message == "live runtime started"
            && log
                .fields_json
                .as_deref()
                .is_some_and(|fields| fields.contains("\"category\":\"system\""))
    }));
    assert!(logs.iter().any(|log| {
        log.level == "INFO" && log.target == "runtime.live" && log.message == "live.stopped"
    }));
    assert!(logs.iter().any(|log| {
        log.level == "INFO"
            && log.message == "live runtime stopped"
            && log
                .fields_json
                .as_deref()
                .is_some_and(|fields| fields.contains("\"category\":\"system\""))
    }));
    assert!(logs.iter().any(|log| {
        log.level == "INFO"
            && log.target == "runtime.broker_snapshot"
            && log.message == "broker.snapshot.cash"
    }));
    assert!(logs.iter().any(|log| {
        log.level == "INFO"
            && log.message == "live broker cash snapshot captured"
            && log
                .fields_json
                .as_deref()
                .is_some_and(|fields| fields.contains("\"category\":\"broker\""))
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
    assert!(logs.iter().any(|log| {
        log.level == "ERROR"
            && log.target == "runtime.alert"
            && log.message == "reconciliation_drift.alert"
            && log
                .fields_json
                .as_deref()
                .is_some_and(|fields| fields.contains("reconciliation_drift"))
    }));
}

#[tokio::test]
async fn live_runtime_writes_reconciliation_alert_to_file_sink_when_configured() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let alert_file = std::env::temp_dir().join(format!(
        "trader-live-alert-{}.jsonl",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let live = LiveRuntime::new(
        db.clone(),
        LiveRuntimeSettings {
            run_id: "live-alert-file".to_string(),
            broker_kind: BrokerKind::Binance,
            account_id: "live-account".to_string(),
            base_currency: "USDT".to_string(),
            initial_cash: dec("25000"),
            broker_snapshot_interval_ms: Some(5),
            alert_sink: AlertSinkSettings::File {
                path: alert_file.display().to_string(),
                cooldown_ms: 60_000,
            },
            logging: Default::default(),
        },
    );
    let cancel = CancellationFlag::default();
    let task_cancel = cancel.clone();
    let handle = tokio::spawn(async move { live.run(task_cancel).await.unwrap() });

    wait_for_system_log(&db, "live-alert-file", "runtime.alert").await;

    cancel.cancel();
    handle.await.unwrap();

    let content = std::fs::read_to_string(&alert_file).unwrap();
    assert!(content.contains("\"target\":\"runtime.alert\""));
    assert!(content.contains("\"message\":\"reconciliation_drift.alert\""));
    assert!(content.contains("\"dedup_key\":\"reconciliation_drift.alert|live-alert-file|live-account||USDT|cash_total_drift\""));
    assert!(content.contains("\"run_id\":\"live-alert-file\""));

    let _ = std::fs::remove_file(alert_file);
}

#[tokio::test]
async fn live_runtime_posts_reconciliation_alert_to_webhook_sink_when_configured() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        tokio::io::AsyncWriteExt::write_all(
            &mut stream,
            b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\ncontent-type: text/plain\r\n\r\nok",
        )
        .await
        .unwrap();
        request
    });
    let live = LiveRuntime::new(
        db.clone(),
        LiveRuntimeSettings {
            run_id: "live-alert-webhook".to_string(),
            broker_kind: BrokerKind::Binance,
            account_id: "live-account".to_string(),
            base_currency: "USDT".to_string(),
            initial_cash: dec("25000"),
            broker_snapshot_interval_ms: Some(5),
            alert_sink: AlertSinkSettings::Webhook {
                url: format!("http://{addr}/alerts"),
                cooldown_ms: 60_000,
                timeout_ms: 1_000,
                max_retries: 0,
                auth_token: None,
            },
            logging: Default::default(),
        },
    );
    let cancel = CancellationFlag::default();
    let task_cancel = cancel.clone();
    let handle = tokio::spawn(async move { live.run(task_cancel).await.unwrap() });

    wait_for_system_log(&db, "live-alert-webhook", "runtime.alert").await;

    cancel.cancel();
    handle.await.unwrap();
    let request = server.await.unwrap();
    assert!(request.starts_with("POST /alerts HTTP/1.1"));
    assert!(request.contains("\"target\":\"runtime.alert\""));
    assert!(request.contains("\"message\":\"reconciliation_drift.alert\""));
    assert!(request.contains("\"dedup_key\":\"reconciliation_drift.alert|live-alert-webhook|live-account||USDT|cash_total_drift\""));
    let delivery_logs = db
        .list_system_logs_filtered(SystemLogFilter {
            run_id: Some("live-alert-webhook".to_string()),
            level: None,
            target: Some("runtime.alert_delivery".to_string()),
            from_ms: None,
            to_ms: None,
            search: None,
            limit: None,
            offset: None,
        })
        .await
        .unwrap();
    assert!(delivery_logs.iter().any(|log| {
        log.level == "INFO"
            && log.fields_json.as_deref().is_some_and(|fields| {
                fields.contains("\"status\":\"sent\"") && fields.contains("\"http_status\":200")
            })
    }));
}

#[tokio::test]
async fn live_runtime_sends_reconciliation_alert_to_all_configured_sinks() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let alert_file = std::env::temp_dir().join(format!(
        "trader-live-alert-multi-{}.jsonl",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        tokio::io::AsyncWriteExt::write_all(
            &mut stream,
            b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\ncontent-type: text/plain\r\n\r\nok",
        )
        .await
        .unwrap();
        request
    });
    let live = LiveRuntime::new(
        db.clone(),
        LiveRuntimeSettings {
            run_id: "live-alert-multi".to_string(),
            broker_kind: BrokerKind::Binance,
            account_id: "live-account".to_string(),
            base_currency: "USDT".to_string(),
            initial_cash: dec("25000"),
            broker_snapshot_interval_ms: Some(5),
            alert_sink: AlertSinkSettings::Multi(vec![
                AlertSinkSettings::File {
                    path: alert_file.display().to_string(),
                    cooldown_ms: 60_000,
                },
                AlertSinkSettings::Webhook {
                    url: format!("http://{addr}/alerts"),
                    cooldown_ms: 60_000,
                    timeout_ms: 1_000,
                    max_retries: 0,
                    auth_token: None,
                },
            ]),
            logging: Default::default(),
        },
    );
    let cancel = CancellationFlag::default();
    let task_cancel = cancel.clone();
    let handle = tokio::spawn(async move { live.run(task_cancel).await.unwrap() });

    wait_for_system_log(&db, "live-alert-multi", "runtime.alert").await;

    cancel.cancel();
    handle.await.unwrap();
    let request = server.await.unwrap();
    assert!(request.starts_with("POST /alerts HTTP/1.1"));
    assert!(request.contains("\"message\":\"reconciliation_drift.alert\""));
    let content = std::fs::read_to_string(&alert_file).unwrap();
    assert!(content.contains("\"message\":\"reconciliation_drift.alert\""));
    let delivery_logs = db
        .list_system_logs_filtered(SystemLogFilter {
            run_id: Some("live-alert-multi".to_string()),
            level: None,
            target: Some("runtime.alert_delivery".to_string()),
            from_ms: None,
            to_ms: None,
            search: None,
            limit: None,
            offset: None,
        })
        .await
        .unwrap();
    assert!(delivery_logs.iter().any(|log| {
        log.level == "INFO"
            && log
                .fields_json
                .as_deref()
                .is_some_and(|fields| fields.contains("\"sink\":\"file\""))
    }));
    assert!(delivery_logs.iter().any(|log| {
        log.level == "INFO"
            && log
                .fields_json
                .as_deref()
                .is_some_and(|fields| fields.contains("\"sink\":\"webhook\""))
    }));

    let _ = std::fs::remove_file(alert_file);
}

#[tokio::test]
async fn live_runtime_retries_webhook_alert_with_auth_header() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let mut requests = Vec::new();
        for status_line in [
            "HTTP/1.1 500 Internal Server Error\r\ncontent-length: 5\r\ncontent-type: text/plain\r\n\r\nerror",
            "HTTP/1.1 200 OK\r\ncontent-length: 2\r\ncontent-type: text/plain\r\n\r\nok",
        ] {
            let (mut stream, _) = listener.accept().await.unwrap();
            requests.push(read_http_request(&mut stream).await);
            tokio::io::AsyncWriteExt::write_all(&mut stream, status_line.as_bytes())
                .await
                .unwrap();
        }
        requests
    });
    let live = LiveRuntime::new(
        db.clone(),
        LiveRuntimeSettings {
            run_id: "live-alert-webhook-retry".to_string(),
            broker_kind: BrokerKind::Binance,
            account_id: "live-account".to_string(),
            base_currency: "USDT".to_string(),
            initial_cash: dec("25000"),
            broker_snapshot_interval_ms: Some(5),
            alert_sink: AlertSinkSettings::Webhook {
                url: format!("http://{addr}/alerts"),
                cooldown_ms: 60_000,
                timeout_ms: 1_000,
                max_retries: 1,
                auth_token: Some("secret-token".to_string()),
            },
            logging: Default::default(),
        },
    );
    let cancel = CancellationFlag::default();
    let task_cancel = cancel.clone();
    let handle = tokio::spawn(async move { live.run(task_cancel).await.unwrap() });

    wait_for_system_log(&db, "live-alert-webhook-retry", "runtime.alert").await;

    cancel.cancel();
    handle.await.unwrap();
    let requests = server.await.unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests[0].starts_with("POST /alerts HTTP/1.1"));
    assert!(
        requests[0].contains("authorization: Bearer secret-token")
            || requests[0].contains("Authorization: Bearer secret-token")
    );
    assert!(requests[1].contains("\"message\":\"reconciliation_drift.alert\""));
    let delivery_logs = db
        .list_system_logs_filtered(SystemLogFilter {
            run_id: Some("live-alert-webhook-retry".to_string()),
            level: None,
            target: Some("runtime.alert_delivery".to_string()),
            from_ms: None,
            to_ms: None,
            search: None,
            limit: None,
            offset: None,
        })
        .await
        .unwrap();
    assert!(delivery_logs.iter().any(|log| {
        log.level == "INFO"
            && log.fields_json.as_deref().is_some_and(|fields| {
                fields.contains("\"status\":\"sent\"") && fields.contains("\"attempts\":2")
            })
    }));
}

#[tokio::test]
async fn live_runtime_does_not_retry_webhook_alert_on_client_error_and_logs_failure() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        tokio::io::AsyncWriteExt::write_all(
            &mut stream,
            b"HTTP/1.1 400 Bad Request\r\ncontent-length: 3\r\ncontent-type: text/plain\r\n\r\nbad",
        )
        .await
        .unwrap();
        request
    });
    let live = LiveRuntime::new(
        db.clone(),
        LiveRuntimeSettings {
            run_id: "live-alert-webhook-400".to_string(),
            broker_kind: BrokerKind::Binance,
            account_id: "live-account".to_string(),
            base_currency: "USDT".to_string(),
            initial_cash: dec("25000"),
            broker_snapshot_interval_ms: Some(5),
            alert_sink: AlertSinkSettings::Webhook {
                url: format!("http://{addr}/alerts"),
                cooldown_ms: 60_000,
                timeout_ms: 1_000,
                max_retries: 3,
                auth_token: None,
            },
            logging: Default::default(),
        },
    );
    let cancel = CancellationFlag::default();
    let task_cancel = cancel.clone();
    let handle = tokio::spawn(async move { live.run(task_cancel).await.unwrap() });

    wait_for_system_log(&db, "live-alert-webhook-400", "runtime.alert").await;

    cancel.cancel();
    handle.await.unwrap();
    let request = server.await.unwrap();
    assert!(request.starts_with("POST /alerts HTTP/1.1"));
    let delivery_logs = db
        .list_system_logs_filtered(SystemLogFilter {
            run_id: Some("live-alert-webhook-400".to_string()),
            level: None,
            target: Some("runtime.alert_delivery".to_string()),
            from_ms: None,
            to_ms: None,
            search: None,
            limit: None,
            offset: None,
        })
        .await
        .unwrap();
    assert!(delivery_logs.iter().any(|log| {
        log.level == "WARN"
            && log.fields_json.as_deref().is_some_and(|fields| {
                fields.contains("\"status\":\"failed\"")
                    && fields.contains("\"http_status\":400")
                    && fields.contains("\"attempts\":1")
            })
    }));
}

#[tokio::test]
async fn live_runtime_suppresses_duplicate_file_sink_alerts_within_cooldown() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let alert_file = std::env::temp_dir().join(format!(
        "trader-live-alert-dedup-{}.jsonl",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    db.record_system_log(storage::SystemLogCommand {
        run_id: Some("live-alert-dedup".to_string()),
        ts_ms: chrono::Utc::now().timestamp_millis(),
        level: "ERROR".to_string(),
        target: "runtime.alert".to_string(),
        message: "reconciliation_drift.alert".to_string(),
        fields: Some(serde_json::json!({
            "run_id": "live-alert-dedup",
            "account_id": "live-account",
            "risk_type": "reconciliation_drift",
            "reason": "cash_total_drift",
            "threshold": "0",
            "observed_value": "75000",
            "runtime_cash": "25000",
            "broker_cash": "100000",
            "currency": "USDT",
        })),
    })
    .await
    .unwrap();
    let live = LiveRuntime::new(
        db.clone(),
        LiveRuntimeSettings {
            run_id: "live-alert-dedup".to_string(),
            broker_kind: BrokerKind::Binance,
            account_id: "live-account".to_string(),
            base_currency: "USDT".to_string(),
            initial_cash: dec("25000"),
            broker_snapshot_interval_ms: Some(5),
            alert_sink: AlertSinkSettings::File {
                path: alert_file.display().to_string(),
                cooldown_ms: 60_000,
            },
            logging: Default::default(),
        },
    );
    let cancel = CancellationFlag::default();
    let task_cancel = cancel.clone();
    let handle = tokio::spawn(async move { live.run(task_cancel).await.unwrap() });

    wait_for_system_log(&db, "live-alert-dedup", "runtime.reconciliation").await;

    cancel.cancel();
    handle.await.unwrap();

    let content = std::fs::read_to_string(&alert_file).unwrap_or_default();
    assert!(!content.contains("\"reason\":\"cash_total_drift\""));
    let alert_logs = db
        .list_system_logs_filtered(SystemLogFilter {
            run_id: Some("live-alert-dedup".to_string()),
            level: None,
            target: Some("runtime.alert".to_string()),
            from_ms: None,
            to_ms: None,
            search: None,
            limit: None,
            offset: None,
        })
        .await
        .unwrap();
    assert!(alert_logs.len() >= 2);

    let _ = std::fs::remove_file(alert_file);
}

fn dec(value: &str) -> Decimal {
    value.parse().unwrap()
}

async fn seed_external_order(
    db: &Db,
    run_id: &str,
    order_id: &str,
    client_order_id: &str,
    broker_order_id: &str,
    symbol: &str,
    side: &str,
    qty: &str,
    filled_qty: &str,
    status: &str,
) {
    db.record_external_order(ExternalOrderCommand {
        run_id: run_id.to_string(),
        order_id: order_id.to_string(),
        client_order_id: client_order_id.to_string(),
        broker_order_id: Some(broker_order_id.to_string()),
        account_id: "live-account".to_string(),
        symbol: symbol.to_string(),
        side: side.to_string(),
        order_type: "LIMIT".to_string(),
        price: Some(dec("185.00")),
        qty: dec(qty),
        filled_qty: dec(filled_qty),
        status: status.to_string(),
        ts_ms: 1,
    })
    .await
    .unwrap();
}

async fn seed_external_fill(
    db: &Db,
    run_id: &str,
    order_id: &str,
    fill_id: &str,
    symbol: &str,
    side: &str,
    price: &str,
    qty: &str,
    fee: &str,
) {
    db.record_external_fill(ExternalFillCommand {
        id: fill_id.to_string(),
        order_id: order_id.to_string(),
        run_id: run_id.to_string(),
        symbol: symbol.to_string(),
        side: side.to_string(),
        price: dec(price),
        qty: dec(qty),
        fee: dec(fee),
        ts_ms: 1,
    })
    .await
    .unwrap();
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

async fn wait_for_runtime_event_category(db: &Db, run_id: &str, category: &str) {
    for _ in 0..50 {
        if db
            .list_events_by_source(run_id)
            .await
            .unwrap()
            .iter()
            .any(|event| event.category == category)
        {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("{run_id} did not record runtime event {category}");
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

async fn wait_for_system_log_message_contains(
    db: &Db,
    run_id: &str,
    target: &str,
    message: &str,
    expected_field: &str,
) {
    for _ in 0..50 {
        if db
            .list_system_logs_filtered(SystemLogFilter {
                run_id: Some(run_id.to_string()),
                target: Some(target.to_string()),
                ..SystemLogFilter::default()
            })
            .await
            .unwrap()
            .iter()
            .any(|log| {
                log.message == message
                    && log
                        .fields_json
                        .as_deref()
                        .is_some_and(|fields| fields.contains(expected_field))
            })
        {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("{run_id} did not emit system log {target} {message} containing {expected_field}");
}

async fn wait_for_reconciliation_audit(db: &Db, run_id: &str) {
    for _ in 0..50 {
        if !db
            .list_reconciliation_audits(run_id)
            .await
            .unwrap()
            .is_empty()
        {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("{run_id} did not record reconciliation audit");
}

async fn wait_for_reconciliation_position_drift(db: &Db, run_id: &str) {
    for _ in 0..50 {
        if db
            .list_reconciliation_audits(run_id)
            .await
            .unwrap()
            .iter()
            .any(|audit| audit.position_drift_count > 0)
        {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("{run_id} did not record reconciliation position drift");
}

async fn wait_for_reconciliation_position_drift_count(db: &Db, run_id: &str, min_count: usize) {
    for _ in 0..50 {
        if db
            .list_reconciliation_audits(run_id)
            .await
            .unwrap()
            .iter()
            .any(|audit| audit.position_drift_count as usize >= min_count)
        {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("{run_id} did not record {min_count} reconciliation position drifts");
}

async fn wait_for_reconciliation_position_clean(db: &Db, run_id: &str) {
    for _ in 0..50 {
        if db
            .list_reconciliation_audits(run_id)
            .await
            .unwrap()
            .iter()
            .any(|audit| audit.position_drift_count == 0)
        {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("{run_id} did not record clean reconciliation position audit");
}

async fn wait_for_reconciliation_cash_drift_count(db: &Db, run_id: &str, min_count: usize) {
    for _ in 0..50 {
        if db
            .list_reconciliation_audits(run_id)
            .await
            .unwrap()
            .iter()
            .any(|audit| audit.cash_drift_count as usize >= min_count)
        {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("{run_id} did not record {min_count} reconciliation cash drifts");
}

async fn wait_for_reconciliation_open_order_drift(db: &Db, run_id: &str) {
    for _ in 0..50 {
        if db
            .list_reconciliation_audits(run_id)
            .await
            .unwrap()
            .iter()
            .any(|audit| audit.open_order_drift_count > 0)
        {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("{run_id} did not record reconciliation open order drift");
}

async fn wait_for_reconciliation_open_order_drift_count(db: &Db, run_id: &str, min_count: usize) {
    for _ in 0..100 {
        let count = db
            .list_reconciliation_audits(run_id)
            .await
            .unwrap()
            .iter()
            .filter(|audit| audit.open_order_drift_count > 0)
            .count();
        if count >= min_count {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("{run_id} did not record {min_count} reconciliation open order drift audits");
}

async fn read_http_request(stream: &mut tokio::net::TcpStream) -> String {
    let mut bytes = Vec::new();
    let mut buf = [0u8; 1024];
    loop {
        let size = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            tokio::io::AsyncReadExt::read(stream, &mut buf),
        )
        .await
        .unwrap()
        .unwrap();
        if size == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..size]);
        if request_body_complete(&bytes) {
            break;
        }
    }
    String::from_utf8_lossy(&bytes).to_string()
}

fn request_body_complete(bytes: &[u8]) -> bool {
    let Some(header_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") else {
        return false;
    };
    let header_text = String::from_utf8_lossy(&bytes[..header_end]);
    let content_length = header_text
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
        .unwrap_or(0);
    bytes.len() >= header_end + 4 + content_length
}
