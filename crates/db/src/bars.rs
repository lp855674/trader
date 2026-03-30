use crate::error::DbError;
use sqlx::SqlitePool;

/// OHLCV row to insert (conflict on instrument + source + ts ignored).
pub struct NewBar<'a> {
    pub instrument_id: i64,
    pub data_source_id: &'a str,
    pub ts_ms: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

pub async fn insert_bar(pool: &SqlitePool, bar: &NewBar<'_>) -> Result<(), DbError> {
    sqlx::query(
        "INSERT INTO bars (instrument_id, data_source_id, ts_ms, o, h, l, c, volume)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(instrument_id, data_source_id, ts_ms) DO NOTHING",
    )
    .bind(bar.instrument_id)
    .bind(bar.data_source_id)
    .bind(bar.ts_ms)
    .bind(bar.open)
    .bind(bar.high)
    .bind(bar.low)
    .bind(bar.close)
    .bind(bar.volume)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn count_bars_for_source(
    pool: &SqlitePool,
    instrument_id: i64,
    data_source_id: &str,
) -> Result<i64, DbError> {
    let n = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM bars WHERE instrument_id = ? AND data_source_id = ?",
    )
    .bind(instrument_id)
    .bind(data_source_id)
    .fetch_one(pool)
    .await?;
    Ok(n)
}

pub async fn last_bar_close(
    pool: &SqlitePool,
    instrument_id: i64,
    data_source_id: &str,
) -> Result<Option<f64>, DbError> {
    let row = sqlx::query_scalar::<_, f64>(
        "SELECT c FROM bars WHERE instrument_id = ? AND data_source_id = ? ORDER BY ts_ms DESC LIMIT 1",
    )
    .bind(instrument_id)
    .bind(data_source_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}
