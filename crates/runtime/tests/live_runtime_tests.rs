use async_trait::async_trait;
use broker::{
    Broker, BrokerAccountSnapshot, BrokerError, BrokerExecution, BrokerKind, BrokerOpenOrder,
    BrokerOrder, BrokerPositionSide, BrokerPositionSnapshot, BrokerStatus, PlaceOrderResponse,
};
use runtime::{AlertSinkSettings, CancellationFlag, LiveRuntime, LiveRuntimeSettings};
use rust_decimal::Decimal;
use std::sync::Arc;
use storage::{Db, NewOrder, RuntimePositionSnapshotCommand, SystemLogFilter};
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
    db.insert_order(NewOrder {
        id: "order-recover".to_string(),
        run_id: "live-startup-recovery".to_string(),
        client_order_id: "client-recover".to_string(),
        broker_order_id: Some("broker-recover".to_string()),
        account_id: "live-account".to_string(),
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        side: "BUY".to_string(),
        order_type: "LIMIT".to_string(),
        price: Some("185.00".to_string()),
        qty: "2".to_string(),
        filled_qty: "0".to_string(),
        status: "SUBMITTED".to_string(),
        created_at_ms: 1,
        updated_at_ms: 1,
    })
    .await
    .unwrap();

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
}

#[tokio::test]
async fn live_runtime_logs_unmatched_broker_orders_and_executions_on_startup() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    db.insert_order(NewOrder {
        id: "order-known".to_string(),
        run_id: "live-startup-unmatched".to_string(),
        client_order_id: "client-known".to_string(),
        broker_order_id: Some("broker-known".to_string()),
        account_id: "live-account".to_string(),
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        side: "BUY".to_string(),
        order_type: "LIMIT".to_string(),
        price: Some("185.00".to_string()),
        qty: "1".to_string(),
        filled_qty: "0".to_string(),
        status: "SUBMITTED".to_string(),
        created_at_ms: 1,
        updated_at_ms: 1,
    })
    .await
    .unwrap();

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
    let cancel = CancellationFlag::default();
    let task_cancel = cancel.clone();
    let handle = tokio::spawn(async move { live.run(task_cancel).await.unwrap() });

    wait_for_system_log(&db, "live-startup-unmatched", "runtime.startup_recovery").await;

    cancel.cancel();
    handle.await.unwrap();

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
        account_id: &str,
    ) -> Result<BrokerAccountSnapshot, BrokerError> {
        Ok(BrokerAccountSnapshot {
            account_id: account_id.to_string(),
            cash: dec("123456"),
            equity: dec("123456"),
            buying_power: dec("123456"),
            margin_used: Decimal::ZERO,
        })
    }

    async fn position_snapshots(
        &self,
        account_id: &str,
    ) -> Result<Vec<BrokerPositionSnapshot>, BrokerError> {
        Ok(vec![BrokerPositionSnapshot {
            account_id: account_id.to_string(),
            exchange: "IBKR".to_string(),
            symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
            position_side: BrokerPositionSide::Long,
            qty: dec("2"),
            avg_price: dec("185.25"),
            margin_used: Decimal::ZERO,
            unrealized_pnl: Decimal::ZERO,
            ts_ms: 1_700_000_000_000,
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
        _symbol: Option<&str>,
    ) -> Result<Vec<BrokerExecution>, BrokerError> {
        Ok(vec![
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
    assert!(content.contains("\"dedup_key\":\"reconciliation_drift.alert|live-alert-file|live-account||cash_total_drift\""));
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
    assert!(request.contains("\"dedup_key\":\"reconciliation_drift.alert|live-alert-webhook|live-account||cash_total_drift\""));
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
