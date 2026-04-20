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
            "TSLA.US" => (0.4, 0.95),
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
    db::set_runtime_control(database.pool(), "mode", "observe_only")
        .await
        .expect("mode");
    db::replace_symbol_allowlist(
        database.pool(),
        &[
            ("AAPL.US".to_string(), true),
            ("MSFT.US".to_string(), true),
            ("TSLA.US".to_string(), true),
        ],
    )
    .await
    .expect("allowlist");

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
        database: database.clone(),
        events: event_tx,
        execution_router,
        ingest_registry: registry,
        risk_limits: RiskLimits::default(),
        strategy: Arc::new(RankedTestStrategy),
        api_key: None,
    };
    (api::router(state), database)
}

#[tokio::test]
async fn runtime_cycle_round_trip() {
    let (app, _database) = test_app().await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/runtime/cycle")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"venue":"US_EQUITY","account_id":"acc_mvp_paper"}"#,
                ))
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
    let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(json["mode"], "observe_only");
    assert_eq!(json["accepted"], serde_json::json!(["AAPL.US", "MSFT.US"]));
    assert_eq!(json["placed"], serde_json::json!([]));

    let latest = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/runtime/cycle/latest")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(latest.status(), StatusCode::OK);
    let latest_body = latest.into_body().collect().await.expect("body").to_bytes();
    let latest_json: serde_json::Value = serde_json::from_slice(&latest_body).expect("json");
    assert_eq!(latest_json["mode"], "observe_only");
    assert_eq!(
        latest_json["rejected"][0]["reason"],
        "score_below_threshold"
    );

    let history = app
        .oneshot(
            Request::builder()
                .uri("/v1/runtime/cycle/history?limit=5")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(history.status(), StatusCode::OK);
    let history_body = history
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let history_json: serde_json::Value = serde_json::from_slice(&history_body).expect("json");
    assert_eq!(history_json.as_array().map(|items| items.len()), Some(1));
    assert_eq!(
        history_json[0]["accepted"],
        serde_json::json!(["AAPL.US", "MSFT.US"])
    );
}

#[tokio::test]
async fn runtime_execution_state_exposes_positions_orders_and_latest_cycle() {
    let (app, database) = test_app().await;
    db::set_runtime_control(database.pool(), "mode", "paper_only")
        .await
        .expect("mode");
    let instrument_id = db::upsert_instrument(database.pool(), Venue::UsEquity.as_str(), "AAPL.US")
        .await
        .expect("instrument");

    db::insert_order(
        database.pool(),
        &db::NewOrder {
            order_id: "open-order-1",
            account_id: "acc_mvp_paper",
            instrument_id,
            side: "buy",
            qty: 1.0,
            status: "SUBMITTED",
            order_type: "limit",
            limit_price: Some(100.0),
            exchange_ref: Some("paper-open-order-1"),
            idempotency_key: Some("open-order-key"),
            created_at_ms: 10,
            updated_at_ms: 10,
        },
    )
    .await
    .expect("open order");

    db::insert_order(
        database.pool(),
        &db::NewOrder {
            order_id: "filled-order-1",
            account_id: "acc_mvp_paper",
            instrument_id,
            side: "buy",
            qty: 2.0,
            status: "FILLED",
            order_type: "limit",
            limit_price: Some(100.0),
            exchange_ref: Some("paper-filled-order-1"),
            idempotency_key: Some("filled-order-key"),
            created_at_ms: 20,
            updated_at_ms: 20,
        },
    )
    .await
    .expect("filled order");
    db::insert_fill(
        database.pool(),
        &db::NewFill {
            fill_id: "fill-1",
            order_id: "filled-order-1",
            qty: 2.0,
            price: 100.0,
            created_at_ms: 21,
        },
    )
    .await
    .expect("fill");

    let cycle = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/runtime/cycle")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"venue":"US_EQUITY","account_id":"acc_mvp_paper"}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("cycle response");
    assert_eq!(cycle.status(), StatusCode::OK);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/runtime/execution-state?account_id=acc_mvp_paper")
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
    let json: serde_json::Value = serde_json::from_slice(&body).expect("json");

    assert_eq!(json["account_id"], "acc_mvp_paper");
    assert_eq!(json["positions"][0]["symbol"], "AAPL.US");
    assert_eq!(json["positions"][0]["net_qty"], 2.0);
    assert_eq!(json["open_orders"][0]["order_id"], "open-order-1");
    assert_eq!(json["open_orders"][0]["status"], "SUBMITTED");
    assert_eq!(json["latest_cycle"]["mode"], "paper_only");
    assert_eq!(
        json["latest_cycle"]["accepted"],
        serde_json::json!(["AAPL.US", "MSFT.US"])
    );
    assert!(
        json["latest_cycle"]["skipped"]
            .as_array()
            .expect("skipped")
            .iter()
            .any(|item| item["reason"] == "guard_open_order_exists")
    );
}

#[tokio::test]
async fn runtime_reconciliation_latest_exposes_snapshot_and_local_state() {
    let (app, database) = test_app().await;
    db::set_runtime_control(database.pool(), "mode", "observe_only")
        .await
        .expect("mode");
    let instrument_id = db::upsert_instrument(database.pool(), Venue::UsEquity.as_str(), "AAPL.US")
        .await
        .expect("instrument");

    db::insert_order(
        database.pool(),
        &db::NewOrder {
            order_id: "recon-open-order",
            account_id: "acc_mvp_paper",
            instrument_id,
            side: "buy",
            qty: 1.0,
            status: "SUBMITTED",
            order_type: "limit",
            limit_price: Some(100.0),
            exchange_ref: Some("paper-recon-open-order"),
            idempotency_key: Some("recon-open-order-key"),
            created_at_ms: 10,
            updated_at_ms: 10,
        },
    )
    .await
    .expect("open order");
    db::insert_order(
        database.pool(),
        &db::NewOrder {
            order_id: "recon-filled-order",
            account_id: "acc_mvp_paper",
            instrument_id,
            side: "buy",
            qty: 2.0,
            status: "FILLED",
            order_type: "limit",
            limit_price: Some(100.0),
            exchange_ref: Some("paper-recon-filled-order"),
            idempotency_key: Some("recon-filled-order-key"),
            created_at_ms: 20,
            updated_at_ms: 20,
        },
    )
    .await
    .expect("filled order");
    db::insert_fill(
        database.pool(),
        &db::NewFill {
            fill_id: "recon-fill",
            order_id: "recon-filled-order",
            qty: 2.0,
            price: 100.0,
            created_at_ms: 21,
        },
    )
    .await
    .expect("fill");

    let snapshot = db::ReconciliationSnapshot {
        id: "snapshot-1",
        account_id: "acc_mvp_paper",
        broker_cash: 0.0,
        local_cash: 0.0,
        broker_positions_json: r#"[{"symbol":"AAPL.US","net_qty":1.0}]"#,
        local_positions_json: r#"[{"symbol":"AAPL.US","net_qty":2.0}]"#,
        mismatch_count: 1,
        status: "mismatch",
    };
    db::insert_reconciliation_snapshot(database.pool(), &snapshot)
        .await
        .expect("snapshot");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/runtime/reconciliation/latest?account_id=acc_mvp_paper")
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
    let json: serde_json::Value = serde_json::from_slice(&body).expect("json");

    assert_eq!(json["account_id"], "acc_mvp_paper");
    assert_eq!(json["runtime_mode"], "observe_only");
    assert_eq!(json["local_positions"][0]["symbol"], "AAPL.US");
    assert_eq!(json["local_positions"][0]["net_qty"], 2.0);
    assert_eq!(json["local_open_orders"][0]["order_id"], "recon-open-order");
    assert_eq!(json["latest_snapshot"]["status"], "mismatch");
    assert_eq!(json["latest_snapshot"]["mismatch_count"], 1);
    assert_eq!(
        json["latest_snapshot"]["broker_positions"][0]["symbol"],
        "AAPL.US"
    );
    assert_eq!(
        json["latest_snapshot"]["local_positions"][0]["net_qty"],
        2.0
    );
}
