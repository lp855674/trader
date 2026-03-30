//! Task 10: integration-style HTTP checks via `tower::ServiceExt::oneshot`.

use std::collections::HashMap;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use db::Db;
use domain::Venue;
use exec::{ExecutionAdapter, ExecutionRouter, PaperAdapter};
use http_body_util::BodyExt;
use ingest::{IngestRegistry, MockBarsAdapter};
use pipeline::RiskLimits;
use tokio::sync::broadcast;
use tower::ServiceExt;

async fn test_app() -> Router {
    let database = Db::connect("sqlite::memory:")
        .await
        .expect("db connect");
    db::ensure_mvp_seed(database.pool()).await.expect("seed");

    let paper = Arc::new(PaperAdapter::new(database.clone()));
    let mut routes = HashMap::new();
    routes.insert(
        "acc_mvp_paper".to_string(),
        paper as Arc<dyn ExecutionAdapter>,
    );
    let execution_router = ExecutionRouter::new(routes);

    let mut registry = IngestRegistry::default();
    registry.register(Arc::new(MockBarsAdapter::new(Venue::UsEquity, "mock_us")));
    registry.register(Arc::new(MockBarsAdapter::new(Venue::HkEquity, "mock_hk")));
    registry.register(Arc::new(MockBarsAdapter::new(Venue::Crypto, "mock_crypto")));
    registry.register(Arc::new(MockBarsAdapter::new(
        Venue::Polymarket,
        "mock_poly",
    )));

    let (event_tx, _event_rx) = broadcast::channel::<api::StreamEvent>(8);
    let state = api::AppState {
        database,
        events: event_tx,
        execution_router,
        ingest_registry: registry,
        risk_limits: RiskLimits::default(),
        api_key: None,
    };
    api::router(state)
}

async fn test_app_with_key(key: &str) -> Router {
    // Rebuild with key by duplicating minimal setup to keep this file standalone.
    let database = Db::connect("sqlite::memory:").await.expect("db connect");
    db::ensure_mvp_seed(database.pool()).await.expect("seed");

    let paper = Arc::new(PaperAdapter::new(database.clone()));
    let mut routes = HashMap::new();
    routes.insert(
        "acc_mvp_paper".to_string(),
        paper as Arc<dyn ExecutionAdapter>,
    );
    let execution_router = ExecutionRouter::new(routes);

    let mut registry = IngestRegistry::default();
    registry.register(Arc::new(MockBarsAdapter::new(Venue::UsEquity, "mock_us")));
    registry.register(Arc::new(MockBarsAdapter::new(Venue::HkEquity, "mock_hk")));
    registry.register(Arc::new(MockBarsAdapter::new(Venue::Crypto, "mock_crypto")));
    registry.register(Arc::new(MockBarsAdapter::new(
        Venue::Polymarket,
        "mock_poly",
    )));

    let (event_tx, _event_rx) = broadcast::channel::<api::StreamEvent>(8);
    let state = api::AppState {
        database,
        events: event_tx,
        execution_router,
        ingest_registry: registry,
        risk_limits: RiskLimits::default(),
        api_key: Some(key.to_string()),
    };
    api::router(state)
}

#[tokio::test]
async fn get_health_returns_ok_json() {
    let app = test_app().await;
    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(v["status"], "ok");
}

#[tokio::test]
async fn get_instruments_returns_json_array() {
    let app = test_app().await;
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/instruments")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert!(v.is_array(), "expected JSON array, got {v}");
}

#[tokio::test]
async fn v1_routes_require_key_when_configured() {
    let app = test_app_with_key("k").await;
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/instruments")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn v1_routes_accept_bearer_key() {
    let app = test_app_with_key("k").await;
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/instruments")
                .header("authorization", "Bearer k")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
}
