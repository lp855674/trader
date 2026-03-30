//! Ingest → strategy → risk → execution pipeline (shared by `quantd` and `api`).

mod risk;

use domain::{InstrumentId, OrderIntent, Venue};
use exec::OrderAck;
use ingest::IngestAdapter;
use strategy::{Strategy, StrategyContext};
use uuid::Uuid;

pub use risk::RiskLimits;

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
}

/// Per-venue tick: account, symbol, and timestamps (owned strings so HTTP/async callers stay `Send`).
#[derive(Clone)]
pub struct VenueTickParams {
    pub account_id: String,
    pub venue: Venue,
    pub symbol: String,
    pub ts_ms: i64,
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
) -> Result<Option<OrderAck>, PipelineError> {
    let pool = database.pool();
    let venue_str = params.venue.as_str();
    let iid = db::upsert_instrument(pool, venue_str, &params.symbol).await?;

    tracing::info!(
        channel = "pipeline",
        venue = venue_str,
        account_id = %params.account_id,
        data_source_id = ingest_adapter.data_source_id(),
        "ingest_once start"
    );

    ingest_adapter.ingest_once(database, iid).await?;

    let last = db::last_bar_close(pool, iid, ingest_adapter.data_source_id()).await?;

    let instrument = InstrumentId::new(params.venue, params.symbol.clone());

    let context = StrategyContext {
        instrument: instrument.clone(),
        instrument_db_id: iid,
        last_bar_close: last,
        ts_ms: params.ts_ms,
    };

    let Some(signal) = strategy.evaluate(&context) else {
        tracing::info!(
            channel = "pipeline",
            venue = venue_str,
            account_id = %params.account_id,
            "strategy skipped (no signal)"
        );
        return Ok(None);
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

    if let Err(reason) = risk_limits.check(&signal) {
        let risk_id = Uuid::new_v4().to_string();
        db::insert_risk_decision(
            pool,
            &risk_id,
            &signal_id,
            false,
            Some(reason.as_str()),
            params.ts_ms,
        )
        .await?;
        tracing::warn!(
            channel = "pipeline",
            venue = venue_str,
            account_id = %params.account_id,
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
        params.ts_ms,
    )
    .await?;

    let intent = OrderIntent {
        strategy_id: signal.strategy_id.clone(),
        instrument,
        instrument_db_id: iid,
        side: signal.side,
        qty: signal.qty,
        limit_price: signal.limit_price,
    };

    let ack = exec_router
        .place_order(&params.account_id, &intent, None)
        .await?;

    tracing::info!(
        channel = "pipeline",
        venue = venue_str,
        account_id = %params.account_id,
        order_id = %ack.order_id,
        strategy_id = %signal.strategy_id,
        "order placed"
    );

    Ok(Some(ack))
}
