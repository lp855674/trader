use api::router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use std::path::PathBuf;
use storage::{Db, RuntimeEventCommand, StrategyRunStartCommand};
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
