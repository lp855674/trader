use broker::BrokerKind;
use runtime::{AlertSinkSettings, CancellationFlag, LiveRuntime, LiveRuntimeSettings};
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
