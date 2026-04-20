use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use db::Db;
use domain::Venue;
use exec::{ExecutionAdapter, ExecutionRouter, PaperAdapter};
use futures::StreamExt;
use ingest::{IngestRegistry, MockBarsAdapter};
use pipeline::RiskLimits;
use strategy::NoOpStrategy;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

async fn test_app() -> (Router, broadcast::Sender<api::StreamEvent>) {
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

    let (event_tx, _event_rx) = broadcast::channel::<api::StreamEvent>(8);
    let state = api::AppState {
        database,
        events: event_tx.clone(),
        execution_router,
        ingest_registry: registry,
        risk_limits: RiskLimits::default(),
        strategy: Arc::new(NoOpStrategy),
        api_key: None,
    };
    (api::router(state), event_tx)
}

#[tokio::test]
async fn websocket_connection_stays_open_and_delivers_events() {
    let (app, event_tx) = test_app().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local addr");

    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });

    let url = format!("ws://{addr}/v1/stream");
    let (mut stream, _response) = connect_async(&url).await.expect("ws connect");

    let hello = stream
        .next()
        .await
        .expect("hello frame")
        .expect("hello frame ok");
    let Message::Text(hello_text) = hello else {
        panic!("expected text hello");
    };
    let hello_json: serde_json::Value =
        serde_json::from_str(&hello_text).expect("hello json");
    assert_eq!(hello_json["kind"], "hello");

    tokio::time::sleep(Duration::from_millis(200)).await;

    event_tx
        .send(api::StreamEvent::Error {
            error_code: "smoke".to_string(),
            message: "alive".to_string(),
        })
        .expect("broadcast event");

    let next = tokio::time::timeout(Duration::from_secs(1), stream.next())
        .await
        .expect("event timeout")
        .expect("event frame")
        .expect("event frame ok");
    let Message::Text(event_text) = next else {
        panic!("expected text event");
    };
    let event_json: serde_json::Value =
        serde_json::from_str(&event_text).expect("event json");
    assert_eq!(event_json["kind"], "error");
    assert_eq!(event_json["error_code"], "smoke");
    assert_eq!(event_json["message"], "alive");

    server.abort();
}
