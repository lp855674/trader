use api::router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use runtime::RuntimeRunMetadata;
use rust_decimal::Decimal;
use std::path::PathBuf;
use std::sync::Arc;
use storage::{
    CorporateActionMetaCommand, CryptoMarketMetaCommand, CryptoPositionCommand, Db,
    FundingRateCommand, NewAccountBalance, NewConfigVersion, NewFill, NewOrder,
    NewPortfolioSnapshot, PaperPortfolioSnapshotCommand, PositionCommand, RuntimeEventCommand,
    StrategyRunStartCommand, SystemLogCommand,
};
use tokio::sync::Notify;
use tower::ServiceExt;

#[tokio::test]
async fn health_returns_ok() {
    let response = router()
        .oneshot(
            Request::builder()
                .uri("/api/v1/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn api_requests_are_captured_as_system_logs() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = api::router_with_state(api::AppState::new(
        db.clone(),
        "configs/backtest/ma_cross.toml".into(),
    ));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let logs = db
        .list_system_logs_filtered(storage::SystemLogFilter {
            search: Some("/api/v1/health".to_string()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(
        logs.iter().any(|log| log.message == "api request completed"
            && log.level == "INFO"
            && log
                .fields_json
                .as_deref()
                .is_some_and(|fields| fields.contains("\"status\":200"))),
        "{logs:?}"
    );
}

#[tokio::test]
async fn logging_metrics_route_reads_shared_writer_metrics() {
    use events::SystemLogLayer;
    use tracing_subscriber::prelude::*;

    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let state = api::AppState::new(db.clone(), "configs/backtest/ma_cross.toml".into());
    let (layer_tx, _rx_guard) = tokio::sync::mpsc::channel(1);
    let subscriber = tracing_subscriber::registry()
        .with(SystemLogLayer::new(layer_tx, None).with_metrics(state.log_writer_metrics.clone()));
    let dispatch = tracing::Dispatch::new(subscriber);
    tracing::dispatcher::with_default(&dispatch, || {
        tracing::info!("first log fills shared api metrics channel");
        tracing::info!("second log increments shared api dropped count");
    });

    let response = api::router_with_state(state)
        .oneshot(
            Request::builder()
                .uri("/api/v1/ops/logging/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("\"dropped_logs\":1"), "{body}");
    assert!(body.contains("\"buffer_size\":1000"), "{body}");
}

#[tokio::test]
async fn logging_retention_scheduler_purges_old_system_logs() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let now_ms = chrono::Utc::now().timestamp_millis();
    db.record_system_log(SystemLogCommand {
        run_id: Some("retention-scheduler".to_string()),
        ts_ms: now_ms - 2 * 86_400_000,
        level: "INFO".to_string(),
        target: "scheduler.test".to_string(),
        message: "old log".to_string(),
        fields: None,
    })
    .await
    .unwrap();
    db.record_system_log(SystemLogCommand {
        run_id: Some("retention-scheduler".to_string()),
        ts_ms: now_ms,
        level: "INFO".to_string(),
        target: "scheduler.test".to_string(),
        message: "new log".to_string(),
        fields: None,
    })
    .await
    .unwrap();

    let config_path = std::env::temp_dir().join(format!(
        "trader-logging-retention-{}.toml",
        std::process::id()
    ));
    std::fs::write(
        &config_path,
        r#"
        [runtime]
        mode = "backtest"
        run_id = "retention-scheduler"

        [database]
        url = "sqlite::memory:"

        [data]
        source = "csv"
        path = "datasets/sample/aapl_1d.csv"

        [strategy]
        name = "moving_average_cross"
        symbols = ["US:NASDAQ:AAPL:EQUITY"]
        fast_window = 2
        slow_window = 3

        [portfolio]
        initial_cash = "100000"
        base_currency = "USD"
        order_qty = "1"
        max_abs_qty = "100"

        [risk]
        max_order_notional = "1000000"
        min_cash_after_order = "0"
        max_exposure = "1000000"
        max_drawdown = "1"
        max_leverage = "1"
        max_margin_used = "0"
        trading_halted = false

        [broker]
        kind = "simulated"
        mode = "paper"

        [paper]
        account_id = "paper"
        slippage_bps = "0"
        fee_bps = "0"

        [live]
        enabled = false

        [logging]
        retention_days = 1
        "#,
    )
    .unwrap();

    let scheduler = api::spawn_logging_retention_scheduler(
        db.clone(),
        config_path.display().to_string(),
        std::time::Duration::from_millis(5),
    );

    let mut remaining = Vec::new();
    for _ in 0..20 {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        remaining = db
            .list_system_logs_filtered(storage::SystemLogFilter {
                run_id: Some("retention-scheduler".to_string()),
                ..Default::default()
            })
            .await
            .unwrap();
        if remaining.len() == 1 {
            break;
        }
    }
    scheduler.abort();
    let _ = scheduler.await;
    std::fs::remove_file(config_path).unwrap();

    assert_eq!(remaining.len(), 1, "{remaining:?}");
    assert_eq!(remaining[0].message, "new log");
}

#[tokio::test]
async fn broker_status_returns_v1_fake_connectors() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let response = api::router_with_state(api::AppState::new(
        db,
        "configs/backtest/ma_cross.toml".into(),
    ))
    .oneshot(
        Request::builder()
            .uri("/api/v1/brokers/status")
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("\"kind\":\"FUTU\""));
    assert!(body.contains("\"kind\":\"BINANCE\""));
    assert!(body.contains("\"kind\":\"OKX\""));
    assert!(body.contains("\"kind\":\"INTERACTIVE_BROKERS\""));
}

#[tokio::test]
async fn broker_account_returns_configured_fake_snapshot() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let response = api::router_with_state(api::AppState::new(
        db,
        "configs/backtest/ma_cross.toml".into(),
    ))
    .oneshot(
        Request::builder()
            .uri("/api/v1/brokers/account/paper?broker=simulated")
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("\"account_id\":\"paper\""));
    assert!(body.contains("\"cash\":\"100000\""));
    assert!(body.contains("\"margin_used\":\"0\""));
}

#[tokio::test]
async fn broker_account_uses_explicit_broker_without_default_run_config() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let response = api::router_with_state(api::AppState::with_server_config(
        db,
        config::ServerConfig::default(),
    ))
    .oneshot(
        Request::builder()
            .uri("/api/v1/brokers/account/paper?broker=simulated")
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("\"account_id\":\"paper\""));
    assert!(body.contains("\"cash\":\"100000\""));
}

#[tokio::test]
async fn paper_preflight_returns_configured_dry_run_summary() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let payload = serde_json::json!({
        "config_toml": std::fs::read_to_string("configs/backtest/slow-paper.toml").unwrap(),
    });
    let response = api::router_with_state(api::AppState::with_server_config(
        db,
        config::ServerConfig::default(),
    ))
    .oneshot(
        Request::builder()
            .method("POST")
            .uri("/api/v1/preflight/paper")
            .header("content-type", "application/json")
            .body(Body::from(payload.to_string()))
            .unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("\"status\":\"ok\""));
    assert!(body.contains("\"run_id\":\"sample-slow-paper\""));
    assert!(body.contains("\"broker\":\"simulated\""));
    assert!(body.contains("\"broker_mode\":\"paper\""));
    assert!(body.contains("\"bars\":3"));
    assert!(body.contains("\"order_submit_enabled\":false"));
}

#[tokio::test]
async fn paper_preflight_get_does_not_use_default_run_config() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let response = api::router_with_state(api::AppState::new(
        db,
        "configs/backtest/slow-paper.toml".into(),
    ))
    .oneshot(
        Request::builder()
            .uri("/api/v1/preflight/paper")
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("POST /api/v1/preflight/paper"));
}

#[tokio::test]
async fn paper_preflight_uses_request_config_without_default_run_config() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let bars_path = temp_config_path("request-driven-paper-preflight-bars").with_extension("csv");
    std::fs::write(
        &bars_path,
        "ts_ms,open,high,low,close,volume\n1,10,10,10,10,1\n2,11,11,11,11,1\n",
    )
    .unwrap();

    let app = api::router_with_state(api::AppState::with_server_config(
        db,
        config::ServerConfig::default(),
    ));
    let payload = serde_json::json!({
        "config_toml": replay_launch_config_toml_with_mode(
            "request-paper-preflight-no-default",
            "paper",
            &bars_path,
        ),
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/preflight/paper")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("\"status\":\"ok\""));
    assert!(body.contains("\"run_id\":\"request-paper-preflight-no-default\""));
    assert!(body.contains("\"broker\":\"simulated\""));
    assert!(body.contains("\"bars\":2"));
}

#[tokio::test]
async fn run_order_events_route_returns_audit_projection() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    db.start_strategy_run(StrategyRunStartCommand {
        run_id: "run-a".to_string(),
        name: "sample".to_string(),
        mode: "paper".to_string(),
        started_at_ms: 1,
        config: serde_json::json!({}),
    })
    .await
    .unwrap();
    db.record_runtime_event(RuntimeEventCommand {
        source: "run-a".to_string(),
        ts_ms: 1,
        category: "broker.order.submitted".to_string(),
        payload: serde_json::json!({
            "run_id": "run-a",
            "status": "SUBMITTED"
        }),
    })
    .await
    .unwrap();

    let response = api::router_with_state(api::AppState::new(
        db,
        "configs/backtest/ma_cross.toml".into(),
    ))
    .oneshot(
        Request::builder()
            .uri("/api/v1/runs/run-a/order-events")
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("\"run_id\":\"run-a\""));
    assert!(body.contains("\"event_type\":\"broker.order.submitted\""));
    assert!(body.contains("\"payload\":{\"run_id\":\"run-a\",\"status\":\"SUBMITTED\"}"));
}

#[tokio::test]
async fn run_order_events_route_filters_structured_audit_projection() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    db.start_strategy_run(StrategyRunStartCommand {
        run_id: "run-a".to_string(),
        name: "sample".to_string(),
        mode: "paper".to_string(),
        started_at_ms: 1,
        config: serde_json::json!({}),
    })
    .await
    .unwrap();
    db.record_runtime_event(RuntimeEventCommand {
        source: "run-a".to_string(),
        ts_ms: 10,
        category: "broker.order.submitted".to_string(),
        payload: serde_json::json!({
            "run_id": "run-a",
            "order_id": "order-a",
            "client_order_id": "client-a",
            "broker_order_id": "broker-a",
            "account_id": "paper",
            "symbol": "US:NASDAQ:AAPL:EQUITY",
            "status": "SUBMITTED"
        }),
    })
    .await
    .unwrap();
    db.record_runtime_event(RuntimeEventCommand {
        source: "run-a".to_string(),
        ts_ms: 20,
        category: "broker.order.recovered".to_string(),
        payload: serde_json::json!({
            "run_id": "run-a",
            "order_id": "order-a",
            "client_order_id": "client-a",
            "broker_order_id": "broker-a",
            "account_id": "paper",
            "symbol": "US:NASDAQ:AAPL:EQUITY",
            "status": "FILLED",
            "message": "startup recovery matched broker order state"
        }),
    })
    .await
    .unwrap();

    let response = api::router_with_state(api::AppState::new(
        db,
        "configs/backtest/ma_cross.toml".into(),
    ))
    .oneshot(
        Request::builder()
            .uri("/api/v1/runs/run-a/order-events?status=FILLED&event_type=broker.order.recovered&from_ms=15&to_ms=25&limit=1")
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let rows = body.as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["event_type"], "broker.order.recovered");
    assert_eq!(rows[0]["status"], "FILLED");
    assert_eq!(
        rows[0]["message"],
        "startup recovery matched broker order state"
    );
}

#[tokio::test]
async fn run_risk_events_route_returns_audit_projection() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    db.start_strategy_run(StrategyRunStartCommand {
        run_id: "run-a".to_string(),
        name: "sample".to_string(),
        mode: "paper".to_string(),
        started_at_ms: 1,
        config: serde_json::json!({}),
    })
    .await
    .unwrap();
    db.record_runtime_event(RuntimeEventCommand {
        source: "run-a".to_string(),
        ts_ms: 2,
        category: "algorithm.risk.rejected".to_string(),
        payload: serde_json::json!({
            "run_id": "run-a",
            "decision": "rejected",
            "reason": "max_exposure",
            "threshold": "1000",
            "observed_value": "1200"
        }),
    })
    .await
    .unwrap();

    let response = api::router_with_state(api::AppState::new(
        db,
        "configs/backtest/ma_cross.toml".into(),
    ))
    .oneshot(
        Request::builder()
            .uri("/api/v1/runs/run-a/risk-events")
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("\"run_id\":\"run-a\""));
    assert!(body.contains("\"risk_type\":\"pre_trade\""));
    assert!(body.contains("\"decision\":\"rejected\""));
    assert!(body.contains("\"reason\":\"max_exposure\""));
}

#[tokio::test]
async fn run_risk_events_route_filters_structured_audit_projection() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    db.start_strategy_run(StrategyRunStartCommand {
        run_id: "run-a".to_string(),
        name: "sample".to_string(),
        mode: "paper".to_string(),
        started_at_ms: 1,
        config: serde_json::json!({}),
    })
    .await
    .unwrap();
    db.record_runtime_event(RuntimeEventCommand {
        source: "run-a".to_string(),
        ts_ms: 10,
        category: "algorithm.risk.rejected".to_string(),
        payload: serde_json::json!({
            "run_id": "run-a",
            "account_id": "paper",
            "symbol": "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP",
            "risk_type": "reconciliation_drift",
            "decision": "warn",
            "reason": "position_qty_drift",
            "threshold": "1",
            "observed_value": "2"
        }),
    })
    .await
    .unwrap();
    db.record_runtime_event(RuntimeEventCommand {
        source: "run-a".to_string(),
        ts_ms: 20,
        category: "algorithm.risk.rejected".to_string(),
        payload: serde_json::json!({
            "run_id": "run-a",
            "account_id": "paper",
            "symbol": "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP",
            "risk_type": "max_exposure",
            "decision": "rejected",
            "reason": "too_large",
            "threshold": "100",
            "observed_value": "120"
        }),
    })
    .await
    .unwrap();

    let response = api::router_with_state(api::AppState::new(
        db,
        "configs/backtest/ma_cross.toml".into(),
    ))
    .oneshot(
        Request::builder()
            .uri("/api/v1/runs/run-a/risk-events?risk_type=reconciliation_drift&decision=warn&account_id=paper&from_ms=5&to_ms=15&limit=1")
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let rows = body.as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["risk_type"], "reconciliation_drift");
    assert_eq!(rows[0]["decision"], "warn");
    assert_eq!(rows[0]["reason"], "position_qty_drift");
}

#[tokio::test]
async fn run_insights_route_returns_strategy_decisions() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    db.start_strategy_run(StrategyRunStartCommand {
        run_id: "run-a".to_string(),
        name: "sample".to_string(),
        mode: "backtest".to_string(),
        started_at_ms: 1,
        config: serde_json::json!({}),
    })
    .await
    .unwrap();
    db.record_runtime_event(RuntimeEventCommand {
        source: "run-a".to_string(),
        ts_ms: 2,
        category: "algorithm.alpha.generated".to_string(),
        payload: serde_json::json!({
            "run_id": "run-a",
            "strategy": "moving_average_cross",
            "symbol": "US:NASDAQ:AAPL:EQUITY",
            "side": "BUY",
            "confidence": "0.75"
        }),
    })
    .await
    .unwrap();

    let response = api::router_with_state(api::AppState::new(
        db,
        "configs/backtest/ma_cross.toml".into(),
    ))
    .oneshot(
        Request::builder()
            .uri("/api/v1/runs/run-a/insights")
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("\"run_id\":\"run-a\""));
    assert!(body.contains("\"strategy\":\"moving_average_cross\""));
    assert!(body.contains("\"symbol\":\"US:NASDAQ:AAPL:EQUITY\""));
    assert!(body.contains("\"confidence\":\"0.75\""));
    assert!(body.contains("\"payload\":{"));
}

#[tokio::test]
async fn run_portfolio_targets_route_returns_target_positions() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    db.start_strategy_run(StrategyRunStartCommand {
        run_id: "run-a".to_string(),
        name: "sample".to_string(),
        mode: "backtest".to_string(),
        started_at_ms: 1,
        config: serde_json::json!({}),
    })
    .await
    .unwrap();
    db.record_runtime_event(RuntimeEventCommand {
        source: "run-a".to_string(),
        ts_ms: 3,
        category: "algorithm.portfolio.target".to_string(),
        payload: serde_json::json!({
            "run_id": "run-a",
            "account_id": "paper",
            "symbol": "US:NASDAQ:AAPL:EQUITY",
            "target_qty": "10"
        }),
    })
    .await
    .unwrap();

    let response = api::router_with_state(api::AppState::new(
        db,
        "configs/backtest/ma_cross.toml".into(),
    ))
    .oneshot(
        Request::builder()
            .uri("/api/v1/runs/run-a/portfolio-targets")
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("\"run_id\":\"run-a\""));
    assert!(body.contains("\"account_id\":\"paper\""));
    assert!(body.contains("\"symbol\":\"US:NASDAQ:AAPL:EQUITY\""));
    assert!(body.contains("\"target_qty\":\"10\""));
    assert!(body.contains("\"payload\":{"));
}

#[tokio::test]
async fn run_crypto_positions_route_returns_contract_position_projection() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    db.record_crypto_position(CryptoPositionCommand {
        run_id: "run-contract".to_string(),
        account_id: "paper".to_string(),
        exchange: "BINANCE".to_string(),
        symbol: "BTCUSDT_PERP".to_string(),
        asset_class: "CRYPTO_PERP".to_string(),
        margin_mode: "cross".to_string(),
        position_side: "short".to_string(),
        leverage: dec("3.5"),
        qty: dec("-0.250"),
        avg_price: dec("65001.0000"),
        margin_used: dec("1625.025"),
        funding_fee: dec("-1.50"),
        realized_pnl: dec("2.00"),
        unrealized_pnl: dec("20.0001"),
        updated_at_ms: 11,
    })
    .await
    .unwrap();
    db.record_crypto_position(CryptoPositionCommand {
        run_id: "other-run".to_string(),
        account_id: "paper".to_string(),
        exchange: "BINANCE".to_string(),
        symbol: "ETHUSDT_PERP".to_string(),
        asset_class: "CRYPTO_PERP".to_string(),
        margin_mode: "isolated".to_string(),
        position_side: "long".to_string(),
        leverage: dec("2"),
        qty: dec("1.000"),
        avg_price: dec("3500.0000"),
        margin_used: dec("1750.0000"),
        funding_fee: dec("0"),
        realized_pnl: dec("0"),
        unrealized_pnl: dec("0"),
        updated_at_ms: 12,
    })
    .await
    .unwrap();

    let response = api::router_with_state(api::AppState::new(
        db,
        "configs/backtest/ma_cross.toml".into(),
    ))
    .oneshot(
        Request::builder()
            .uri("/api/v1/runs/run-contract/crypto-positions")
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let positions: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let positions = positions.as_array().unwrap();
    assert_eq!(positions.len(), 1);
    let position = &positions[0];
    assert_eq!(position["run_id"], "run-contract");
    assert_eq!(position["account_id"], "paper");
    assert_eq!(position["exchange"], "BINANCE");
    assert_eq!(position["symbol"], "BTCUSDT_PERP");
    assert_eq!(position["asset_class"], "CRYPTO_PERP");
    assert_eq!(position["margin_mode"], "cross");
    assert_eq!(position["position_side"], "short");
    assert_eq!(position["leverage"], "3.5");
    assert_eq!(position["qty"], "-0.250");
    assert_eq!(position["avg_price"], "65001.0000");
    assert_eq!(position["margin_used"], "1625.025");
    assert_eq!(position["funding_fee"], "-1.50");
    assert_eq!(position["realized_pnl"], "2.00");
    assert_eq!(position["unrealized_pnl"], "20.0001");
    assert_eq!(position["updated_at_ms"], 11);
    assert!(position["qty"].is_string());
}

#[tokio::test]
async fn run_cash_snapshots_route_returns_filtered_snapshot_series() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    for run_id in ["run-snapshots", "other-run"] {
        db.start_strategy_run(StrategyRunStartCommand {
            run_id: run_id.to_string(),
            name: "sample".to_string(),
            mode: "paper".to_string(),
            started_at_ms: 1,
            config: serde_json::json!({}),
        })
        .await
        .unwrap();
    }
    for (run_id, currency, ts_ms, cash) in [
        ("run-snapshots", "USD", 10, dec("1000")),
        ("run-snapshots", "USDT", 20, dec("2000")),
        ("run-snapshots", "USD", 30, dec("1100")),
        ("other-run", "USD", 30, dec("9999")),
    ] {
        db.record_paper_portfolio_snapshot(PaperPortfolioSnapshotCommand {
            run_id: run_id.to_string(),
            account_id: "paper".to_string(),
            ts_ms,
            base_currency: currency.to_string(),
            cash,
            market_value: Decimal::ZERO,
            equity: cash,
            realized_pnl: Decimal::ZERO,
            unrealized_pnl: Decimal::ZERO,
            positions: Vec::new(),
        })
        .await
        .unwrap();
    }

    let response = api::router_with_state(api::AppState::new(
        db,
        "configs/backtest/ma_cross.toml".into(),
    ))
    .oneshot(
        Request::builder()
            .uri("/api/v1/runs/run-snapshots/cash-snapshots?currency=USD&from_ms=15&to_ms=35")
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let snapshots: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let snapshots = snapshots.as_array().unwrap();
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0]["run_id"], "run-snapshots");
    assert_eq!(snapshots[0]["currency"], "USD");
    assert_eq!(snapshots[0]["cash"], "1100");
    assert_eq!(snapshots[0]["ts_ms"], 30);
}

#[tokio::test]
async fn run_position_snapshots_route_returns_filtered_snapshot_series() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    for run_id in ["run-snapshots", "other-run"] {
        db.start_strategy_run(StrategyRunStartCommand {
            run_id: run_id.to_string(),
            name: "sample".to_string(),
            mode: "paper".to_string(),
            started_at_ms: 1,
            config: serde_json::json!({}),
        })
        .await
        .unwrap();
    }
    for (run_id, symbol, ts_ms, qty) in [
        (
            "run-snapshots",
            "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP",
            10,
            dec("0.25"),
        ),
        (
            "run-snapshots",
            "CRYPTO:BINANCE:ETHUSDT_PERP:CRYPTO_PERP",
            20,
            dec("-1.5"),
        ),
        (
            "run-snapshots",
            "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP",
            30,
            dec("0.50"),
        ),
        (
            "other-run",
            "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP",
            30,
            dec("9"),
        ),
    ] {
        db.record_paper_portfolio_snapshot(PaperPortfolioSnapshotCommand {
            run_id: run_id.to_string(),
            account_id: "paper".to_string(),
            ts_ms,
            base_currency: "USDT".to_string(),
            cash: dec("1000"),
            market_value: dec("32550"),
            equity: dec("33550"),
            realized_pnl: Decimal::ZERO,
            unrealized_pnl: dec("50"),
            positions: vec![PositionCommand {
                run_id: run_id.to_string(),
                account_id: "paper".to_string(),
                symbol: symbol.to_string(),
                qty,
                avg_price: dec("65000"),
                updated_at_ms: ts_ms,
            }],
        })
        .await
        .unwrap();
    }

    let response = api::router_with_state(api::AppState::new(
        db,
        "configs/backtest/ma_cross.toml".into(),
    ))
    .oneshot(
        Request::builder()
            .uri("/api/v1/runs/run-snapshots/position-snapshots?symbol=CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP&from_ms=15&to_ms=35")
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let snapshots: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let snapshots = snapshots.as_array().unwrap();
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0]["run_id"], "run-snapshots");
    assert_eq!(
        snapshots[0]["symbol"],
        "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP"
    );
    assert!(snapshots[0]["position_side"].is_null());
    assert_eq!(snapshots[0]["qty"], "0.50");
}

#[tokio::test]
async fn run_reconciliation_route_summarizes_snapshots_and_drift_events() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    db.start_strategy_run(StrategyRunStartCommand {
        run_id: "run-recon".to_string(),
        name: "sample".to_string(),
        mode: "paper".to_string(),
        started_at_ms: 1,
        config: serde_json::json!({}),
    })
    .await
    .unwrap();
    db.record_paper_portfolio_snapshot(PaperPortfolioSnapshotCommand {
        run_id: "run-recon".to_string(),
        account_id: "paper".to_string(),
        ts_ms: 25,
        base_currency: "USDT".to_string(),
        cash: dec("1000"),
        market_value: dec("650"),
        equity: dec("1650"),
        realized_pnl: Decimal::ZERO,
        unrealized_pnl: Decimal::ZERO,
        positions: vec![PositionCommand {
            run_id: "run-recon".to_string(),
            account_id: "paper".to_string(),
            symbol: "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP".to_string(),
            qty: dec("0.01"),
            avg_price: dec("65000"),
            updated_at_ms: 25,
        }],
    })
    .await
    .unwrap();
    db.record_runtime_event(RuntimeEventCommand {
        source: "run-recon".to_string(),
        ts_ms: 30,
        category: "algorithm.risk.rejected".to_string(),
        payload: serde_json::json!({
            "run_id": "run-recon",
            "account_id": "paper",
            "symbol": "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP",
            "risk_type": "reconciliation_drift",
            "decision": "rejected",
            "reason": "position_qty_drift",
            "threshold": "1",
            "observed_value": "5"
        }),
    })
    .await
    .unwrap();

    let response = api::router_with_state(api::AppState::new(
        db,
        "configs/backtest/ma_cross.toml".into(),
    ))
    .oneshot(
        Request::builder()
            .uri("/api/v1/runs/run-recon/reconciliation")
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["run_id"], "run-recon");
    assert_eq!(body["status"], "drift");
    assert_eq!(body["cash_snapshots"], 1);
    assert_eq!(body["position_snapshots"], 1);
    assert_eq!(body["latest_cash_ts_ms"], 25);
    assert_eq!(body["latest_position_ts_ms"], 25);
    assert_eq!(body["drift_events"].as_array().unwrap().len(), 1);
    assert_eq!(body["drift_events"][0]["risk_type"], "reconciliation_drift");
}

#[tokio::test]
async fn reconciliation_drifts_routes_filter_audit_rows() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    db.record_runtime_event(RuntimeEventCommand {
        source: "run-recon-a".to_string(),
        ts_ms: 30,
        category: "algorithm.risk.rejected".to_string(),
        payload: serde_json::json!({
            "run_id": "run-recon-a",
            "account_id": "paper",
            "symbol": "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP",
            "risk_type": "reconciliation_drift",
            "decision": "warn",
            "reason": "position_qty_drift",
            "threshold": "1",
            "observed_value": "5"
        }),
    })
    .await
    .unwrap();
    db.record_runtime_event(RuntimeEventCommand {
        source: "run-recon-b".to_string(),
        ts_ms: 40,
        category: "algorithm.risk.rejected".to_string(),
        payload: serde_json::json!({
            "run_id": "run-recon-b",
            "account_id": "paper",
            "symbol": "CRYPTO:BINANCE:ETHUSDT_PERP:CRYPTO_PERP",
            "risk_type": "reconciliation_drift",
            "decision": "warn",
            "reason": "cash_drift",
            "threshold": "2",
            "observed_value": "3"
        }),
    })
    .await
    .unwrap();
    db.record_runtime_event(RuntimeEventCommand {
        source: "run-recon-a".to_string(),
        ts_ms: 50,
        category: "algorithm.risk.rejected".to_string(),
        payload: serde_json::json!({
            "run_id": "run-recon-a",
            "account_id": "paper",
            "symbol": "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP",
            "risk_type": "max_exposure",
            "decision": "reject",
            "reason": "too_large"
        }),
    })
    .await
    .unwrap();

    let app = api::router_with_state(api::AppState::new(
        db,
        "configs/backtest/ma_cross.toml".into(),
    ));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/reconciliation-drifts?run_id=run-recon-a&limit=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let rows = body.as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["run_id"], "run-recon-a");
    assert_eq!(rows[0]["risk_type"], "reconciliation_drift");

    let run_response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/runs/run-recon-b/reconciliation-drifts?symbol=CRYPTO:BINANCE:ETHUSDT_PERP:CRYPTO_PERP")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(run_response.status(), StatusCode::OK);
    let bytes = to_bytes(run_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let rows = body.as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["run_id"], "run-recon-b");
    assert_eq!(rows[0]["reason"], "cash_drift");
}

#[tokio::test]
async fn config_lifecycle_routes_return_release_audit_and_run_binding_status() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    db.start_strategy_run(StrategyRunStartCommand {
        run_id: "run-config".to_string(),
        name: "sample".to_string(),
        mode: "paper".to_string(),
        started_at_ms: 1,
        config: serde_json::json!({}),
    })
    .await
    .unwrap();
    db.record_config(storage::ConfigRecordCommand {
        id: "config-paper".to_string(),
        name: "paper-binance".to_string(),
        config_type: "BROKER".to_string(),
        content: "order_submit_enabled = true".to_string(),
        format: "TOML".to_string(),
        checksum: Some("sha256:v1".to_string()),
        ts_ms: 2,
    })
    .await
    .unwrap();
    db.record_config_release(storage::ConfigReleaseCommand {
        config_id: "config-paper".to_string(),
        version: "v1".to_string(),
        status: "released".to_string(),
        released_by: Some("ops".to_string()),
        notes: Some("paper broker rollout".to_string()),
        ts_ms: 3,
    })
    .await
    .unwrap();
    db.bind_run_config_version(storage::RunConfigVersionBindingCommand {
        run_id: "run-config".to_string(),
        config_id: "config-paper".to_string(),
        version: "v1".to_string(),
        ts_ms: 4,
    })
    .await
    .unwrap();
    db.record_config_audit(storage::ConfigAuditCommand {
        config_id: "config-paper".to_string(),
        version: Some("v1".to_string()),
        action: "rollback".to_string(),
        actor: Some("ops".to_string()),
        reason: Some("restore previous release".to_string()),
        ts_ms: 5,
    })
    .await
    .unwrap();
    let app = api::router_with_state(api::AppState::new(
        db,
        "configs/backtest/ma_cross.toml".into(),
    ));

    let releases = get_json(app.clone(), "/api/v1/configs/config-paper/releases").await;
    assert_eq!(releases.as_array().unwrap().len(), 1);
    assert_eq!(releases[0]["version"], "v1");
    assert_eq!(releases[0]["status"], "released");
    assert_eq!(releases[0]["released_by"], "ops");

    let audits = get_json(app.clone(), "/api/v1/configs/config-paper/audits").await;
    assert_eq!(audits.as_array().unwrap().len(), 1);
    assert_eq!(audits[0]["action"], "rollback");
    assert_eq!(audits[0]["reason"], "restore previous release");

    let binding = get_json(app, "/api/v1/runs/run-config/config-version").await;
    assert_eq!(binding["run_id"], "run-config");
    assert_eq!(binding["config_id"], "config-paper");
    assert_eq!(binding["version"], "v1");
}

#[tokio::test]
async fn config_crud_routes_create_transition_diff_and_rollback() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = api::router_with_state(api::AppState::new(
        db,
        "configs/backtest/ma_cross.toml".into(),
    ));

    let (status, v1) = request_json(
        app.clone(),
        "POST",
        "/api/v1/configs",
        serde_json::json!({
            "name": "paper-risk",
            "content": {
                "risk": { "max_order_notional": "1000" },
                "enabled": true
            },
            "created_by": "ops",
            "ts_ms": 100
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(v1["name"], "paper-risk");
    assert_eq!(v1["version"], 1);
    assert_eq!(v1["state"], "draft");

    let (status, v2) = request_json(
        app.clone(),
        "POST",
        "/api/v1/configs",
        serde_json::json!({
            "name": "paper-risk",
            "content": {
                "risk": { "max_order_notional": "2000" },
                "symbols": ["AAPL"]
            },
            "created_by": "ops",
            "parent_version": 1,
            "ts_ms": 200
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(v2["version"], 2);

    let versions = get_json(app.clone(), "/api/v1/configs/paper-risk").await;
    assert_eq!(versions.as_array().unwrap().len(), 2);
    assert_eq!(versions[0]["version"], 1);
    assert_eq!(versions[1]["parent_version"], 1);

    let latest = get_json(app.clone(), "/api/v1/configs/paper-risk/latest").await;
    assert_eq!(latest["version"], 2);
    assert_eq!(latest["state"], "draft");

    let specific = get_json(app.clone(), "/api/v1/configs/paper-risk/1").await;
    assert_eq!(specific["content"]["risk"]["max_order_notional"], "1000");

    for state in ["pending_review", "approved", "published"] {
        let (status, body) = request_json(
            app.clone(),
            "PUT",
            "/api/v1/configs/paper-risk/1/state",
            serde_json::json!({
                "new_state": state,
                "changed_by": "ops",
                "reason": format!("set {state}"),
                "ts_ms": 300
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["state"], state);
    }

    let published = get_json(app.clone(), "/api/v1/configs/paper-risk/published").await;
    assert_eq!(published["version"], 1);
    assert_eq!(published["state"], "published");

    let diff = get_json(app.clone(), "/api/v1/configs/paper-risk/diff?v1=1&v2=2").await;
    assert_eq!(diff["added"], serde_json::json!(["symbols"]));
    assert_eq!(diff["removed"], serde_json::json!(["enabled"]));
    assert_eq!(diff["changed"][0]["path"], "risk.max_order_notional");

    let (status, rollback) = request_json(
        app.clone(),
        "POST",
        "/api/v1/configs/paper-risk/1/rollback",
        serde_json::json!({
            "actor": "ops",
            "reason": "restore v1",
            "ts_ms": 400
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(rollback["version"], 3);
    assert_eq!(rollback["state"], "draft");
    assert_eq!(rollback["parent_version"], 1);
}

#[tokio::test]
async fn config_governance_routes_enforce_independent_production_approval() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = api::router_with_state(api::AppState::new(
        db,
        "configs/backtest/ma_cross.toml".into(),
    ));

    let (status, created) = request_json(
        app.clone(),
        "POST",
        "/api/v1/configs",
        serde_json::json!({
            "name": "prod-risk",
            "content": { "risk": { "max_order_notional": "1000" } },
            "created_by": "release",
            "target_env": "production",
            "rollout": "canary",
            "ts_ms": 100
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["target_env"], "production");
    assert_eq!(created["rollout"], "canary");

    for (state, actor, ts_ms) in [
        ("pending_review", "release", 200),
        ("approved", "release", 300),
    ] {
        let (status, body) = request_json(
            app.clone(),
            "PUT",
            "/api/v1/configs/prod-risk/1/state",
            serde_json::json!({
                "new_state": state,
                "changed_by": actor,
                "reason": state,
                "ts_ms": ts_ms
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["state"], state);
    }

    let (status, _) = request_json(
        app.clone(),
        "PUT",
        "/api/v1/configs/prod-risk/1/state",
        serde_json::json!({
            "new_state": "published",
            "changed_by": "release",
            "reason": "publish",
            "ts_ms": 400
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, approved) = request_json(
        app.clone(),
        "PUT",
        "/api/v1/configs/prod-risk/1/state",
        serde_json::json!({
            "new_state": "approved",
            "changed_by": "risk-owner",
            "reason": "independent approval",
            "ts_ms": 500
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(approved["approved_by"], "risk-owner");

    let (status, published) = request_json(
        app.clone(),
        "PUT",
        "/api/v1/configs/prod-risk/1/state",
        serde_json::json!({
            "new_state": "published",
            "changed_by": "release",
            "reason": "publish",
            "ts_ms": 600
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(published["state"], "published");
    assert_eq!(published["approved_by"], "risk-owner");
    assert_eq!(published["published_by"], "release");
}

#[tokio::test]
async fn config_governance_routes_enforce_roles_and_list_pending_approvals() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = api::router_with_state(api::AppState::new(
        db,
        "configs/backtest/ma_cross.toml".into(),
    ));

    let (status, _) = request_json(
        app.clone(),
        "POST",
        "/api/v1/configs",
        serde_json::json!({
            "name": "prod-queue",
            "content": { "risk": { "max_order_notional": "1000" } },
            "created_by": "release",
            "target_env": "production",
            "rollout": "canary",
            "ts_ms": 100
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, _) = request_json(
        app.clone(),
        "PUT",
        "/api/v1/configs/prod-queue/1/state",
        serde_json::json!({
            "new_state": "pending_review",
            "changed_by": "trader",
            "actor_role": "viewer",
            "reason": "request approval",
            "ts_ms": 200
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, pending) = request_json(
        app.clone(),
        "PUT",
        "/api/v1/configs/prod-queue/1/state",
        serde_json::json!({
            "new_state": "pending_review",
            "changed_by": "release",
            "actor_role": "release_manager",
            "reason": "request approval",
            "ts_ms": 300
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(pending["state"], "pending_review");

    let queue = get_json(
        app.clone(),
        "/api/v1/config-approvals/pending?target_env=production",
    )
    .await;
    assert_eq!(queue.as_array().unwrap().len(), 1);
    assert_eq!(queue[0]["name"], "prod-queue");
    assert_eq!(queue[0]["target_env"], "production");

    let (status, _) = request_json(
        app.clone(),
        "PUT",
        "/api/v1/configs/prod-queue/1/state",
        serde_json::json!({
            "new_state": "approved",
            "changed_by": "release",
            "actor_role": "release_manager",
            "reason": "approve",
            "ts_ms": 400
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, approved) = request_json(
        app.clone(),
        "PUT",
        "/api/v1/configs/prod-queue/1/state",
        serde_json::json!({
            "new_state": "approved",
            "changed_by": "risk-owner",
            "actor_role": "approver",
            "reason": "risk approval",
            "ts_ms": 500
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(approved["approved_by"], "risk-owner");

    let (status, _) = request_json(
        app.clone(),
        "PUT",
        "/api/v1/configs/prod-queue/1/state",
        serde_json::json!({
            "new_state": "published",
            "changed_by": "trader",
            "actor_role": "approver",
            "reason": "publish",
            "ts_ms": 600
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, published) = request_json(
        app.clone(),
        "PUT",
        "/api/v1/configs/prod-queue/1/state",
        serde_json::json!({
            "new_state": "published",
            "changed_by": "release",
            "actor_role": "release_manager",
            "reason": "publish",
            "ts_ms": 700
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(published["state"], "published");
    assert_eq!(published["published_by"], "release");
}

#[tokio::test]
async fn config_governance_routes_enforce_staging_roles_and_list_pending_approvals() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = api::router_with_state(api::AppState::new(
        db,
        "configs/backtest/ma_cross.toml".into(),
    ));

    let (status, _) = request_json(
        app.clone(),
        "POST",
        "/api/v1/configs",
        serde_json::json!({
            "name": "staging-queue",
            "content": { "risk": { "max_order_notional": "1000" } },
            "created_by": "release",
            "target_env": "staging",
            "rollout": "canary",
            "ts_ms": 100
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, _) = request_json(
        app.clone(),
        "PUT",
        "/api/v1/configs/staging-queue/1/state",
        serde_json::json!({
            "new_state": "pending_review",
            "changed_by": "trader",
            "actor_role": "viewer",
            "reason": "request approval",
            "ts_ms": 200
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, pending) = request_json(
        app.clone(),
        "PUT",
        "/api/v1/configs/staging-queue/1/state",
        serde_json::json!({
            "new_state": "pending_review",
            "changed_by": "release",
            "actor_role": "release_manager",
            "reason": "request approval",
            "ts_ms": 300
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(pending["state"], "pending_review");

    let queue = get_json(
        app.clone(),
        "/api/v1/config-approvals/pending?target_env=staging",
    )
    .await;
    assert_eq!(queue.as_array().unwrap().len(), 1);
    assert_eq!(queue[0]["name"], "staging-queue");
    assert_eq!(queue[0]["target_env"], "staging");

    let (status, _) = request_json(
        app.clone(),
        "PUT",
        "/api/v1/configs/staging-queue/1/state",
        serde_json::json!({
            "new_state": "approved",
            "changed_by": "release",
            "actor_role": "release_manager",
            "reason": "approve",
            "ts_ms": 400
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, approved) = request_json(
        app.clone(),
        "PUT",
        "/api/v1/configs/staging-queue/1/state",
        serde_json::json!({
            "new_state": "approved",
            "changed_by": "qa-owner",
            "actor_role": "approver",
            "reason": "qa approval",
            "ts_ms": 500
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(approved["approved_by"], "qa-owner");

    let (status, _) = request_json(
        app.clone(),
        "PUT",
        "/api/v1/configs/staging-queue/1/state",
        serde_json::json!({
            "new_state": "published",
            "changed_by": "qa-owner",
            "actor_role": "approver",
            "reason": "publish",
            "ts_ms": 600
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, published) = request_json(
        app.clone(),
        "PUT",
        "/api/v1/configs/staging-queue/1/state",
        serde_json::json!({
            "new_state": "published",
            "changed_by": "release",
            "actor_role": "release_manager",
            "reason": "publish",
            "ts_ms": 700
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(published["state"], "published");
    assert_eq!(published["published_by"], "release");
}

#[tokio::test]
async fn backtest_start_binds_run_to_config_snapshot_version() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = api::router_with_state(api::AppState::new(
        db,
        "configs/backtest/ma_cross.toml".into(),
    ));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/backtests")
                .header("content-type", "application/json")
                .body(launch_config_path_body(
                    "configs/backtest/ma_cross.toml",
                    "backtest",
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let binding = get_json(app, "/api/v1/runs/sample-ma-cross/config-version").await;
    assert_eq!(binding["run_id"], "sample-ma-cross");
    assert_eq!(binding["config_id"], "run:sample-ma-cross");
    assert!(binding["version"].as_str().unwrap().starts_with("fnv1a64:"));
}

#[tokio::test]
async fn funding_rates_route_returns_filtered_decimal_series() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    db.record_funding_rate(FundingRateCommand {
        id: "funding-1".to_string(),
        exchange: "BINANCE".to_string(),
        symbol: "BTCUSDT_PERP".to_string(),
        funding_time_ms: 1000,
        funding_rate: dec("0.0002"),
        mark_price: Some(dec("65001.0000")),
        source: "testnet".to_string(),
    })
    .await
    .unwrap();
    db.record_funding_rate(FundingRateCommand {
        id: "funding-outside-window".to_string(),
        exchange: "BINANCE".to_string(),
        symbol: "BTCUSDT_PERP".to_string(),
        funding_time_ms: 2500,
        funding_rate: dec("0.0003"),
        mark_price: Some(dec("65002.0000")),
        source: "testnet".to_string(),
    })
    .await
    .unwrap();

    let response = api::router_with_state(api::AppState::new(
        db,
        "configs/backtest/ma_cross.toml".into(),
    ))
    .oneshot(
        Request::builder()
            .uri(
                "/api/v1/funding-rates?exchange=BINANCE&symbol=BTCUSDT_PERP&start_ms=0&end_ms=2000",
            )
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let rates: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let rates = rates.as_array().unwrap();
    assert_eq!(rates.len(), 1);
    let rate = &rates[0];
    assert_eq!(rate["id"], "funding-1");
    assert_eq!(rate["exchange"], "BINANCE");
    assert_eq!(rate["symbol"], "BTCUSDT_PERP");
    assert_eq!(rate["funding_time_ms"], 1000);
    assert_eq!(rate["funding_rate"], "0.0002");
    assert_eq!(rate["mark_price"], "65001.0000");
    assert_eq!(rate["source"], "testnet");
    assert!(rate["funding_rate"].is_string());
}

#[tokio::test]
async fn crypto_market_meta_route_returns_filtered_decimal_metadata() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    db.record_crypto_market_meta(CryptoMarketMetaCommand {
        exchange: "BINANCE".to_string(),
        symbol: "BTCUSDT_PERP".to_string(),
        base_asset: "BTC".to_string(),
        quote_asset: "USDT".to_string(),
        instrument_type: "PERP".to_string(),
        contract_type: Some("LINEAR".to_string()),
        contract_size: Some(dec("1")),
        settlement_asset: Some("USDT".to_string()),
        min_notional: Some(dec("10")),
        min_qty: Some(dec("0.001")),
        max_qty: Some(dec("100")),
        price_precision: Some(2),
        qty_precision: Some(3),
        price_tick: Some(dec("0.10")),
        qty_step: Some(dec("0.001")),
        maker_fee_rate: Some(dec("0.0002")),
        taker_fee_rate: Some(dec("0.0004")),
        funding_interval_hours: Some(8),
        max_leverage: Some(dec("50")),
        margin_modes: Some(vec!["CROSS".to_string(), "ISOLATED".to_string()]),
        is_inverse: false,
        is_active: true,
        created_at_ms: 10,
        updated_at_ms: 11,
    })
    .await
    .unwrap();

    let response = api::router_with_state(api::AppState::new(
        db,
        "configs/backtest/ma_cross.toml".into(),
    ))
    .oneshot(
        Request::builder()
            .uri("/api/v1/crypto-market-meta?exchange=BINANCE&symbol=BTCUSDT_PERP")
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let metas: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let metas = metas.as_array().unwrap();
    assert_eq!(metas.len(), 1);
    let meta = &metas[0];
    assert_eq!(meta["exchange"], "BINANCE");
    assert_eq!(meta["symbol"], "BTCUSDT_PERP");
    assert_eq!(meta["instrument_type"], "PERP");
    assert_eq!(meta["contract_type"], "LINEAR");
    assert_eq!(meta["contract_size"], "1");
    assert_eq!(meta["min_notional"], "10");
    assert_eq!(meta["price_tick"], "0.10");
    assert_eq!(meta["qty_step"], "0.001");
    assert_eq!(meta["max_leverage"], "50");
    assert_eq!(meta["margin_modes"][0], "CROSS");
    assert_eq!(meta["is_inverse"], false);
    assert_eq!(meta["is_active"], true);
    assert_eq!(meta["updated_at_ms"], 11);
    assert!(meta["min_qty"].is_string());
}

#[tokio::test]
async fn corporate_actions_route_returns_filtered_actions() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    db.record_corporate_action_meta(CorporateActionMetaCommand {
        market: "US".to_string(),
        exchange: "NASDAQ".to_string(),
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        action_type: "SPLIT".to_string(),
        ex_date_ms: 1000,
        record_date_ms: Some(1100),
        payable_date_ms: Some(1200),
        ratio: Some("4:1".to_string()),
        cash_amount: None,
        currency: None,
        source: Some("fixture".to_string()),
        created_at_ms: 1300,
        updated_at_ms: 1300,
    })
    .await
    .unwrap();
    db.record_corporate_action_meta(CorporateActionMetaCommand {
        market: "US".to_string(),
        exchange: "NYSE".to_string(),
        symbol: "US:NYSE:IBM:EQUITY".to_string(),
        action_type: "DIVIDEND".to_string(),
        ex_date_ms: 1000,
        record_date_ms: None,
        payable_date_ms: None,
        ratio: None,
        cash_amount: Some(dec("1.25")),
        currency: Some("USD".to_string()),
        source: Some("fixture".to_string()),
        created_at_ms: 1300,
        updated_at_ms: 1300,
    })
    .await
    .unwrap();

    let response = api::router_with_state(api::AppState::new(
        db,
        "configs/backtest/ma_cross.toml".into(),
    ))
    .oneshot(
        Request::builder()
            .uri(
                "/api/v1/corporate-actions?market=US&symbol=US:NASDAQ:AAPL:EQUITY&start_ms=0&end_ms=2000",
            )
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let actions: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let actions = actions.as_array().unwrap();
    assert_eq!(actions.len(), 1);
    let action = &actions[0];
    assert_eq!(action["market"], "US");
    assert_eq!(action["exchange"], "NASDAQ");
    assert_eq!(action["symbol"], "US:NASDAQ:AAPL:EQUITY");
    assert_eq!(action["action_type"], "SPLIT");
    assert_eq!(action["ex_date_ms"], 1000);
    assert_eq!(action["record_date_ms"], 1100);
    assert_eq!(action["payable_date_ms"], 1200);
    assert_eq!(action["ratio"], "4:1");
    assert!(action["cash_amount"].is_null());
    assert_eq!(action["source"], "fixture");
}

#[tokio::test]
async fn ingestion_status_route_returns_tracker_status() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    db.record_system_log(SystemLogCommand {
        run_id: None,
        ts_ms: 1234,
        level: "INFO".to_string(),
        target: "ingestion".to_string(),
        message: "ingested 2 rows into funding_rates from binance".to_string(),
        fields: Some(serde_json::json!({
            "source": "binance",
            "table": "funding_rates",
            "rows_fetched": 3,
            "rows_upserted": 2,
            "duration_ms": 25
        })),
    })
    .await
    .unwrap();

    let response = api::router_with_state(api::AppState::new(
        db,
        "configs/backtest/ma_cross.toml".into(),
    ))
    .oneshot(
        Request::builder()
            .uri("/api/v1/ingestion/status")
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let sources = body["sources"].as_array().unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0]["name"], "binance");
    assert_eq!(sources[0]["table"], "funding_rates");
    assert_eq!(sources[0]["rows_fetched"], 3);
    assert_eq!(sources[0]["rows_upserted"], 2);
    assert_eq!(sources[0]["duration_ms"], 25);
}

#[tokio::test]
async fn system_logs_route_filters_by_run_level_target_and_time() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    for (run_id, ts_ms, level, target, message) in [
        (
            Some("run-logs"),
            100,
            "INFO",
            "runtime.execution",
            "started",
        ),
        (
            Some("run-logs"),
            200,
            "ERROR",
            "runtime.execution",
            "failed",
        ),
        (None, 200, "ERROR", "system.scheduler", "scheduler failed"),
        (
            Some("other-run"),
            200,
            "ERROR",
            "runtime.execution",
            "other failed",
        ),
    ] {
        db.record_system_log(SystemLogCommand {
            run_id: run_id.map(str::to_string),
            ts_ms,
            level: level.to_string(),
            target: target.to_string(),
            message: message.to_string(),
            fields: Some(serde_json::json!({
                "category": target.split('.').next().unwrap_or(target)
            })),
        })
        .await
        .unwrap();
    }
    let app = api::router_with_state(api::AppState::new(
        db,
        "configs/backtest/ma_cross.toml".into(),
    ));

    let logs = get_json(
        app.clone(),
        "/api/v1/system-logs?run_id=run-logs&level=ERROR&target=runtime.execution&from_ms=150&to_ms=250",
    )
    .await;
    let logs = logs.as_array().unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0]["message"], "failed");
    assert_eq!(logs[0]["fields"]["category"], "runtime");

    let run_logs = get_json(
        app,
        "/api/v1/runs/run-logs/system-logs?level=ERROR&target=runtime.execution",
    )
    .await;
    let run_logs = run_logs.as_array().unwrap();
    assert_eq!(run_logs.len(), 1);
    assert_eq!(run_logs[0]["run_id"], "run-logs");
}

#[tokio::test]
async fn logs_route_returns_paginated_search_results_with_total() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    for (run_id, ts_ms, level, target, message) in [
        (
            Some("run-logs"),
            100,
            "INFO",
            "runtime.execution",
            "execution started",
        ),
        (
            Some("run-logs"),
            200,
            "ERROR",
            "runtime.execution",
            "execution failed",
        ),
        (
            Some("run-logs"),
            300,
            "ERROR",
            "runtime.execution",
            "execution failed again",
        ),
    ] {
        db.record_system_log(SystemLogCommand {
            run_id: run_id.map(str::to_string),
            ts_ms,
            level: level.to_string(),
            target: target.to_string(),
            message: message.to_string(),
            fields: Some(serde_json::json!({
                "category": "runtime"
            })),
        })
        .await
        .unwrap();
    }
    let app = api::router_with_state(api::AppState::new(
        db,
        "configs/backtest/ma_cross.toml".into(),
    ));

    let body = get_json(
        app,
        "/api/v1/logs?run_id=run-logs&search=failed&limit=1&offset=1",
    )
    .await;
    assert_eq!(body["total"], 2);
    assert_eq!(body["limit"], 1);
    assert_eq!(body["offset"], 1);
    let logs = body["logs"].as_array().unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0]["message"], "execution failed again");
    assert_eq!(logs[0]["fields"]["category"], "runtime");
}

#[tokio::test]
async fn reconciliation_alert_summary_routes_aggregate_runtime_alert_logs() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    for (run_id, ts_ms, message, fields) in [
        (
            Some("run-alert-a"),
            100,
            "reconciliation_drift.alert",
            serde_json::json!({
                "account_id": "paper",
                "symbol": "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP",
                "reason": "position_qty_drift"
            }),
        ),
        (
            Some("run-alert-b"),
            200,
            "reconciliation_drift.alert",
            serde_json::json!({
                "account_id": "paper",
                "symbol": "CRYPTO:BINANCE:ETHUSDT_PERP:CRYPTO_PERP",
                "reason": "cash_total_drift"
            }),
        ),
        (
            Some("run-alert-b"),
            300,
            "other.alert",
            serde_json::json!({
                "account_id": "paper",
                "reason": "ignored"
            }),
        ),
    ] {
        db.record_system_log(SystemLogCommand {
            run_id: run_id.map(str::to_string),
            ts_ms,
            level: "ERROR".to_string(),
            target: "runtime.alert".to_string(),
            message: message.to_string(),
            fields: Some(fields),
        })
        .await
        .unwrap();
    }
    let app = api::router_with_state(api::AppState::new(
        db,
        "configs/backtest/ma_cross.toml".into(),
    ));

    let summary = get_json(
        app.clone(),
        "/api/v1/reconciliation-alerts/summary?account_id=paper",
    )
    .await;
    assert_eq!(summary["alert_count"], 2);
    assert_eq!(summary["latest_alert_ts_ms"], 200);
    assert_eq!(summary["runs"].as_array().unwrap().len(), 2);
    assert_eq!(summary["reasons"].as_array().unwrap().len(), 2);

    let run_summary = get_json(
        app,
        "/api/v1/runs/run-alert-b/reconciliation-alerts/summary?symbol=CRYPTO:BINANCE:ETHUSDT_PERP:CRYPTO_PERP",
    )
    .await;
    assert_eq!(run_summary["run_id"], "run-alert-b");
    assert_eq!(run_summary["alert_count"], 1);
    assert_eq!(run_summary["latest_alert_ts_ms"], 200);
    assert_eq!(run_summary["runs"][0], "run-alert-b");
    assert_eq!(run_summary["reasons"][0], "cash_total_drift");
}

#[tokio::test]
async fn reconciliation_alert_delivery_summary_routes_aggregate_delivery_logs() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    for (run_id, ts_ms, fields) in [
        (
            Some("run-delivery-a"),
            100,
            serde_json::json!({
                "account_id": "paper",
                "symbol": "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP",
                "sink": "webhook",
                "status": "failed",
                "http_status": 500
            }),
        ),
        (
            Some("run-delivery-b"),
            200,
            serde_json::json!({
                "account_id": "paper",
                "symbol": "CRYPTO:BINANCE:ETHUSDT_PERP:CRYPTO_PERP",
                "sink": "file",
                "status": "sent"
            }),
        ),
        (
            Some("run-delivery-b"),
            300,
            serde_json::json!({
                "account_id": "other",
                "sink": "webhook",
                "status": "sent"
            }),
        ),
    ] {
        db.record_system_log(SystemLogCommand {
            run_id: run_id.map(str::to_string),
            ts_ms,
            level: "INFO".to_string(),
            target: "runtime.alert_delivery".to_string(),
            message: "alert.delivery".to_string(),
            fields: Some(fields),
        })
        .await
        .unwrap();
    }
    let app = api::router_with_state(api::AppState::new(
        db,
        "configs/backtest/ma_cross.toml".into(),
    ));

    let summary = get_json(
        app.clone(),
        "/api/v1/reconciliation-alert-deliveries/summary?account_id=paper",
    )
    .await;
    assert_eq!(summary["delivery_count"], 2);
    assert_eq!(summary["latest_delivery_ts_ms"], 200);
    assert_eq!(summary["sent_count"], 1);
    assert_eq!(summary["failed_count"], 1);
    assert_eq!(summary["sinks"].as_array().unwrap().len(), 2);

    let run_summary = get_json(
        app,
        "/api/v1/runs/run-delivery-b/reconciliation-alert-deliveries/summary?symbol=CRYPTO:BINANCE:ETHUSDT_PERP:CRYPTO_PERP",
    )
    .await;
    assert_eq!(run_summary["run_id"], "run-delivery-b");
    assert_eq!(run_summary["delivery_count"], 1);
    assert_eq!(run_summary["sent_count"], 1);
    assert_eq!(run_summary["failed_count"], 0);
    assert_eq!(run_summary["sinks"][0], "file");
    assert_eq!(run_summary["statuses"][0], "sent");
}

#[tokio::test]
async fn live_runtime_routes_start_report_status_and_stop() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = api::router_with_state(api::AppState::new(
        db,
        "configs/backtest/ma_cross.toml".into(),
    ));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/live-runs")
                .header("content-type", "application/json")
                .body(launch_config_path_body(
                    "configs/backtest/ma_cross.toml",
                    "live",
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    wait_for_body_fragment(
        app.clone(),
        "/api/v1/live-runs/sample-ma-cross/status",
        "running",
    )
    .await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/live-runs/sample-ma-cross/stop")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    wait_for_body_fragment(app, "/api/v1/live-runs/sample-ma-cross/status", "stopped").await;
}

#[tokio::test]
async fn live_runtime_route_uses_configured_broker_snapshot_interval() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let config_path =
        std::env::temp_dir().join(format!("trader-live-snapshot-{}.toml", std::process::id()));
    std::fs::write(
        &config_path,
        r#"
        [runtime]
        mode = "live"
        run_id = "api-live-snapshot"

        [database]
        url = "sqlite::memory:"

        [data]
        source = "csv"
        path = "datasets/sample/aapl_1d.csv"

        [strategy]
        name = "moving_average_cross"
        symbols = ["US:NASDAQ:AAPL:EQUITY"]
        fast_window = 2
        slow_window = 3

        [portfolio]
        initial_cash = "25000"
        base_currency = "USD"
        order_qty = "1"
        max_abs_qty = "100"

        [risk]
        max_order_notional = "1000000"
        min_cash_after_order = "0"
        max_exposure = "1000000"
        max_drawdown = "1"
        max_leverage = "10"
        max_margin_used = "0"
        trading_halted = false

        [broker]
        kind = "simulated"
        mode = "paper"

        [paper]
        account_id = "paper"
        slippage_bps = "25"
        fee_bps = "10"

        [live]
        enabled = true
        broker_snapshot_interval_ms = 5
        "#,
    )
    .unwrap();
    let app = api::router_with_state(api::AppState::new(db, config_path.display().to_string()));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/live-runs")
                .header("content-type", "application/json")
                .body(launch_config_path_body(&config_path, "live"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    wait_for_body_fragment(
        app.clone(),
        "/api/v1/runs/api-live-snapshot/cash-snapshots?currency=USD",
        "\"cash\":\"100000\"",
    )
    .await;
    wait_for_body_fragment(
        app.clone(),
        "/api/v1/runs/api-live-snapshot/position-snapshots?symbol=CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP&position_side=long",
        "\"unrealized_pnl\":\"12.5\"",
    )
    .await;
    wait_for_body_fragment(
        app.clone(),
        "/api/v1/runs/api-live-snapshot/reconciliation",
        "\"status\":\"drift\"",
    )
    .await;
    wait_for_body_fragment(
        app.clone(),
        "/api/v1/runs/api-live-snapshot/reconciliation",
        "position_missing_runtime",
    )
    .await;
    wait_for_body_fragment(
        app.clone(),
        "/api/v1/runs/api-live-snapshot/config-version",
        "\"run_id\":\"api-live-snapshot\"",
    )
    .await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/live-runs/api-live-snapshot/stop")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

fn dec(value: &str) -> Decimal {
    value.parse().unwrap()
}

#[tokio::test]
async fn explicit_run_scoped_routes_only_return_requested_run_data() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    for run_id in ["run-a", "run-b"] {
        db.start_strategy_run(StrategyRunStartCommand {
            run_id: run_id.to_string(),
            name: format!("strategy-{run_id}"),
            mode: "paper".to_string(),
            started_at_ms: 1,
            config: serde_json::json!({ "run_id": run_id }),
        })
        .await
        .unwrap();
    }
    db.insert_order(NewOrder {
        id: "order-a".to_string(),
        run_id: "run-a".to_string(),
        client_order_id: "client-a".to_string(),
        broker_order_id: Some("broker-a".to_string()),
        account_id: "paper".to_string(),
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        side: "BUY".to_string(),
        order_type: "MARKET".to_string(),
        price: Some("100".to_string()),
        qty: "1".to_string(),
        filled_qty: "1".to_string(),
        status: "FILLED".to_string(),
        created_at_ms: 10,
        updated_at_ms: 10,
    })
    .await
    .unwrap();
    db.insert_order(NewOrder {
        id: "order-b".to_string(),
        run_id: "run-b".to_string(),
        client_order_id: "client-b".to_string(),
        broker_order_id: Some("broker-b".to_string()),
        account_id: "paper".to_string(),
        symbol: "US:NASDAQ:MSFT:EQUITY".to_string(),
        side: "SELL".to_string(),
        order_type: "LIMIT".to_string(),
        price: Some("200".to_string()),
        qty: "2".to_string(),
        filled_qty: "0".to_string(),
        status: "NEW".to_string(),
        created_at_ms: 20,
        updated_at_ms: 20,
    })
    .await
    .unwrap();
    db.insert_fill(NewFill {
        id: "fill-a".to_string(),
        order_id: "order-a".to_string(),
        run_id: "run-a".to_string(),
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        side: "BUY".to_string(),
        price: "100".to_string(),
        qty: "1".to_string(),
        fee: "0.5".to_string(),
        ts_ms: 11,
    })
    .await
    .unwrap();
    db.insert_fill(NewFill {
        id: "fill-b".to_string(),
        order_id: "order-b".to_string(),
        run_id: "run-b".to_string(),
        symbol: "US:NASDAQ:MSFT:EQUITY".to_string(),
        side: "SELL".to_string(),
        price: "200".to_string(),
        qty: "2".to_string(),
        fee: "1.0".to_string(),
        ts_ms: 21,
    })
    .await
    .unwrap();
    db.record_position(PositionCommand {
        run_id: "run-a".to_string(),
        account_id: "paper".to_string(),
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        qty: dec("1"),
        avg_price: dec("100"),
        updated_at_ms: 11,
    })
    .await
    .unwrap();
    db.record_position(PositionCommand {
        run_id: "run-b".to_string(),
        account_id: "paper".to_string(),
        symbol: "US:NASDAQ:MSFT:EQUITY".to_string(),
        qty: dec("2"),
        avg_price: dec("200"),
        updated_at_ms: 21,
    })
    .await
    .unwrap();
    db.upsert_account_balance(NewAccountBalance {
        run_id: "run-a".to_string(),
        account_id: "paper".to_string(),
        asset: "USD".to_string(),
        total: "1000".to_string(),
        available: "900".to_string(),
        frozen: "100".to_string(),
        updated_at_ms: 11,
    })
    .await
    .unwrap();
    db.upsert_account_balance(NewAccountBalance {
        run_id: "run-b".to_string(),
        account_id: "paper".to_string(),
        asset: "USD".to_string(),
        total: "2000".to_string(),
        available: "1800".to_string(),
        frozen: "200".to_string(),
        updated_at_ms: 21,
    })
    .await
    .unwrap();
    db.insert_portfolio_snapshot(NewPortfolioSnapshot {
        id: "snapshot-a".to_string(),
        run_id: "run-a".to_string(),
        account_id: "paper".to_string(),
        ts_ms: 11,
        cash: "900".to_string(),
        market_value: "100".to_string(),
        equity: "1000".to_string(),
        realized_pnl: "0".to_string(),
        unrealized_pnl: "0".to_string(),
    })
    .await
    .unwrap();
    db.insert_portfolio_snapshot(NewPortfolioSnapshot {
        id: "snapshot-b".to_string(),
        run_id: "run-b".to_string(),
        account_id: "paper".to_string(),
        ts_ms: 21,
        cash: "1800".to_string(),
        market_value: "200".to_string(),
        equity: "2000".to_string(),
        realized_pnl: "0".to_string(),
        unrealized_pnl: "0".to_string(),
    })
    .await
    .unwrap();

    let app = api::router_with_state(api::AppState::new(
        db,
        "configs/backtest/ma_cross.toml".into(),
    ));

    for (uri, expected_fragment, unexpected_fragment) in [
        (
            "/api/v1/runs/run-a/orders",
            "\"run_id\":\"run-a\"",
            "\"run_id\":\"run-b\"",
        ),
        (
            "/api/v1/runs/run-a/fills",
            "\"run_id\":\"run-a\"",
            "\"run_id\":\"run-b\"",
        ),
        (
            "/api/v1/runs/run-a/positions",
            "\"run_id\":\"run-a\"",
            "\"run_id\":\"run-b\"",
        ),
        (
            "/api/v1/runs/run-a/account-balances",
            "\"run_id\":\"run-a\"",
            "\"run_id\":\"run-b\"",
        ),
        (
            "/api/v1/runs/run-a/portfolio-snapshots",
            "\"run_id\":\"run-a\"",
            "\"run_id\":\"run-b\"",
        ),
        (
            "/api/v1/orders?run_id=run-a",
            "\"run_id\":\"run-a\"",
            "\"run_id\":\"run-b\"",
        ),
        (
            "/api/v1/fills?run_id=run-a",
            "\"run_id\":\"run-a\"",
            "\"run_id\":\"run-b\"",
        ),
        (
            "/api/v1/positions?run_id=run-a",
            "\"run_id\":\"run-a\"",
            "\"run_id\":\"run-b\"",
        ),
        (
            "/api/v1/account-balances?run_id=run-a",
            "\"run_id\":\"run-a\"",
            "\"run_id\":\"run-b\"",
        ),
        (
            "/api/v1/portfolio/snapshots?run_id=run-a",
            "\"run_id\":\"run-a\"",
            "\"run_id\":\"run-b\"",
        ),
    ] {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{uri}");
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(body.contains(expected_fragment), "{uri}: {body}");
        assert!(!body.contains(unexpected_fragment), "{uri}: {body}");
    }

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/orders")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/runs/run-a/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let metrics: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(metrics["order_count"], 1);
    assert_eq!(metrics["fill_count"], 1);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/metrics?run_id=run-a")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let metrics: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(metrics["order_count"], 1);
    assert_eq!(metrics["fill_count"], 1);
}

#[tokio::test]
async fn replay_launch_uses_request_config_toml_for_distinct_runs() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let bars_path = temp_config_path("request-driven-replay-bars").with_extension("csv");
    std::fs::write(
        &bars_path,
        "ts_ms,open,high,low,close,volume\n1,10,10,10,10,1\n2,11,11,11,11,1\n",
    )
    .unwrap();

    let app = api::router_with_state(api::AppState::new(
        db.clone(),
        "configs/backtest/ma_cross.toml".into(),
    ));

    for run_id in ["request-replay-a", "request-replay-b"] {
        let payload = serde_json::json!({
            "config_toml": replay_launch_config_toml(run_id, &bars_path),
        });
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/replays")
                    .header("content-type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED, "{run_id}");
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let summary: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(summary["bars"], 2);
    }

    let runs = db.list_strategy_runs().await.unwrap();
    assert!(runs.iter().any(|run| run.id == "request-replay-a"));
    assert!(runs.iter().any(|run| run.id == "request-replay-b"));

    for run_id in ["request-replay-a", "request-replay-b"] {
        let binding = db.get_run_config_version_binding(run_id).await.unwrap();
        assert!(binding.is_some(), "{run_id}");
    }
}

#[tokio::test]
async fn replay_launch_with_request_config_does_not_require_default_run_config() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let bars_path = temp_config_path("request-driven-no-default-bars").with_extension("csv");
    std::fs::write(
        &bars_path,
        "ts_ms,open,high,low,close,volume\n1,10,10,10,10,1\n2,11,11,11,11,1\n",
    )
    .unwrap();

    let app = api::router_with_state(api::AppState::with_server_config(
        db.clone(),
        config::ServerConfig::default(),
    ));
    let payload = serde_json::json!({
        "config_toml": replay_launch_config_toml("request-replay-no-default", &bars_path),
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/replays")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    assert!(
        db.get_strategy_run("request-replay-no-default")
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        db.get_run_config_version_binding("request-replay-no-default")
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn run_launch_requires_explicit_config_source_even_with_default_run_config() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = api::router_with_state(api::AppState::new(
        db,
        "configs/backtest/ma_cross.toml".into(),
    ));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/backtests")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("explicit run config"));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/backtests")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "mode": "backtest" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("explicit run config"));
}

#[tokio::test]
async fn replay_launch_can_bind_to_referenced_config_version() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let bars_path = temp_config_path("request-ref-replay-bars").with_extension("csv");
    std::fs::write(
        &bars_path,
        "ts_ms,open,high,low,close,volume\n1,10,10,10,10,1\n2,11,11,11,11,1\n",
    )
    .unwrap();

    let config_toml = replay_launch_config_toml("request-ref-replay", &bars_path);
    let config_value: toml::Value = toml::from_str(&config_toml).unwrap();
    let content_json = serde_json::to_string(&config_value).unwrap();
    db.create_config_version(NewConfigVersion {
        name: "managed-replay-run".to_string(),
        content_json,
        created_by: "test".to_string(),
        parent_version: None,
        target_env: None,
        rollout: None,
        ts_ms: 100,
    })
    .await
    .unwrap();

    let app = api::router_with_state(api::AppState::new(
        db.clone(),
        "configs/backtest/ma_cross.toml".into(),
    ));
    let payload = serde_json::json!({
        "config_ref": {
            "name": "managed-replay-run",
            "version": 1
        }
    });
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/replays")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let run = db
        .get_strategy_run("request-ref-replay")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.mode, "replay");
    let binding = db
        .get_run_config_version_binding("request-ref-replay")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(binding.config_id, "config:managed-replay-run:v1");
    assert_eq!(binding.version, "1");
}

#[tokio::test]
async fn replay_launch_rejects_multiple_config_sources() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let bars_path = temp_config_path("request-multiple-source-bars").with_extension("csv");
    std::fs::write(
        &bars_path,
        "ts_ms,open,high,low,close,volume\n1,10,10,10,10,1\n",
    )
    .unwrap();

    let app = api::router_with_state(api::AppState::new(
        db.clone(),
        "configs/backtest/ma_cross.toml".into(),
    ));
    let payload = serde_json::json!({
        "config_toml": replay_launch_config_toml("multiple-source-replay", &bars_path),
        "config": serde_json::json!({}),
        "mode": "replay"
    });
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/replays")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(
        db.get_strategy_run("multiple-source-replay")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn replay_launch_can_override_run_id_from_referenced_config() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let bars_path = temp_config_path("request-ref-override-bars").with_extension("csv");
    std::fs::write(
        &bars_path,
        "ts_ms,open,high,low,close,volume\n1,10,10,10,10,1\n2,11,11,11,11,1\n",
    )
    .unwrap();

    let config_toml = replay_launch_config_toml("template-replay-run", &bars_path);
    let config_value: toml::Value = toml::from_str(&config_toml).unwrap();
    let content_json = serde_json::to_string(&config_value).unwrap();
    db.create_config_version(NewConfigVersion {
        name: "managed-replay-template".to_string(),
        content_json,
        created_by: "test".to_string(),
        parent_version: None,
        target_env: None,
        rollout: None,
        ts_ms: 100,
    })
    .await
    .unwrap();

    let app = api::router_with_state(api::AppState::new(
        db.clone(),
        "configs/backtest/ma_cross.toml".into(),
    ));
    let payload = serde_json::json!({
        "config_ref": {
            "name": "managed-replay-template",
            "version": 1
        },
        "run_id": "request-override-replay"
    });
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/replays")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    assert!(
        db.get_strategy_run("template-replay-run")
            .await
            .unwrap()
            .is_none()
    );
    let run = db
        .get_strategy_run("request-override-replay")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.mode, "replay");
    let run_config: serde_json::Value = serde_json::from_str(&run.config_json).unwrap();
    assert_eq!(run_config["runtime"]["run_id"], "request-override-replay");
    let binding = db
        .get_run_config_version_binding("request-override-replay")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(binding.config_id, "config:managed-replay-template:v1");
    assert_eq!(binding.version, "1");
}

#[tokio::test]
async fn replay_launch_can_override_mode_from_referenced_config() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let bars_path = temp_config_path("request-ref-mode-override-bars").with_extension("csv");
    std::fs::write(
        &bars_path,
        "ts_ms,open,high,low,close,volume\n1,10,10,10,10,1\n2,11,11,11,11,1\n",
    )
    .unwrap();

    let config_toml =
        replay_launch_config_toml_with_mode("template-mode-replay-run", "backtest", &bars_path);
    let config_value: toml::Value = toml::from_str(&config_toml).unwrap();
    let content_json = serde_json::to_string(&config_value).unwrap();
    db.create_config_version(NewConfigVersion {
        name: "managed-replay-mode-template".to_string(),
        content_json,
        created_by: "test".to_string(),
        parent_version: None,
        target_env: None,
        rollout: None,
        ts_ms: 100,
    })
    .await
    .unwrap();

    let app = api::router_with_state(api::AppState::new(
        db.clone(),
        "configs/backtest/ma_cross.toml".into(),
    ));
    let payload = serde_json::json!({
        "config_ref": {
            "name": "managed-replay-mode-template",
            "version": 1
        },
        "run_id": "request-mode-override-replay",
        "mode": "replay"
    });
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/replays")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    assert!(
        db.get_strategy_run("template-mode-replay-run")
            .await
            .unwrap()
            .is_none()
    );
    let run = db
        .get_strategy_run("request-mode-override-replay")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.mode, "replay");
    let run_config: serde_json::Value = serde_json::from_str(&run.config_json).unwrap();
    assert_eq!(
        run_config["runtime"]["run_id"],
        "request-mode-override-replay"
    );
    assert_eq!(run_config["runtime"]["mode"], "replay");
}

#[tokio::test]
async fn replay_launch_rejects_mode_mismatch_after_overrides() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let bars_path = temp_config_path("request-mode-mismatch-bars").with_extension("csv");
    std::fs::write(
        &bars_path,
        "ts_ms,open,high,low,close,volume\n1,10,10,10,10,1\n2,11,11,11,11,1\n",
    )
    .unwrap();

    let app = api::router_with_state(api::AppState::new(
        db.clone(),
        "configs/backtest/ma_cross.toml".into(),
    ));
    let payload = serde_json::json!({
        "config_toml": replay_launch_config_toml("mode-mismatch-replay", &bars_path),
        "mode": "paper"
    });
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/replays")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(
        db.get_strategy_run("mode-mismatch-replay")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn run_status_prefers_in_memory_runtime_state_for_active_run() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    db.start_strategy_run(StrategyRunStartCommand {
        run_id: "active-run".to_string(),
        name: "sample".to_string(),
        mode: "paper".to_string(),
        started_at_ms: 1,
        config: serde_json::json!({}),
    })
    .await
    .unwrap();

    let state = api::AppState::new(db, "configs/backtest/ma_cross.toml".into());
    let release = Arc::new(Notify::new());
    let release_for_task = release.clone();
    state
        .runtime_manager
        .spawn_with_metadata(
            "active-run".to_string(),
            RuntimeRunMetadata {
                mode: Some("paper".to_string()),
            },
            move |cancel| async move {
                while !cancel.is_cancelled() {
                    release_for_task.notified().await;
                }
            },
        )
        .await
        .unwrap();

    let response = api::router_with_state(state.clone())
        .oneshot(
            Request::builder()
                .uri("/api/v1/runs/active-run/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["run_id"], "active-run");
    assert_eq!(body["status"], "running");
    assert_eq!(body["mode"], "paper");
    assert_eq!(body["status_source"], "runtime_registry");
    assert_eq!(body["mode_source"], "runtime_registry");
    assert_eq!(body["timestamp_source"], "runtime_registry");
    assert!(body["started_at_ms"].as_i64().unwrap() > 0);
    assert!(body["last_state_change_at_ms"].as_i64().unwrap() > 0);

    state.runtime_manager.cancel("active-run").await;
    release.notify_waiters();
    state.runtime_manager.wait_for_idle("active-run").await;
}

#[tokio::test]
async fn run_detail_prefers_in_memory_runtime_state_for_active_run() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    db.start_strategy_run(StrategyRunStartCommand {
        run_id: "detail-run".to_string(),
        name: "sample".to_string(),
        mode: "paper".to_string(),
        started_at_ms: 1,
        config: serde_json::json!({}),
    })
    .await
    .unwrap();

    let state = api::AppState::new(db, "configs/backtest/ma_cross.toml".into());
    let release = Arc::new(Notify::new());
    let release_for_task = release.clone();
    state
        .runtime_manager
        .spawn_with_metadata(
            "detail-run".to_string(),
            RuntimeRunMetadata {
                mode: Some("paper".to_string()),
            },
            move |cancel| async move {
                while !cancel.is_cancelled() {
                    release_for_task.notified().await;
                }
            },
        )
        .await
        .unwrap();

    let response = api::router_with_state(state.clone())
        .oneshot(
            Request::builder()
                .uri("/api/v1/runs/detail-run")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["id"], "detail-run");
    assert_eq!(body["status"], "running");
    assert_eq!(body["mode"], "paper");
    assert_eq!(body["status_source"], "runtime_registry");
    assert_eq!(body["mode_source"], "runtime_registry");
    assert_eq!(body["started_at_ms_source"], "runtime_registry");
    assert_eq!(body["last_state_change_at_ms_source"], "runtime_registry");
    assert!(body["started_at_ms"].as_i64().unwrap() > 0);
    assert!(body["last_state_change_at_ms"].as_i64().unwrap() > 0);
    assert_eq!(body["runtime"]["source"], "runtime_registry");
    assert_eq!(body["runtime"]["mode"], "paper");
    assert_eq!(body["runtime"]["status"], "running");

    state.runtime_manager.cancel("detail-run").await;
    release.notify_waiters();
    state.runtime_manager.wait_for_idle("detail-run").await;
}

#[tokio::test]
async fn run_detail_keeps_terminal_runtime_snapshot_out_of_active_runtime_field() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    db.start_strategy_run(StrategyRunStartCommand {
        run_id: "terminal-detail-run".to_string(),
        name: "sample".to_string(),
        mode: "paper".to_string(),
        started_at_ms: 1,
        config: serde_json::json!({}),
    })
    .await
    .unwrap();

    let state = api::AppState::new(db, "configs/backtest/ma_cross.toml".into());
    state
        .runtime_manager
        .spawn_with_metadata(
            "terminal-detail-run".to_string(),
            RuntimeRunMetadata {
                mode: Some("paper".to_string()),
            },
            |_cancel| async {},
        )
        .await
        .unwrap();
    state
        .runtime_manager
        .wait_for_idle("terminal-detail-run")
        .await;

    let response = api::router_with_state(state)
        .oneshot(
            Request::builder()
                .uri("/api/v1/runs/terminal-detail-run")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["id"], "terminal-detail-run");
    assert_eq!(body["status"], "completed");
    assert_eq!(body["status_source"], "runtime_registry");
    assert!(body["runtime"].is_null(), "{body}");
}

#[tokio::test]
async fn run_list_prefers_in_memory_runtime_state_for_active_run() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    for run_id in ["list-active", "list-idle"] {
        db.start_strategy_run(StrategyRunStartCommand {
            run_id: run_id.to_string(),
            name: "sample".to_string(),
            mode: "paper".to_string(),
            started_at_ms: 1,
            config: serde_json::json!({}),
        })
        .await
        .unwrap();
    }
    db.update_strategy_run_status("list-idle", "completed", Some(2), None)
        .await
        .unwrap();

    let state = api::AppState::new(db, "configs/backtest/ma_cross.toml".into());
    let release = Arc::new(Notify::new());
    let release_for_task = release.clone();
    state
        .runtime_manager
        .spawn_with_metadata(
            "list-active".to_string(),
            RuntimeRunMetadata {
                mode: Some("paper".to_string()),
            },
            move |cancel| async move {
                while !cancel.is_cancelled() {
                    release_for_task.notified().await;
                }
            },
        )
        .await
        .unwrap();

    let response = api::router_with_state(state.clone())
        .oneshot(
            Request::builder()
                .uri("/api/v1/runs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let runs = body.as_array().unwrap();
    let active = runs.iter().find(|run| run["id"] == "list-active").unwrap();
    let idle = runs.iter().find(|run| run["id"] == "list-idle").unwrap();
    assert_eq!(active["status"], "running");
    assert_eq!(active["mode"], "paper");
    assert_eq!(active["status_source"], "runtime_registry");
    assert_eq!(active["mode_source"], "runtime_registry");
    assert_eq!(active["started_at_ms_source"], "runtime_registry");
    assert_eq!(active["last_state_change_at_ms_source"], "runtime_registry");
    assert_eq!(active["runtime"]["source"], "runtime_registry");
    assert_eq!(idle["status"], "completed");
    assert_eq!(idle["status_source"], "storage");
    assert_eq!(idle["mode_source"], "storage");
    assert_eq!(idle["started_at_ms_source"], "storage");
    assert_eq!(idle["ended_at_ms_source"], "storage");
    assert_eq!(idle["last_state_change_at_ms_source"], "storage");

    state.runtime_manager.cancel("list-active").await;
    release.notify_waiters();
    state.runtime_manager.wait_for_idle("list-active").await;
}

#[tokio::test]
async fn live_runtime_route_fails_by_default_for_fake_unmatched_startup_open_orders() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    insert_recoverable_order(&db, "api-live-startup-recovery-fail").await;
    let config_path = temp_config_path("trader-live-startup-recovery-fail");
    std::fs::write(
        &config_path,
        live_startup_recovery_config("api-live-startup-recovery-fail", true, None),
    )
    .unwrap();
    let app = api::router_with_state(api::AppState::new(db, config_path.display().to_string()));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/live-runs")
                .header("content-type", "application/json")
                .body(launch_config_path_body(&config_path, "live"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    wait_for_body_fragment(
        app.clone(),
        "/api/v1/live-runs/api-live-startup-recovery-fail/status",
        "\"status\":\"failed\"",
    )
    .await;
    wait_for_body_fragment(
        app,
        "/api/v1/runs/api-live-startup-recovery-fail/system-logs?target=runtime.startup_recovery",
        "startup_recovery.failed",
    )
    .await;
}

#[tokio::test]
async fn live_runtime_route_warn_only_continues_for_fake_unmatched_startup_open_orders() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    insert_recoverable_order(&db, "api-live-startup-recovery-warn-only").await;
    let config_path = temp_config_path("trader-live-startup-recovery-warn-only");
    std::fs::write(
        &config_path,
        live_startup_recovery_config(
            "api-live-startup-recovery-warn-only",
            true,
            Some("warn_only"),
        ),
    )
    .unwrap();
    let app = api::router_with_state(api::AppState::new(db, config_path.display().to_string()));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/live-runs")
                .header("content-type", "application/json")
                .body(launch_config_path_body(&config_path, "live"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    wait_for_body_fragment(
        app.clone(),
        "/api/v1/live-runs/api-live-startup-recovery-warn-only/status",
        "\"status\":\"running\"",
    )
    .await;
    wait_for_body_fragment(
        app.clone(),
        "/api/v1/runs/api-live-startup-recovery-warn-only/system-logs?target=runtime.startup_recovery",
        "\"unmatched_open_orders\":1",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/live-runs/api-live-startup-recovery-warn-only/stop")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

fn workspace_root() -> std::path::PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("api crate should be under crates/api")
        .to_path_buf()
}

fn temp_config_path(prefix: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{}-{nanos}.toml", std::process::id()))
}

fn launch_config_path_body(path: impl AsRef<std::path::Path>, mode: &str) -> Body {
    let config_toml = std::fs::read_to_string(path).unwrap();
    Body::from(serde_json::json!({ "config_toml": config_toml, "mode": mode }).to_string())
}

fn replay_launch_config_toml(run_id: &str, data_path: &std::path::Path) -> String {
    replay_launch_config_toml_with_mode(run_id, "replay", data_path)
}

fn replay_launch_config_toml_with_mode(
    run_id: &str,
    runtime_mode: &str,
    data_path: &std::path::Path,
) -> String {
    format!(
        r#"
        [runtime]
        mode = "{runtime_mode}"
        run_id = "{run_id}"

        [database]
        url = "sqlite::memory:"

        [data]
        source = "csv"
        path = '{}'

        [strategy]
        name = "moving_average_cross"
        symbols = ["US:NASDAQ:AAPL:EQUITY"]
        fast_window = 2
        slow_window = 3

        [portfolio]
        initial_cash = "100000"
        base_currency = "USD"
        order_qty = "1"
        max_abs_qty = "100"

        [risk]
        max_order_notional = "1000000"
        min_cash_after_order = "0"
        max_exposure = "1000000"
        max_drawdown = "1"
        max_leverage = "10"
        max_margin_used = "0"
        trading_halted = false

        [broker]
        kind = "simulated"
        mode = "paper"

        [paper]
        account_id = "paper"
        slippage_bps = "0"
        fee_bps = "0"

        [live]
        enabled = false
        "#,
        data_path.display()
    )
}

fn live_startup_recovery_config(
    run_id: &str,
    fake_startup_unmatched_open_order: bool,
    unmatched_open_orders_policy: Option<&str>,
) -> String {
    let startup_recovery = unmatched_open_orders_policy
        .map(|policy| {
            format!(
                r#"
        [live.startup_recovery]
        unmatched_open_orders = "{policy}"
"#
            )
        })
        .unwrap_or_default();

    format!(
        r#"
        [runtime]
        mode = "live"
        run_id = "{run_id}"

        [database]
        url = "sqlite::memory:"

        [data]
        source = "csv"
        path = "datasets/sample/aapl_1d.csv"

        [strategy]
        name = "moving_average_cross"
        symbols = ["US:NASDAQ:AAPL:EQUITY"]
        fast_window = 2
        slow_window = 3

        [portfolio]
        initial_cash = "25000"
        base_currency = "USD"
        order_qty = "1"
        max_abs_qty = "100"

        [risk]
        max_order_notional = "1000000"
        min_cash_after_order = "0"
        max_exposure = "1000000"
        max_drawdown = "1"
        max_leverage = "10"
        max_margin_used = "0"
        trading_halted = false

        [broker]
        kind = "simulated"
        mode = "paper"
        fake_startup_unmatched_open_order = {fake_startup_unmatched_open_order}

        [paper]
        account_id = "paper"
        slippage_bps = "25"
        fee_bps = "10"

        [live]
        enabled = true
        {startup_recovery}
        "#
    )
}

async fn insert_recoverable_order(db: &Db, run_id: &str) {
    db.insert_order(NewOrder {
        id: format!("{run_id}-order"),
        run_id: run_id.to_string(),
        client_order_id: format!("{run_id}-client-order"),
        broker_order_id: Some(format!("{run_id}-broker-order")),
        account_id: "paper".to_string(),
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
}

async fn wait_for_body_fragment(app: axum::Router, uri: &str, fragment: &str) {
    for _ in 0..50 {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        if response.status() == StatusCode::OK {
            let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            if bytes
                .as_ref()
                .windows(fragment.len())
                .any(|window| window == fragment.as_bytes())
            {
                return;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("{uri} did not contain {fragment}");
}

async fn get_json(app: axum::Router, uri: &str) -> serde_json::Value {
    let response = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn request_json(
    app: axum::Router,
    method: &str,
    uri: &str,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let response = app
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, body)
}
