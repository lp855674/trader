//! quantd library surface (re-exports for integration tests).

use db::ReconciliationSnapshot;
use domain::Venue;
use exec::ExecutionRouter;
use ingest::IngestRegistry;
use std::time::{SystemTime, UNIX_EPOCH};
use strategy::Strategy;
use uuid::Uuid;

pub use pipeline::{PipelineError, RiskLimits, VenueTickParams, run_one_tick_for_venue};

const RUNTIME_MODE_KEY: &str = "mode";
const OBSERVE_ONLY_MODE: &str = "observe_only";

pub async fn init_runtime_defaults(database: &db::Db) -> Result<(), db::DbError> {
    if db::get_runtime_control(database.pool(), RUNTIME_MODE_KEY)
        .await?
        .is_none()
    {
        db::set_runtime_control(database.pool(), RUNTIME_MODE_KEY, OBSERVE_ONLY_MODE).await?;
    }
    Ok(())
}

pub async fn set_runtime_mode(database: &db::Db, mode: &str) -> Result<(), db::DbError> {
    db::set_runtime_control(database.pool(), RUNTIME_MODE_KEY, mode).await
}

pub async fn record_reconciliation_failure(
    database: &db::Db,
    account_id: &str,
    status: &str,
) -> Result<(), db::DbError> {
    let snapshot_id = Uuid::new_v4().to_string();
    let snapshot = ReconciliationSnapshot {
        id: &snapshot_id,
        account_id,
        broker_cash: 0.0,
        local_cash: 0.0,
        broker_positions_json: "[]",
        local_positions_json: "[]",
        mismatch_count: 1,
        status,
    };
    db::insert_reconciliation_snapshot(database.pool(), &snapshot).await
}

pub async fn run_background_universe_cycle_once(
    database: &db::Db,
    ingest_registry: &IngestRegistry,
    execution_router: &ExecutionRouter,
    strategy: &dyn Strategy,
    risk_limits: RiskLimits,
    venue: Venue,
    account_id: &str,
) -> Result<Option<pipeline::UniverseCycleReport>, PipelineError> {
    let Some(adapter) = ingest_registry.adapter_for_venue(venue) else {
        return Ok(None);
    };
    let ts_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0);
    let params = pipeline::UniverseRunParams {
        account_id: account_id.to_string(),
        venue,
        ts_ms,
    };
    let report = pipeline::run_universe_cycle(
        database,
        adapter.as_ref(),
        execution_router,
        strategy,
        risk_limits,
        &params,
    )
    .await?;
    Ok(Some(report))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use async_trait::async_trait;
    use domain::{Side, Signal, Venue};
    use exec::{ExecutionAdapter, ExecutionRouter, PaperAdapter};
    use ingest::{IngestRegistry, MockBarsAdapter};
    use pipeline::RiskLimits;
    use strategy::{ScoredCandidate, Strategy, StrategyContext};

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
                _ => return Ok(None),
            };
            Ok(Some(ScoredCandidate {
                symbol: context.instrument.symbol.clone(),
                score,
                confidence,
            }))
        }
    }

    #[tokio::test]
    async fn startup_sets_observe_only_mode_when_missing() {
        let database = db::Db::connect("sqlite::memory:").await.expect("db");

        super::init_runtime_defaults(&database)
            .await
            .expect("init defaults");

        let mode = db::get_runtime_control(database.pool(), "mode")
            .await
            .expect("mode");
        assert_eq!(mode.as_deref(), Some("observe_only"));
    }

    #[tokio::test]
    async fn background_cycle_once_persists_history() {
        let database = db::Db::connect("sqlite::memory:").await.expect("db");
        db::ensure_mvp_seed(database.pool()).await.expect("seed");
        super::init_runtime_defaults(&database)
            .await
            .expect("init defaults");
        db::replace_symbol_allowlist(
            database.pool(),
            &[("AAPL.US".to_string(), true), ("MSFT.US".to_string(), true)],
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

        let mut ingest_registry = IngestRegistry::default();
        ingest_registry.register(Arc::new(MockBarsAdapter::paper_bars(Venue::UsEquity)));

        let report = super::run_background_universe_cycle_once(
            &database,
            &ingest_registry,
            &execution_router,
            &RankedTestStrategy,
            RiskLimits::default(),
            Venue::UsEquity,
            "acc_mvp_paper",
        )
        .await
        .expect("cycle")
        .expect("report");

        assert_eq!(report.accepted, vec!["AAPL.US", "MSFT.US"]);
        let history = pipeline::load_universe_cycle_history(&database, 10)
            .await
            .expect("history");
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].accepted, vec!["AAPL.US", "MSFT.US"]);
    }
}
