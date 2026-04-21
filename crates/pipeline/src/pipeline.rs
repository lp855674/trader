//! Ingest → strategy → risk → execution pipeline (shared by `quantd` and `api`).

mod execution_guard;
mod risk;
mod universe;

use domain::{InstrumentId, OrderIntent, Signal, Venue};
use exec::OrderAck;
use ingest::IngestAdapter;
use serde::{Deserialize, Serialize};
use strategy::{Strategy, StrategyContext};
use uuid::Uuid;

use execution_guard::{ExecutionGuardDecision, ExecutionGuardInput, evaluate_execution_guard};
pub use risk::RiskLimits;
pub use universe::{
    CycleDecision, UniverseCandidate, UniverseCycleConfig, UniverseCycleResult,
    run_one_cycle_for_universe,
};

const RUNTIME_MODE_KEY: &str = "mode";
const OBSERVE_ONLY_MODE: &str = "observe_only";
const LAST_CYCLE_KEY: &str = "runtime.last_cycle";

#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error(transparent)]
    Ingest(#[from] ingest::IngestError),
    #[error(transparent)]
    Db(#[from] db::DbError),
    #[error(transparent)]
    Exec(#[from] exec::ExecError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("risk denied: {0}")]
    RiskDenied(String),
    #[error("strategy error: {0}")]
    Strategy(String),
    #[error("strategy does not support universe scoring")]
    UnsupportedStrategy,
    #[error("symbol allowlist is empty")]
    EmptyAllowlist,
}

/// Per-venue tick: account, symbol, and timestamps (owned strings so HTTP/async callers stay `Send`).
#[derive(Clone)]
pub struct VenueTickParams {
    pub account_id: String,
    pub venue: Venue,
    pub symbol: String,
    pub ts_ms: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RankedSymbol {
    pub symbol: String,
    pub score: f64,
    pub confidence: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SymbolDecision {
    pub symbol: String,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PlacedOrder {
    pub symbol: String,
    pub order_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct UniverseCycleReport {
    pub mode: String,
    pub account_id: String,
    pub venue: String,
    pub triggered_at_ms: i64,
    pub ranked: Vec<RankedSymbol>,
    pub accepted: Vec<String>,
    pub rejected: Vec<SymbolDecision>,
    pub skipped: Vec<SymbolDecision>,
    pub placed: Vec<PlacedOrder>,
}

#[derive(Clone, Debug)]
pub struct UniverseRunParams {
    pub account_id: String,
    pub venue: Venue,
    pub ts_ms: i64,
}

struct EvaluatedSignal {
    instrument_id: i64,
    signal: Signal,
}

#[derive(Clone, Debug)]
struct UniverseRunnerConfig {
    max_positions: usize,
    min_score: f64,
    min_confidence: f64,
}

impl UniverseRunnerConfig {
    fn from_env() -> Self {
        let default = Self {
            max_positions: 3,
            min_score: 0.6,
            min_confidence: 0.6,
        };
        let max_positions = std::env::var("QUANTD_UNIVERSE_MAX_POSITIONS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(default.max_positions);
        let min_score = std::env::var("QUANTD_UNIVERSE_MIN_SCORE")
            .ok()
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|value| value.is_finite())
            .unwrap_or(default.min_score);
        let min_confidence = std::env::var("QUANTD_UNIVERSE_MIN_CONFIDENCE")
            .ok()
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|value| value.is_finite())
            .unwrap_or(default.min_confidence);
        Self {
            max_positions,
            min_score,
            min_confidence,
        }
    }
}

/// One ingest → strategy → risk → execution（限价） for a single venue/instrument.
/// Returns [`Some(OrderAck)`] when an order was placed; [`None`] if the strategy did not emit a signal.
pub async fn run_one_tick_for_venue(
    database: &db::Db,
    ingest_adapter: &dyn IngestAdapter,
    exec_router: &exec::ExecutionRouter,
    strategy: &dyn Strategy,
    risk_limits: RiskLimits,
    params: &VenueTickParams,
    idempotency_key: Option<&str>,
) -> Result<Option<OrderAck>, PipelineError> {
    let venue_str = params.venue.as_str();
    let Some(evaluated_signal) =
        evaluate_signal_for_tick(database, ingest_adapter, strategy, params).await?
    else {
        tracing::info!(
            channel = "pipeline",
            venue = venue_str,
            account_id = %params.account_id,
            "strategy skipped (no signal)"
        );
        return Ok(None);
    };
    execute_signal(
        database,
        exec_router,
        risk_limits,
        &params.account_id,
        params.venue,
        params.ts_ms,
        evaluated_signal,
        idempotency_key,
    )
    .await
}

pub async fn run_universe_cycle(
    database: &db::Db,
    ingest_adapter: &dyn IngestAdapter,
    exec_router: &exec::ExecutionRouter,
    strategy: &dyn Strategy,
    risk_limits: RiskLimits,
    params: &UniverseRunParams,
) -> Result<UniverseCycleReport, PipelineError> {
    let allowlist = db::list_symbol_allowlist(database.pool()).await?;
    let enabled_symbols: Vec<String> = allowlist
        .into_iter()
        .filter_map(|(symbol, enabled)| if enabled { Some(symbol) } else { None })
        .collect();
    if enabled_symbols.is_empty() {
        return Err(PipelineError::EmptyAllowlist);
    }

    let mode = db::get_runtime_control(database.pool(), RUNTIME_MODE_KEY)
        .await?
        .unwrap_or_else(|| OBSERVE_ONLY_MODE.to_string());

    let mut ranked = Vec::new();
    let mut skipped = Vec::new();

    for symbol in enabled_symbols {
        let instrument_id =
            db::upsert_instrument(database.pool(), params.venue.as_str(), &symbol).await?;
        ingest_adapter.ingest_once(database, instrument_id).await?;
        let last_bar_close = db::last_bar_close(
            database.pool(),
            instrument_id,
            ingest_adapter.data_source_id(),
        )
        .await?;
        let context = StrategyContext {
            instrument: InstrumentId::new(params.venue, symbol.as_str()),
            instrument_db_id: instrument_id,
            last_bar_close,
            ts_ms: params.ts_ms,
        };

        match strategy.evaluate_candidate(&context).await {
            Ok(Some(candidate)) => ranked.push(candidate),
            Ok(None) => skipped.push(SymbolDecision {
                symbol,
                reason: "no_candidate".to_string(),
            }),
            Err(error) if error == "strategy does not support universe scoring" => {
                return Err(PipelineError::UnsupportedStrategy);
            }
            Err(error) => skipped.push(SymbolDecision {
                symbol,
                reason: error,
            }),
        }
    }

    let cycle_config = UniverseRunnerConfig::from_env();
    let ranked_candidates: Vec<UniverseCandidate> = ranked
        .iter()
        .map(|candidate| UniverseCandidate {
            symbol: candidate.symbol.clone(),
            score: candidate.score,
            confidence: candidate.confidence,
        })
        .collect();
    let selection = run_one_cycle_for_universe(
        &UniverseCycleConfig {
            max_positions: cycle_config.max_positions,
            min_score: cycle_config.min_score,
            min_confidence: cycle_config.min_confidence,
        },
        &ranked_candidates,
    );

    let mut placed = Vec::new();
    if matches!(mode.as_str(), "enabled" | "paper_only") {
        for symbol in &selection.accepted {
            let tick = VenueTickParams {
                account_id: params.account_id.clone(),
                venue: params.venue,
                symbol: symbol.clone(),
                ts_ms: params.ts_ms,
            };
            match evaluate_signal_for_tick(database, ingest_adapter, strategy, &tick).await {
                Ok(Some(evaluated_signal)) => {
                    let guard_input = ExecutionGuardInput {
                        account_id: params.account_id.clone(),
                        instrument_id: evaluated_signal.instrument_id,
                        symbol: symbol.clone(),
                        side: evaluated_signal.signal.side,
                        strategy_id: evaluated_signal.signal.strategy_id.clone(),
                        ts_ms: params.ts_ms,
                    };
                    match evaluate_execution_guard(database, &guard_input).await {
                        Ok(ExecutionGuardDecision::Allow { idempotency_key }) => {
                            match execute_signal(
                                database,
                                exec_router,
                                risk_limits,
                                &params.account_id,
                                params.venue,
                                params.ts_ms,
                                evaluated_signal,
                                Some(idempotency_key.as_str()),
                            )
                            .await
                            {
                                Ok(Some(ack)) => placed.push(PlacedOrder {
                                    symbol: symbol.clone(),
                                    order_id: ack.order_id,
                                }),
                                Ok(None) => skipped.push(SymbolDecision {
                                    symbol: symbol.clone(),
                                    reason: "no_signal_on_execution".to_string(),
                                }),
                                Err(error) => skipped.push(SymbolDecision {
                                    symbol: symbol.clone(),
                                    reason: format!("execution_error:{error}"),
                                }),
                            }
                        }
                        Ok(ExecutionGuardDecision::Deny { reason }) => {
                            skipped.push(SymbolDecision {
                                symbol: guard_input.symbol,
                                reason,
                            });
                        }
                        Err(error) => skipped.push(SymbolDecision {
                            symbol: symbol.clone(),
                            reason: format!("execution_guard_error:{error}"),
                        }),
                    }
                }
                Ok(None) => skipped.push(SymbolDecision {
                    symbol: symbol.clone(),
                    reason: "no_signal_on_execution".to_string(),
                }),
                Err(error) => skipped.push(SymbolDecision {
                    symbol: symbol.clone(),
                    reason: error.to_string(),
                }),
            }
        }
    }

    let report = UniverseCycleReport {
        mode,
        account_id: params.account_id.clone(),
        venue: params.venue.as_str().to_string(),
        triggered_at_ms: params.ts_ms,
        ranked: ranked
            .into_iter()
            .map(|candidate| RankedSymbol {
                symbol: candidate.symbol,
                score: candidate.score,
                confidence: candidate.confidence,
            })
            .collect(),
        accepted: selection.accepted.clone(),
        rejected: selection
            .rejected
            .into_iter()
            .map(|(symbol, decision)| SymbolDecision {
                symbol,
                reason: decision.to_string(),
            })
            .collect(),
        skipped,
        placed,
    };
    store_last_universe_cycle(database, &report).await?;
    Ok(report)
}

pub async fn store_last_universe_cycle(
    database: &db::Db,
    report: &UniverseCycleReport,
) -> Result<(), PipelineError> {
    let value = serde_json::to_string(report)?;
    db::set_system_config(database.pool(), LAST_CYCLE_KEY, &value).await?;
    persist_universe_cycle_history(database, report).await?;
    Ok(())
}

pub async fn load_last_universe_cycle(
    database: &db::Db,
) -> Result<Option<UniverseCycleReport>, PipelineError> {
    let Some(value) = db::get_system_config(database.pool(), LAST_CYCLE_KEY).await? else {
        return Ok(None);
    };
    let report = serde_json::from_str(&value)?;
    Ok(Some(report))
}

pub async fn load_universe_cycle_history(
    database: &db::Db,
    limit: i64,
) -> Result<Vec<UniverseCycleReport>, PipelineError> {
    let runs = db::list_runtime_cycle_runs(database.pool(), limit).await?;
    let mut reports = Vec::with_capacity(runs.len());
    for run in runs {
        let symbols = db::list_runtime_cycle_symbols_for_run(database.pool(), &run.id).await?;
        reports.push(report_from_rows(run, symbols));
    }
    Ok(reports)
}

async fn persist_universe_cycle_history(
    database: &db::Db,
    report: &UniverseCycleReport,
) -> Result<(), PipelineError> {
    let run_id = Uuid::new_v4().to_string();
    let run = db::NewRuntimeCycleRun {
        id: &run_id,
        account_id: &report.account_id,
        venue: &report.venue,
        mode: &report.mode,
        triggered_at_ms: report.triggered_at_ms,
    };
    db::insert_runtime_cycle_run(database.pool(), &run).await?;

    let mut rows = Vec::new();
    for symbol in &report.ranked {
        rows.push(db::NewRuntimeCycleSymbol {
            run_id: &run_id,
            symbol: &symbol.symbol,
            score: Some(symbol.score),
            confidence: Some(symbol.confidence),
            decision: "ranked",
            reason: None,
            order_id: None,
        });
    }
    for symbol in &report.rejected {
        rows.push(db::NewRuntimeCycleSymbol {
            run_id: &run_id,
            symbol: &symbol.symbol,
            score: None,
            confidence: None,
            decision: "rejected",
            reason: Some(&symbol.reason),
            order_id: None,
        });
    }
    for symbol in &report.accepted {
        rows.push(db::NewRuntimeCycleSymbol {
            run_id: &run_id,
            symbol,
            score: None,
            confidence: None,
            decision: "accepted",
            reason: None,
            order_id: None,
        });
    }
    for symbol in &report.skipped {
        rows.push(db::NewRuntimeCycleSymbol {
            run_id: &run_id,
            symbol: &symbol.symbol,
            score: None,
            confidence: None,
            decision: "skipped",
            reason: Some(&symbol.reason),
            order_id: None,
        });
    }
    for symbol in &report.placed {
        rows.push(db::NewRuntimeCycleSymbol {
            run_id: &run_id,
            symbol: &symbol.symbol,
            score: None,
            confidence: None,
            decision: "placed",
            reason: None,
            order_id: Some(&symbol.order_id),
        });
    }
    db::insert_runtime_cycle_symbols(database.pool(), &rows).await?;
    Ok(())
}

fn report_from_rows(
    run: db::RuntimeCycleRunRow,
    rows: Vec<db::RuntimeCycleSymbolRow>,
) -> UniverseCycleReport {
    let mut ranked = Vec::new();
    let mut accepted = Vec::new();
    let mut rejected = Vec::new();
    let mut skipped = Vec::new();
    let mut placed = Vec::new();

    for row in rows {
        match row.decision.as_str() {
            "ranked" => ranked.push(RankedSymbol {
                symbol: row.symbol,
                score: row.score.unwrap_or_default(),
                confidence: row.confidence.unwrap_or_default(),
            }),
            "accepted" => accepted.push(row.symbol),
            "rejected" => rejected.push(SymbolDecision {
                symbol: row.symbol,
                reason: row.reason.unwrap_or_else(|| "rejected".to_string()),
            }),
            "skipped" => skipped.push(SymbolDecision {
                symbol: row.symbol,
                reason: row.reason.unwrap_or_else(|| "skipped".to_string()),
            }),
            "placed" => placed.push(PlacedOrder {
                symbol: row.symbol,
                order_id: row.order_id.unwrap_or_default(),
            }),
            _ => {}
        }
    }

    UniverseCycleReport {
        mode: run.mode,
        account_id: run.account_id,
        venue: run.venue,
        triggered_at_ms: run.triggered_at_ms,
        ranked,
        accepted,
        rejected,
        skipped,
        placed,
    }
}

async fn evaluate_signal_for_tick(
    database: &db::Db,
    ingest_adapter: &dyn IngestAdapter,
    strategy: &dyn Strategy,
    params: &VenueTickParams,
) -> Result<Option<EvaluatedSignal>, PipelineError> {
    let pool = database.pool();
    let venue_str = params.venue.as_str();
    let instrument_id = db::upsert_instrument(pool, venue_str, &params.symbol).await?;

    tracing::info!(
        channel = "pipeline",
        venue = venue_str,
        account_id = %params.account_id,
        data_source_id = ingest_adapter.data_source_id(),
        "ingest_once start"
    );

    ingest_adapter.ingest_once(database, instrument_id).await?;

    let last_bar_close =
        db::last_bar_close(pool, instrument_id, ingest_adapter.data_source_id()).await?;
    let context = StrategyContext {
        instrument: InstrumentId::new(params.venue, params.symbol.clone()),
        instrument_db_id: instrument_id,
        last_bar_close,
        ts_ms: params.ts_ms,
    };
    let signal = strategy.evaluate(&context).await;
    Ok(signal.map(|signal| EvaluatedSignal {
        instrument_id,
        signal,
    }))
}

async fn execute_signal(
    database: &db::Db,
    exec_router: &exec::ExecutionRouter,
    risk_limits: RiskLimits,
    account_id: &str,
    venue: Venue,
    ts_ms: i64,
    evaluated_signal: EvaluatedSignal,
    idempotency_key: Option<&str>,
) -> Result<Option<OrderAck>, PipelineError> {
    let pool = database.pool();
    let signal = evaluated_signal.signal;

    let signal_id = Uuid::new_v4().to_string();
    let payload = serde_json::to_string(&signal)?;
    db::insert_signal(
        pool,
        &signal_id,
        evaluated_signal.instrument_id,
        &signal.strategy_id,
        &payload,
        ts_ms,
    )
    .await?;

    if let Err(reason) = risk_limits.check(&signal) {
        let risk_id = Uuid::new_v4().to_string();
        db::insert_risk_decision(
            pool,
            &risk_id,
            &signal_id,
            false,
            Some(reason.as_str()),
            ts_ms,
        )
        .await?;
        tracing::warn!(
            channel = "pipeline",
            venue = venue.as_str(),
            account_id = %account_id,
            %reason,
            "risk denied"
        );
        return Err(PipelineError::RiskDenied(reason));
    }

    let risk_id = Uuid::new_v4().to_string();
    db::insert_risk_decision(
        pool,
        &risk_id,
        &signal_id,
        true,
        Some("within_limits"),
        ts_ms,
    )
    .await?;

    let intent = OrderIntent {
        strategy_id: signal.strategy_id.clone(),
        instrument: signal.instrument.clone(),
        instrument_db_id: signal.instrument_db_id,
        side: signal.side,
        qty: signal.qty,
        limit_price: signal.limit_price,
    };

    let ack = exec_router
        .place_order(account_id, &intent, idempotency_key)
        .await?;

    tracing::info!(
        channel = "pipeline",
        venue = venue.as_str(),
        account_id = %account_id,
        order_id = %ack.order_id,
        strategy_id = %signal.strategy_id,
        "order placed"
    );

    Ok(Some(ack))
}
