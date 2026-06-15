use api::router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use rust_decimal::Decimal;
use std::path::PathBuf;
use storage::{
    CryptoPositionCommand, Db, FundingRateCommand, RuntimeEventCommand, StrategyRunStartCommand,
};
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
            .uri("/api/v1/brokers/account/paper")
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
async fn paper_preflight_returns_configured_dry_run_summary() {
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
                .body(Body::empty())
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

fn dec(value: &str) -> Decimal {
    value.parse().unwrap()
}

fn workspace_root() -> std::path::PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("api crate should be under crates/api")
        .to_path_buf()
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
