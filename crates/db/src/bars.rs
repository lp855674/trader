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

/// OHLCV bar row returned from DB.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct BarRow {
    pub ts_ms: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

/// Return the most recent `limit` bars for an instrument/source, ordered oldest-first.
pub async fn get_recent_bars(
    pool: &SqlitePool,
    instrument_id: i64,
    data_source_id: &str,
    limit: i64,
) -> Result<Vec<BarRow>, DbError> {
    let rows = sqlx::query_as::<_, BarRow>(
        r#"SELECT ts_ms, o AS open, h AS high, l AS low, c AS close, volume
           FROM (
             SELECT ts_ms, o, h, l, c, volume
             FROM bars
             WHERE instrument_id = ? AND data_source_id = ?
             ORDER BY ts_ms DESC
             LIMIT ?
           ) ORDER BY ts_ms ASC"#,
    )
    .bind(instrument_id)
    .bind(data_source_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

#[cfg(test)]
mod bars_tests {
    use super::*;
    use crate::Db;

    #[tokio::test]
    async fn get_recent_bars_returns_ordered_rows() {
        let db = Db::connect("sqlite::memory:").await.unwrap();
        // data_sources FK required by bars table
        sqlx::query("INSERT OR IGNORE INTO data_sources (id, kind, config_json) VALUES ('test', 'test', NULL)")
            .execute(db.pool())
            .await
            .unwrap();
        let iid = crate::upsert_instrument(db.pool(), "US_EQUITY", "AAPL").await.unwrap();
        for i in 0..5_i64 {
            insert_bar(db.pool(), &NewBar {
                instrument_id: iid,
                data_source_id: "test",
                ts_ms: 1000 + i * 1000,
                open: 1.0, high: 2.0, low: 0.5, close: 1.5 + i as f64 * 0.1,
                volume: 100.0,
            }).await.unwrap();
        }
        let rows = get_recent_bars(db.pool(), iid, "test", 3).await.unwrap();
        assert_eq!(rows.len(), 3);
        assert!(rows[0].ts_ms < rows[1].ts_ms);
        assert!(rows[1].ts_ms < rows[2].ts_ms);
    }
}
