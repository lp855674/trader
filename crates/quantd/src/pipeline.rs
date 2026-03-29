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

/// One ingest → strategy → risk (allow) → paper order for a single venue/instrument.
pub async fn run_one_tick_for_venue(
    database: &db::Db,
    ingest_adapter: &dyn IngestAdapter,
    router: &exec::ExecutionRouter,
    account_id: &str,
    venue: Venue,
    symbol: &str,
    strategy: &dyn Strategy,
    ts_ms: i64,
) -> Result<(), PipelineError> {
    let pool = database.pool();
    let venue_str = venue.as_str();
    let iid = db::upsert_instrument(pool, venue_str, symbol).await?;

    ingest_adapter.ingest_once(database, iid).await?;

    let last = db::last_bar_close(pool, iid, ingest_adapter.data_source_id()).await?;

    let instrument = InstrumentId {
        venue,
        symbol: symbol.to_string(),
    };

    let context = StrategyContext {
        instrument: instrument.clone(),
        instrument_db_id: iid,
        last_bar_close: last,
        ts_ms,
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
        ts_ms,
    )
    .await?;

    let risk_id = Uuid::new_v4().to_string();
    db::insert_risk_decision(pool, &risk_id, &signal_id, true, Some("mvp_allow_all"), ts_ms)
        .await?;

    let intent = OrderIntent {
        strategy_id: signal.strategy_id.clone(),
        instrument,
        instrument_db_id: iid,
        side: signal.side,
        qty: signal.qty,
    };

    router.place_order(account_id, &intent, None).await?;

    Ok(())
}
