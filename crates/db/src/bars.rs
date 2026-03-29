use crate::error::DbError;
use sqlx::SqlitePool;

pub async fn insert_bar(
    pool: &SqlitePool,
    instrument_id: i64,
    data_source_id: &str,
    ts_ms: i64,
    o: f64,
    h: f64,
    l: f64,
    c: f64,
    volume: f64,
) -> Result<(), DbError> {
    sqlx::query(
        "INSERT INTO bars (instrument_id, data_source_id, ts_ms, o, h, l, c, volume)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(instrument_id, data_source_id, ts_ms) DO NOTHING",
    )
    .bind(instrument_id)
    .bind(data_source_id)
    .bind(ts_ms)
    .bind(o)
    .bind(h)
    .bind(l)
    .bind(c)
    .bind(volume)
    .execute(pool)
    .await?;
    Ok(())
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
