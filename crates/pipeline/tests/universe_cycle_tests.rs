use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use db::Db;
use domain::{Side, Signal, Venue};
use exec::{ExecutionAdapter, ExecutionRouter, PaperAdapter};
use ingest::{IngestRegistry, MockBarsAdapter};
use pipeline::{
    CycleDecision, RiskLimits, UniverseCandidate, UniverseCycleConfig, UniverseRunParams,
    load_universe_cycle_history, run_one_cycle_for_universe, run_universe_cycle,
};
use strategy::{ScoredCandidate, Strategy, StrategyContext};

#[test]
fn selects_high_confidence_top_ranked_symbols() {
    let config = UniverseCycleConfig {
        max_positions: 3,
        min_score: 0.5,
        min_confidence: 0.6,
    };
    let candidates = vec![
        UniverseCandidate {
            symbol: "A".to_string(),
            score: 0.9,
            confidence: 0.8,
        },
        UniverseCandidate {
            symbol: "D".to_string(),
            score: 0.7,
            confidence: 0.5,
        },
        UniverseCandidate {
            symbol: "B".to_string(),
            score: 0.6,
            confidence: 0.7,
        },
        UniverseCandidate {
            symbol: "C".to_string(),
            score: 0.4,
            confidence: 0.9,
        },
        UniverseCandidate {
            symbol: "E".to_string(),
            score: 0.55,
            confidence: 0.65,
        },
        UniverseCandidate {
            symbol: "F".to_string(),
            score: 0.85,
            confidence: 0.95,
        },
    ];

    let result = run_one_cycle_for_universe(&config, &candidates);
    assert_eq!(result.accepted, vec!["A", "F", "B"]);
    assert!(
        result
            .rejected
            .contains(&(String::from("D"), CycleDecision::ConfidenceBelowThreshold))
    );
    assert!(
        result
            .rejected
            .contains(&(String::from("C"), CycleDecision::ScoreBelowThreshold))
    );
    assert!(
        result
            .rejected
            .contains(&(String::from("E"), CycleDecision::MaxPositionsReached))
    );
}

struct ErrorCandidateStrategy {
    code: &'static str,
}

#[async_trait]
impl Strategy for ErrorCandidateStrategy {
    async fn evaluate(&self, _context: &StrategyContext) -> Option<Signal> {
        None
    }

    async fn evaluate_candidate(
        &self,
        _context: &StrategyContext,
    ) -> Result<Option<ScoredCandidate>, String> {
        Err(self.code.to_string())
    }
}

#[derive(Default)]
struct CountingStrategy {
    candidate_calls: Arc<AtomicUsize>,
    signal_calls: Arc<AtomicUsize>,
}

#[async_trait]
impl Strategy for CountingStrategy {
    async fn evaluate(&self, context: &StrategyContext) -> Option<Signal> {
        self.signal_calls.fetch_add(1, Ordering::SeqCst);
        Some(Signal {
            strategy_id: "counting".to_string(),
            instrument: context.instrument.clone(),
            instrument_db_id: context.instrument_db_id,
            side: Side::Buy,
            qty: 1.0,
            limit_price: context.last_bar_close?,
            ts_ms: context.ts_ms,
        })
    }

    async fn evaluate_candidate(
        &self,
        context: &StrategyContext,
    ) -> Result<Option<ScoredCandidate>, String> {
        self.candidate_calls.fetch_add(1, Ordering::SeqCst);
        Ok(Some(ScoredCandidate {
            symbol: context.instrument.symbol.clone(),
            score: 0.9,
            confidence: 0.9,
        }))
    }
}

async fn test_db_with_allowlist(symbols: &[&str], mode: &str) -> Db {
    let database = Db::connect("sqlite::memory:").await.expect("db");
    db::ensure_mvp_seed(database.pool()).await.expect("seed");
    let entries = symbols
        .iter()
        .map(|symbol| ((*symbol).to_string(), true))
        .collect::<Vec<_>>();
    db::replace_symbol_allowlist(database.pool(), &entries)
        .await
        .expect("allowlist");
    db::set_runtime_control(database.pool(), "mode", mode)
        .await
        .expect("mode");
    database
}

fn test_execution_router(database: &Db) -> ExecutionRouter {
    let paper = Arc::new(PaperAdapter::new(database.clone()));
    let mut routes = HashMap::new();
    routes.insert(
        "acc_mvp_paper".to_string(),
        paper as Arc<dyn ExecutionAdapter>,
    );
    ExecutionRouter::new(routes)
}

fn test_ingest_registry() -> IngestRegistry {
    let mut registry = IngestRegistry::default();
    registry.register(Arc::new(MockBarsAdapter::paper_bars(Venue::UsEquity)));
    registry
}

#[tokio::test]
async fn universe_cycle_persists_model_reason_code_in_skipped() {
    let database = test_db_with_allowlist(&["AAPL.US"], "paper_only").await;
    let registry = test_ingest_registry();
    let adapter = registry
        .adapter_for_venue(Venue::UsEquity)
        .expect("adapter");
    let strategy = ErrorCandidateStrategy {
        code: "model_not_found",
    };

    let report = run_universe_cycle(
        &database,
        adapter.as_ref(),
        &test_execution_router(&database),
        &strategy,
        RiskLimits::default(),
        &UniverseRunParams {
            account_id: "acc_mvp_paper".to_string(),
            venue: Venue::UsEquity,
            ts_ms: 1_700_000_000_000,
        },
    )
    .await
    .expect("cycle");

    assert!(report.skipped.iter().any(|item| {
        item.symbol == "AAPL.US" && item.reason == "model_not_found"
    }));

    let history = load_universe_cycle_history(&database, 10)
        .await
        .expect("history");
    assert_eq!(history.len(), 1);
    assert!(history[0].skipped.iter().any(|item| {
        item.symbol == "AAPL.US" && item.reason == "model_not_found"
    }));
}

#[tokio::test]
async fn universe_cycle_calls_candidate_and_signal_once_for_accepted_symbol() {
    let database = test_db_with_allowlist(&["AAPL.US"], "paper_only").await;
    let registry = test_ingest_registry();
    let adapter = registry
        .adapter_for_venue(Venue::UsEquity)
        .expect("adapter");
    let strategy = CountingStrategy::default();
    let candidate_calls = Arc::clone(&strategy.candidate_calls);
    let signal_calls = Arc::clone(&strategy.signal_calls);

    let report = run_universe_cycle(
        &database,
        adapter.as_ref(),
        &test_execution_router(&database),
        &strategy,
        RiskLimits::default(),
        &UniverseRunParams {
            account_id: "acc_mvp_paper".to_string(),
            venue: Venue::UsEquity,
            ts_ms: 1_700_000_000_000,
        },
    )
    .await
    .expect("cycle");

    assert_eq!(candidate_calls.load(Ordering::SeqCst), 1);
    assert_eq!(signal_calls.load(Ordering::SeqCst), 1);
    assert_eq!(report.accepted, vec!["AAPL.US"]);
    assert_eq!(report.placed.len(), 1);
}
