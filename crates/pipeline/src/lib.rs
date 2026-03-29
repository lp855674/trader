//! Ingest → strategy → risk → execution pipeline (shared by `quantd` and `api`).

use domain::{InstrumentId, OrderIntent, Venue};
use ingest::IngestAdapter;
use strategy::{Strategy, StrategyContext};
use uuid::Uuid;

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
}

/// Per-venue tick: account, symbol, and timestamps (owned strings so HTTP/async callers stay `Send`).
#[derive(Clone)]
pub struct VenueTickParams {
    pub account_id: String,
    pub venue: Venue,
    pub symbol: String,
    pub ts_ms: i64,
}

/// One ingest → strategy → risk (allow) → paper order for a single venue/instrument.
pub async fn run_one_tick_for_venue(
    database: &db::Db,
    ingest_adapter: &dyn IngestAdapter,
    exec_router: &exec::ExecutionRouter,
    strategy: &dyn Strategy,
    params: &VenueTickParams,
) -> Result<(), PipelineError> {
    let pool = database.pool();
    let venue_str = params.venue.as_str();
    let iid = db::upsert_instrument(pool, venue_str, &params.symbol).await?;

    ingest_adapter.ingest_once(database, iid).await?;

    let last = db::last_bar_close(pool, iid, ingest_adapter.data_source_id()).await?;

    let instrument = InstrumentId {
        venue: params.venue,
        symbol: params.symbol.clone(),
    };

    let context = StrategyContext {
        instrument: instrument.clone(),
        instrument_db_id: iid,
        last_bar_close: last,
        ts_ms: params.ts_ms,
    };

    let Some(signal) = strategy.evaluate(&context) else {
        return Ok(());
    };

    let signal_id = Uuid::new_v4().to_string();
    let payload = serde_json::to_string(&signal)?;
    db::insert_signal(
        pool,
        &signal_id,
        iid,
        &signal.strategy_id,
        &payload,
        params.ts_ms,
    )
    .await?;

    let risk_id = Uuid::new_v4().to_string();
    db::insert_risk_decision(
        pool,
        &risk_id,
        &signal_id,
        true,
        Some("mvp_allow_all"),
        params.ts_ms,
    )
    .await?;

    let intent = OrderIntent {
        strategy_id: signal.strategy_id.clone(),
        instrument,
        instrument_db_id: iid,
        side: signal.side,
        qty: signal.qty,
    };

    exec_router
        .place_order(&params.account_id, &intent, None)
        .await?;

    Ok(())
}
