use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use domain::{Side, Signal, Venue};
use exec::{ExecutionAdapter, ExecutionRouter, PaperAdapter};
use ingest::MockBarsAdapter;
use pipeline::{RiskLimits, UniverseRunParams, run_universe_cycle};
use strategy::{Strategy, StrategyContext};

struct RankedBuyStrategy;

#[async_trait]
impl Strategy for RankedBuyStrategy {
    async fn evaluate(&self, context: &StrategyContext) -> Option<Signal> {
        let limit_price = context.last_bar_close?;
        Some(Signal {
            strategy_id: "ranked_buy".to_string(),
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
    ) -> Result<Option<strategy::ScoredCandidate>, String> {
        Ok(Some(strategy::ScoredCandidate {
            symbol: context.instrument.symbol.clone(),
            score: 0.9,
            confidence: 0.9,
        }))
    }
}

async fn setup_router(database: &db::Db) -> ExecutionRouter {
    let paper = Arc::new(PaperAdapter::new(database.clone()));
    let mut routes = HashMap::new();
    routes.insert(
        "acc_mvp_paper".to_string(),
        paper as Arc<dyn ExecutionAdapter>,
    );
    ExecutionRouter::new(routes)
}

async fn setup_database() -> db::Db {
    let database = db::Db::connect("sqlite::memory:").await.expect("db");
    db::ensure_mvp_seed(database.pool()).await.expect("seed");
    db::set_runtime_control(database.pool(), "mode", "paper_only")
        .await
        .expect("mode");
    db::replace_symbol_allowlist(database.pool(), &[("AAPL.US".to_string(), true)])
        .await
        .expect("allowlist");
    database
}

#[tokio::test]
async fn duplicate_cycle_order_is_blocked_in_same_bucket() {
    let database = setup_database().await;
    let router = setup_router(&database).await;
    let strategy = RankedBuyStrategy;
    let ingest = MockBarsAdapter::paper_bars(Venue::UsEquity);

    let first = run_universe_cycle(
        &database,
        &ingest,
        &router,
        &strategy,
        RiskLimits::default(),
        &UniverseRunParams {
            account_id: "acc_mvp_paper".to_string(),
            venue: Venue::UsEquity,
            ts_ms: 310_000,
        },
    )
    .await
    .expect("first cycle");
    assert_eq!(first.placed.len(), 1);

    let second = run_universe_cycle(
        &database,
        &ingest,
        &router,
        &strategy,
        RiskLimits::default(),
        &UniverseRunParams {
            account_id: "acc_mvp_paper".to_string(),
            venue: Venue::UsEquity,
            ts_ms: 320_000,
        },
    )
    .await
    .expect("second cycle");

    assert!(second.placed.is_empty());
    assert!(
        second
            .skipped
            .iter()
            .any(|decision| decision.reason == "guard_duplicate_idempotency")
    );
    let order_count = db::count_orders_for_account(database.pool(), "acc_mvp_paper")
        .await
        .expect("order count");
    assert_eq!(order_count, 1);
}

#[tokio::test]
async fn same_direction_existing_position_is_reported_as_skipped() {
    let database = setup_database().await;
    let instrument_id = db::upsert_instrument(database.pool(), Venue::UsEquity.as_str(), "AAPL.US")
        .await
        .expect("instrument");
    db::insert_order(
        database.pool(),
        &db::NewOrder {
            order_id: "existing-long-order",
            account_id: "acc_mvp_paper",
            instrument_id,
            side: "buy",
            qty: 1.0,
            status: "FILLED",
            order_type: "limit",
            limit_price: Some(100.0),
            exchange_ref: Some("paper-existing-long-order"),
            idempotency_key: Some("existing-long-key"),
            created_at_ms: 1,
            updated_at_ms: 1,
        },
    )
    .await
    .expect("seed order");
    db::insert_fill(
        database.pool(),
        &db::NewFill {
            fill_id: "existing-long-fill",
            order_id: "existing-long-order",
            qty: 1.0,
            price: 100.0,
            created_at_ms: 1,
        },
    )
    .await
    .expect("seed fill");

    let router = setup_router(&database).await;
    let strategy = RankedBuyStrategy;
    let ingest = MockBarsAdapter::paper_bars(Venue::UsEquity);

    let report = run_universe_cycle(
        &database,
        &ingest,
        &router,
        &strategy,
        RiskLimits::default(),
        &UniverseRunParams {
            account_id: "acc_mvp_paper".to_string(),
            venue: Venue::UsEquity,
            ts_ms: 600_000,
        },
    )
    .await
    .expect("cycle");

    assert!(report.placed.is_empty());
    assert!(
        report
            .skipped
            .iter()
            .any(|decision| decision.reason == "guard_same_direction_position_open")
    );
}

#[tokio::test]
async fn open_order_is_reported_as_skipped() {
    let database = setup_database().await;
    let instrument_id = db::upsert_instrument(database.pool(), Venue::UsEquity.as_str(), "AAPL.US")
        .await
        .expect("instrument");
    db::insert_order(
        database.pool(),
        &db::NewOrder {
            order_id: "submitted-order",
            account_id: "acc_mvp_paper",
            instrument_id,
            side: "buy",
            qty: 1.0,
            status: "SUBMITTED",
            order_type: "limit",
            limit_price: Some(100.0),
            exchange_ref: Some("paper-submitted-order"),
            idempotency_key: Some("submitted-key"),
            created_at_ms: 1,
            updated_at_ms: 1,
        },
    )
    .await
    .expect("seed order");

    let router = setup_router(&database).await;
    let strategy = RankedBuyStrategy;
    let ingest = MockBarsAdapter::paper_bars(Venue::UsEquity);

    let report = run_universe_cycle(
        &database,
        &ingest,
        &router,
        &strategy,
        RiskLimits::default(),
        &UniverseRunParams {
            account_id: "acc_mvp_paper".to_string(),
            venue: Venue::UsEquity,
            ts_ms: 600_000,
        },
    )
    .await
    .expect("cycle");

    assert!(report.placed.is_empty());
    assert!(
        report
            .skipped
            .iter()
            .any(|decision| decision.reason == "guard_open_order_exists")
    );
}
