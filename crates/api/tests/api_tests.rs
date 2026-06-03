use api::router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
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
