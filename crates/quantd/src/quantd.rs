//! quantd library surface (re-exports for integration tests).

use std::collections::BTreeMap;

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

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct PositionSummary {
    pub symbol: String,
    pub net_qty: f64,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct OpenOrderSummary {
    pub symbol: String,
    pub count: i64,
}

#[derive(Clone, Debug)]
pub struct BrokerAccountSnapshot {
    pub cash: f64,
    pub positions: Vec<PositionSummary>,
    pub open_orders: Vec<OpenOrderSummary>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct PositionQtyMismatch {
    pub symbol: String,
    pub local_net_qty: f64,
    pub broker_net_qty: f64,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct OpenOrderCountMismatch {
    pub symbol: String,
    pub local_count: i64,
    pub broker_count: i64,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct ReconciliationDiff {
    pub missing_local_positions: Vec<PositionSummary>,
    pub missing_broker_positions: Vec<PositionSummary>,
    pub qty_mismatches: Vec<PositionQtyMismatch>,
    pub open_order_count_mismatches: Vec<OpenOrderCountMismatch>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReconciliationStatus {
    Ok,
    Mismatch,
    BrokerUnreachable,
}

impl ReconciliationStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Mismatch => "mismatch",
            Self::BrokerUnreachable => "broker_unreachable",
        }
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ReconciliationReport {
    pub account_id: String,
    pub status: ReconciliationStatus,
    pub mismatch_count: i64,
    pub broker_cash: f64,
    pub local_cash: f64,
    pub local_positions: Vec<PositionSummary>,
    pub broker_positions: Vec<PositionSummary>,
    pub local_open_orders: Vec<OpenOrderSummary>,
    pub broker_open_orders: Vec<OpenOrderSummary>,
    pub diff: ReconciliationDiff,
}

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
    let status = match status {
        "ok" => ReconciliationStatus::Ok,
        "broker_unreachable" | "broker_connect_failed" => ReconciliationStatus::BrokerUnreachable,
        _ => ReconciliationStatus::Mismatch,
    };
    let report = ReconciliationReport {
        account_id: account_id.to_string(),
        status,
        mismatch_count: 1,
        broker_cash: 0.0,
        local_cash: 0.0,
        local_positions: Vec::new(),
        broker_positions: Vec::new(),
        local_open_orders: Vec::new(),
        broker_open_orders: Vec::new(),
        diff: empty_diff(),
    };
    persist_reconciliation_report(database, &report).await
}

pub async fn run_reconciliation_once(
    database: &db::Db,
    account_id: &str,
    broker_snapshot: Option<&BrokerAccountSnapshot>,
) -> Result<ReconciliationReport, db::DbError> {
    let local_positions = load_local_position_summaries(database, account_id).await?;
    let local_open_orders = load_local_open_order_summaries(database, account_id).await?;
    let local_cash = 0.0;

    let report = match broker_snapshot {
        Some(broker_snapshot) => {
            let diff = diff_reconciliation(
                &local_positions,
                &broker_snapshot.positions,
                &local_open_orders,
                &broker_snapshot.open_orders,
            );
            let mismatch_count = count_mismatches(&diff);
            ReconciliationReport {
                account_id: account_id.to_string(),
                status: if mismatch_count == 0 {
                    ReconciliationStatus::Ok
                } else {
                    ReconciliationStatus::Mismatch
                },
                mismatch_count,
                broker_cash: broker_snapshot.cash,
                local_cash,
                local_positions,
                broker_positions: broker_snapshot.positions.clone(),
                local_open_orders,
                broker_open_orders: broker_snapshot.open_orders.clone(),
                diff,
            }
        }
        None => ReconciliationReport {
            account_id: account_id.to_string(),
            status: ReconciliationStatus::BrokerUnreachable,
            mismatch_count: 1,
            broker_cash: 0.0,
            local_cash,
            local_positions,
            broker_positions: Vec::new(),
            local_open_orders,
            broker_open_orders: Vec::new(),
            diff: empty_diff(),
        },
    };
    persist_reconciliation_report(database, &report).await?;
    Ok(report)
}

pub async fn reconcile_and_maybe_degrade(
    database: &db::Db,
    account_id: &str,
    broker_snapshot: Option<&BrokerAccountSnapshot>,
) -> Result<ReconciliationReport, db::DbError> {
    let report = run_reconciliation_once(database, account_id, broker_snapshot).await?;
    if report.status != ReconciliationStatus::Ok {
        set_runtime_mode(database, OBSERVE_ONLY_MODE).await?;
    }
    Ok(report)
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

async fn persist_reconciliation_report(
    database: &db::Db,
    report: &ReconciliationReport,
) -> Result<(), db::DbError> {
    let snapshot_id = Uuid::new_v4().to_string();
    let broker_positions_json =
        serde_json::to_string(&report.broker_positions).unwrap_or_else(|_| "[]".to_string());
    let local_positions_json =
        serde_json::to_string(&report.local_positions).unwrap_or_else(|_| "[]".to_string());
    let snapshot = ReconciliationSnapshot {
        id: &snapshot_id,
        account_id: &report.account_id,
        broker_cash: report.broker_cash,
        local_cash: report.local_cash,
        broker_positions_json: &broker_positions_json,
        local_positions_json: &local_positions_json,
        mismatch_count: report.mismatch_count,
        status: report.status.as_str(),
    };
    db::insert_reconciliation_snapshot(database.pool(), &snapshot).await
}

async fn load_local_position_summaries(
    database: &db::Db,
    account_id: &str,
) -> Result<Vec<PositionSummary>, db::DbError> {
    db::list_local_positions_for_account(database.pool(), account_id)
        .await
        .map(|rows| {
            rows.into_iter()
                .map(|row| PositionSummary {
                    symbol: row.symbol,
                    net_qty: row.net_qty,
                })
                .collect()
        })
}

async fn load_local_open_order_summaries(
    database: &db::Db,
    account_id: &str,
) -> Result<Vec<OpenOrderSummary>, db::DbError> {
    let rows = db::list_open_orders_for_account(database.pool(), account_id).await?;
    let mut counts = BTreeMap::<String, i64>::new();
    for row in rows {
        *counts.entry(row.symbol).or_default() += 1;
    }
    Ok(counts
        .into_iter()
        .map(|(symbol, count)| OpenOrderSummary { symbol, count })
        .collect())
}

fn diff_reconciliation(
    local_positions: &[PositionSummary],
    broker_positions: &[PositionSummary],
    local_open_orders: &[OpenOrderSummary],
    broker_open_orders: &[OpenOrderSummary],
) -> ReconciliationDiff {
    let local_positions_map = local_positions
        .iter()
        .map(|position| (position.symbol.clone(), position.net_qty))
        .collect::<BTreeMap<_, _>>();
    let broker_positions_map = broker_positions
        .iter()
        .map(|position| (position.symbol.clone(), position.net_qty))
        .collect::<BTreeMap<_, _>>();
    let local_open_orders_map = local_open_orders
        .iter()
        .map(|order| (order.symbol.clone(), order.count))
        .collect::<BTreeMap<_, _>>();
    let broker_open_orders_map = broker_open_orders
        .iter()
        .map(|order| (order.symbol.clone(), order.count))
        .collect::<BTreeMap<_, _>>();

    let mut missing_local_positions = Vec::new();
    let mut missing_broker_positions = Vec::new();
    let mut qty_mismatches = Vec::new();
    let mut open_order_count_mismatches = Vec::new();

    for (symbol, broker_net_qty) in &broker_positions_map {
        match local_positions_map.get(symbol) {
            Some(local_net_qty) if (local_net_qty - broker_net_qty).abs() > 0.000001 => {
                qty_mismatches.push(PositionQtyMismatch {
                    symbol: symbol.clone(),
                    local_net_qty: *local_net_qty,
                    broker_net_qty: *broker_net_qty,
                });
            }
            None => missing_local_positions.push(PositionSummary {
                symbol: symbol.clone(),
                net_qty: *broker_net_qty,
            }),
            _ => {}
        }
    }

    for (symbol, local_net_qty) in &local_positions_map {
        if !broker_positions_map.contains_key(symbol) {
            missing_broker_positions.push(PositionSummary {
                symbol: symbol.clone(),
                net_qty: *local_net_qty,
            });
        }
    }

    for symbol in local_open_orders_map
        .keys()
        .chain(broker_open_orders_map.keys())
        .collect::<std::collections::BTreeSet<_>>()
    {
        let local_count = *local_open_orders_map.get(symbol.as_str()).unwrap_or(&0);
        let broker_count = *broker_open_orders_map.get(symbol.as_str()).unwrap_or(&0);
        if local_count != broker_count {
            open_order_count_mismatches.push(OpenOrderCountMismatch {
                symbol: (*symbol).clone(),
                local_count,
                broker_count,
            });
        }
    }

    ReconciliationDiff {
        missing_local_positions,
        missing_broker_positions,
        qty_mismatches,
        open_order_count_mismatches,
    }
}

fn count_mismatches(diff: &ReconciliationDiff) -> i64 {
    (diff.missing_local_positions.len()
        + diff.missing_broker_positions.len()
        + diff.qty_mismatches.len()
        + diff.open_order_count_mismatches.len()) as i64
}

fn empty_diff() -> ReconciliationDiff {
    ReconciliationDiff {
        missing_local_positions: Vec::new(),
        missing_broker_positions: Vec::new(),
        qty_mismatches: Vec::new(),
        open_order_count_mismatches: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use crate::{BrokerAccountSnapshot, PositionSummary, ReconciliationStatus};

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

    #[tokio::test]
    async fn reconciliation_ok_persists_snapshot_without_degrading() {
        let database = db::Db::connect("sqlite::memory:").await.expect("db");
        db::ensure_mvp_seed(database.pool()).await.expect("seed");
        super::init_runtime_defaults(&database)
            .await
            .expect("init defaults");
        db::set_runtime_control(database.pool(), "mode", "paper_only")
            .await
            .expect("mode");
        let instrument_id =
            db::upsert_instrument(database.pool(), Venue::UsEquity.as_str(), "AAPL.US")
                .await
                .expect("instrument");
        db::insert_order(
            database.pool(),
            &db::NewOrder {
                order_id: "filled-order",
                account_id: "acc_mvp_paper",
                instrument_id,
                side: "buy",
                qty: 2.0,
                status: "FILLED",
                order_type: "limit",
                limit_price: Some(100.0),
                exchange_ref: Some("paper-filled-order"),
                idempotency_key: Some("filled-order-key"),
                created_at_ms: 10,
                updated_at_ms: 10,
            },
        )
        .await
        .expect("order");
        db::insert_fill(
            database.pool(),
            &db::NewFill {
                fill_id: "fill-1",
                order_id: "filled-order",
                qty: 2.0,
                price: 100.0,
                created_at_ms: 11,
            },
        )
        .await
        .expect("fill");

        let broker = BrokerAccountSnapshot {
            cash: 0.0,
            positions: vec![PositionSummary {
                symbol: "AAPL.US".to_string(),
                net_qty: 2.0,
            }],
            open_orders: Vec::new(),
        };
        let report = super::reconcile_and_maybe_degrade(&database, "acc_mvp_paper", Some(&broker))
            .await
            .expect("report");
        assert_eq!(report.status, ReconciliationStatus::Ok);
        let mode = db::get_runtime_control(database.pool(), "mode")
            .await
            .expect("mode");
        assert_eq!(mode.as_deref(), Some("paper_only"));
        let snapshot = db::load_latest_reconciliation_snapshot(database.pool(), "acc_mvp_paper")
            .await
            .expect("snapshot")
            .expect("row");
        assert_eq!(snapshot.status, "ok");
    }

    #[tokio::test]
    async fn reconciliation_mismatch_degrades_to_observe_only() {
        let database = db::Db::connect("sqlite::memory:").await.expect("db");
        db::ensure_mvp_seed(database.pool()).await.expect("seed");
        super::init_runtime_defaults(&database)
            .await
            .expect("init defaults");
        db::set_runtime_control(database.pool(), "mode", "enabled")
            .await
            .expect("mode");
        let instrument_id =
            db::upsert_instrument(database.pool(), Venue::UsEquity.as_str(), "AAPL.US")
                .await
                .expect("instrument");
        db::insert_order(
            database.pool(),
            &db::NewOrder {
                order_id: "filled-order",
                account_id: "acc_mvp_paper",
                instrument_id,
                side: "buy",
                qty: 2.0,
                status: "FILLED",
                order_type: "limit",
                limit_price: Some(100.0),
                exchange_ref: Some("paper-filled-order"),
                idempotency_key: Some("filled-order-key"),
                created_at_ms: 10,
                updated_at_ms: 10,
            },
        )
        .await
        .expect("order");
        db::insert_fill(
            database.pool(),
            &db::NewFill {
                fill_id: "fill-1",
                order_id: "filled-order",
                qty: 2.0,
                price: 100.0,
                created_at_ms: 11,
            },
        )
        .await
        .expect("fill");

        let broker = BrokerAccountSnapshot {
            cash: 0.0,
            positions: vec![PositionSummary {
                symbol: "AAPL.US".to_string(),
                net_qty: 1.0,
            }],
            open_orders: Vec::new(),
        };
        let report = super::reconcile_and_maybe_degrade(&database, "acc_mvp_paper", Some(&broker))
            .await
            .expect("report");
        assert_eq!(report.status, ReconciliationStatus::Mismatch);
        assert_eq!(report.mismatch_count, 1);
        let mode = db::get_runtime_control(database.pool(), "mode")
            .await
            .expect("mode");
        assert_eq!(mode.as_deref(), Some("observe_only"));
    }

    #[tokio::test]
    async fn reconciliation_broker_unreachable_degrades_to_observe_only() {
        let database = db::Db::connect("sqlite::memory:").await.expect("db");
        db::ensure_mvp_seed(database.pool()).await.expect("seed");
        super::init_runtime_defaults(&database)
            .await
            .expect("init defaults");
        db::set_runtime_control(database.pool(), "mode", "enabled")
            .await
            .expect("mode");

        let report = super::reconcile_and_maybe_degrade(&database, "acc_mvp_paper", None)
            .await
            .expect("report");
        assert_eq!(report.status, ReconciliationStatus::BrokerUnreachable);
        let mode = db::get_runtime_control(database.pool(), "mode")
            .await
            .expect("mode");
        assert_eq!(mode.as_deref(), Some("observe_only"));
    }
}
