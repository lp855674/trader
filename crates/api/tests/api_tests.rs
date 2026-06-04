use api::router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use std::path::PathBuf;
use storage::Db;
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
