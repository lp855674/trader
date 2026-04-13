use std::collections::HashMap;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use db::Db;
use domain::Venue;
use exec::{ExecutionAdapter, ExecutionRouter, PaperAdapter};
use http_body_util::BodyExt;
use ingest::{IngestRegistry, MockBarsAdapter};
use pipeline::RiskLimits;
use strategy::NoOpStrategy;
use tokio::sync::broadcast;
use tower::ServiceExt;

async fn test_app() -> Router {
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
    registry.register(Arc::new(MockBarsAdapter::paper_bars(Venue::UsEquity)));
    registry.register(Arc::new(MockBarsAdapter::paper_bars(Venue::HkEquity)));
    registry.register(Arc::new(MockBarsAdapter::paper_bars(Venue::Crypto)));
    registry.register(Arc::new(MockBarsAdapter::paper_bars(Venue::Polymarket)));

    let (event_tx, _event_rx) = broadcast::channel::<api::StreamEvent>(8);
    let state = api::AppState {
        database,
        events: event_tx,
        execution_router,
        ingest_registry: registry,
        risk_limits: RiskLimits::default(),
        strategy: Arc::new(NoOpStrategy),
        api_key: None,
    };
    api::router(state)
}

#[tokio::test]
async fn runtime_controls_round_trip() {
    let app = test_app().await;

    let get_default = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/runtime/mode")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(get_default.status(), StatusCode::OK);
    let default_body = get_default
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let default_json: serde_json::Value = serde_json::from_slice(&default_body).expect("json");
    assert_eq!(default_json["mode"], "observe_only");

    let put_mode = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/v1/runtime/mode")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"mode":"paper_only"}"#))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(put_mode.status(), StatusCode::NO_CONTENT);

    let get_mode = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/runtime/mode")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(get_mode.status(), StatusCode::OK);
    let mode_body = get_mode
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let mode_json: serde_json::Value = serde_json::from_slice(&mode_body).expect("json");
    assert_eq!(mode_json["mode"], "paper_only");

    let put_allowlist = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/v1/runtime/allowlist")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"symbols":[{"symbol":"AAPL.US","enabled":true},{"symbol":"MSFT.US","enabled":false}]}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(put_allowlist.status(), StatusCode::NO_CONTENT);

    let get_allowlist = app
        .oneshot(
            Request::builder()
                .uri("/v1/runtime/allowlist")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(get_allowlist.status(), StatusCode::OK);
    let allowlist_body = get_allowlist
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let allowlist_json: serde_json::Value = serde_json::from_slice(&allowlist_body).expect("json");
    assert_eq!(
        allowlist_json,
        serde_json::json!({
            "symbols": [
                {"symbol": "AAPL.US", "enabled": true},
                {"symbol": "MSFT.US", "enabled": false}
            ]
        })
    );
}

#[tokio::test]
async fn put_runtime_mode_rejects_unknown_mode() {
    let app = test_app().await;
    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/v1/runtime/mode")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"mode":"live_now"}"#))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
