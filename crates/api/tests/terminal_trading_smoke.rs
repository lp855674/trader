use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use db::Db;
use domain::{Side, Signal, Venue};
use exec::{ExecutionAdapter, ExecutionRouter, PaperAdapter};
use http_body_util::BodyExt;
use ingest::{IngestRegistry, MockBarsAdapter};
use pipeline::RiskLimits;
use strategy::{ScoredCandidate, Strategy, StrategyContext};
use tokio::sync::broadcast;
use tower::ServiceExt;

struct RankedTestStrategy;

#[async_trait]
impl Strategy for RankedTestStrategy {
    async fn evaluate(&self, context: &StrategyContext) -> Option<Signal> {
        let limit_price = context.last_bar_close?;
        Some(Signal {
            strategy_id: "ranked_test".to_string(),
            instrument: context.instrument.clone(),
            instrument_db_id: context.instrument_db_id,
            side: Side::Buy,
            qty: 1.0,
            limit_price,
            ts_ms: context.ts_ms,
        })
    }

    async fn evaluate_candidate(
        &self,
        context: &StrategyContext,
    ) -> Result<Option<ScoredCandidate>, String> {
        let (score, confidence) = match context.instrument.symbol.as_str() {
            "AAPL.US" => (0.9, 0.9),
            "MSFT.US" => (0.7, 0.8),
            other => return Err(format!("unexpected symbol:{other}")),
        };
        Ok(Some(ScoredCandidate {
            symbol: context.instrument.symbol.clone(),
            score,
            confidence,
        }))
    }
}

async fn test_app() -> (Router, Db) {
    let database = Db::connect("sqlite::memory:").await.expect("db connect");
    db::ensure_mvp_seed(database.pool()).await.expect("seed");
    db::replace_symbol_allowlist(database.pool(), &[("AAPL.US".to_string(), true)])
        .await
        .expect("allowlist");
    let instrument_id = db::upsert_instrument(database.pool(), Venue::UsEquity.as_str(), "AAPL.US")
        .await
        .expect("aapl instrument");
    db::insert_bar(
        database.pool(),
        &db::NewBar {
            instrument_id,
            data_source_id: db::PAPER_BARS_DATA_SOURCE_ID,
            ts_ms: 1000,
            open: 120.0,
            high: 125.0,
            low: 119.5,
            close: 123.45,
            volume: 1000.0,
        },
    )
    .await
    .expect("bar");
    db::insert_bar(
        database.pool(),
        &db::NewBar {
            instrument_id,
            data_source_id: db::PAPER_BARS_DATA_SOURCE_ID,
            ts_ms: 2000,
            open: 123.45,
            high: 126.0,
            low: 122.5,
            close: 124.0,
            volume: 1500.0,
        },
    )
    .await
    .expect("bar 2");

    let paper = Arc::new(PaperAdapter::new(database.clone()));
    let mut routes = HashMap::new();
    routes.insert(
        "acc_mvp_paper".to_string(),
        paper as Arc<dyn ExecutionAdapter>,
    );

    let mut registry = IngestRegistry::default();
    registry.register(Arc::new(MockBarsAdapter::paper_bars(Venue::UsEquity)));

    let (event_tx, _event_rx) = broadcast::channel::<api::StreamEvent>(8);
    let state = api::AppState {
        database: database.clone(),
        events: event_tx,
        execution_router: ExecutionRouter::new(routes),
        ingest_registry: registry,
        risk_limits: RiskLimits::default(),
        strategy: Arc::new(RankedTestStrategy),
        api_key: None,
    };
    (api::router(state), database)
}

#[tokio::test]
async fn terminal_order_routes_submit_cancel_and_amend() {
    let (app, _database) = test_app().await;

    let submit = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/orders")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"account_id":"acc_mvp_paper","symbol":"AAPL.US","side":"buy","qty":10.0,"order_type":"limit","limit_price":123.45}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(submit.status(), StatusCode::CREATED);

    let body = submit.into_body().collect().await.expect("body").to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
    let order_id = json["order_id"].as_str().expect("order id").to_string();

    let amend = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/orders/{order_id}/amend"))
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"account_id":"acc_mvp_paper","qty":12.0,"limit_price":124.0}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(amend.status(), StatusCode::OK);

    let cancel = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/orders/{order_id}/cancel"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"account_id":"acc_mvp_paper"}"#))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(cancel.status(), StatusCode::OK);
}

#[tokio::test]
async fn list_orders_route_returns_operator_facing_rows() {
    let (app, _database) = test_app().await;

    let submit = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/orders")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"account_id":"acc_mvp_paper","symbol":"AAPL.US","side":"buy","qty":10.0,"order_type":"limit","limit_price":123.45}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(submit.status(), StatusCode::CREATED);
    let submit_body = submit.into_body().collect().await.expect("body").to_bytes();
    let submit_json: serde_json::Value = serde_json::from_slice(&submit_body).expect("json");
    let order_id = submit_json["order_id"].as_str().expect("order id").to_string();

    let list = app
        .oneshot(
            Request::builder()
                .uri("/v1/orders?account_id=acc_mvp_paper")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(list.status(), StatusCode::OK);
    let body = list.into_body().collect().await.expect("body").to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
    let rows = json.as_array().expect("rows");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["order_id"], order_id);
    assert_eq!(rows[0]["symbol"], "AAPL.US");
    assert_eq!(rows[0]["venue"], "US_EQUITY");
    assert_eq!(rows[0]["status"], "SUBMITTED");
}

#[tokio::test]
async fn manual_order_submit_upserts_missing_instrument_for_symbol() {
    let database = Db::connect("sqlite::memory:").await.expect("db connect");
    db::ensure_mvp_seed(database.pool()).await.expect("seed");

    let paper = Arc::new(PaperAdapter::new(database.clone()));
    let mut routes = HashMap::new();
    routes.insert(
        "acc_mvp_paper".to_string(),
        paper as Arc<dyn ExecutionAdapter>,
    );
    let (event_tx, _event_rx) = broadcast::channel::<api::StreamEvent>(8);
    let state = api::AppState {
        database: database.clone(),
        events: event_tx,
        execution_router: ExecutionRouter::new(routes),
        ingest_registry: IngestRegistry::default(),
        risk_limits: RiskLimits::default(),
        strategy: Arc::new(RankedTestStrategy),
        api_key: None,
    };
    let app = api::router(state);

    let submit = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/orders")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"account_id":"acc_mvp_paper","symbol":"AAPL.US","side":"buy","qty":10.0,"order_type":"limit","limit_price":123.45}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(submit.status(), StatusCode::CREATED);

    let list = app
        .oneshot(
            Request::builder()
                .uri("/v1/orders?account_id=acc_mvp_paper")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(list.status(), StatusCode::OK);
    let body = list.into_body().collect().await.expect("body").to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
    let rows = json.as_array().expect("rows");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["symbol"], "AAPL.US");
}

#[tokio::test]
async fn terminal_overview_and_quote_routes_return_operator_facing_data() {
    let (app, _database) = test_app().await;

    let overview = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/terminal/overview?account_id=acc_mvp_paper")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(overview.status(), StatusCode::OK);
    let overview_body = overview
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let overview_json: serde_json::Value = serde_json::from_slice(&overview_body).expect("json");
    assert_eq!(overview_json["account_id"], "acc_mvp_paper");
    assert_eq!(overview_json["runtime_mode"], "observe_only");
    assert!(
        overview_json["watchlist"]
            .as_array()
            .expect("watchlist")
            .iter()
            .any(|item| item["symbol"] == "AAPL.US")
    );

    let quote = app
        .oneshot(
            Request::builder()
                .uri("/v1/quotes/AAPL.US")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(quote.status(), StatusCode::OK);
    let quote_body = quote.into_body().collect().await.expect("body").to_bytes();
    let quote_json: serde_json::Value = serde_json::from_slice(&quote_body).expect("json");
    assert_eq!(quote_json["symbol"], "AAPL.US");
    assert_eq!(quote_json["venue"], "US_EQUITY");
    assert_eq!(quote_json["last_price"], 124.0);
    assert_eq!(quote_json["day_high"], 126.0);
    assert_eq!(quote_json["day_low"], 119.5);
    assert_eq!(quote_json["bars"].as_array().map(|items| items.len()), Some(2));
}

#[tokio::test]
async fn quote_route_returns_empty_view_for_allowlist_symbol_without_instrument() {
    let (app, database) = test_app().await;
    db::replace_symbol_allowlist(
        database.pool(),
        &[
            ("AAPL.US".to_string(), true),
            ("MSFT.US".to_string(), true),
        ],
    )
    .await
    .expect("allowlist");

    let quote = app
        .oneshot(
            Request::builder()
                .uri("/v1/quotes/MSFT.US")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(quote.status(), StatusCode::OK);
    let quote_body = quote.into_body().collect().await.expect("body").to_bytes();
    let quote_json: serde_json::Value = serde_json::from_slice(&quote_body).expect("json");
    assert_eq!(quote_json["symbol"], "MSFT.US");
    assert_eq!(quote_json["venue"], "US_EQUITY");
    assert!(quote_json["last_price"].is_null());
    assert!(quote_json["day_high"].is_null());
    assert!(quote_json["day_low"].is_null());
    assert_eq!(quote_json["bars"].as_array().map(|items| items.len()), Some(0));
}
