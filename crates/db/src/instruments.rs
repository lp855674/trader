use crate::error::DbError;
use sqlx::SqlitePool;

#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct InstrumentRow {
    pub id: i64,
    pub venue: String,
    pub symbol: String,
}

pub async fn upsert_instrument(
    pool: &SqlitePool,
    venue: &str,
    symbol: &str,
) -> Result<i64, DbError> {
    let r = sqlx::query_scalar::<_, i64>(
        "SELECT id FROM instruments WHERE venue = ? AND symbol = ?",
    )
    .bind(venue)
    .bind(symbol)
    .fetch_optional(pool)
    .await?;

    if let Some(id) = r {
        return Ok(id);
    }

    sqlx::query("INSERT INTO instruments (venue, symbol) VALUES (?, ?)")
        .bind(venue)
        .bind(symbol)
        .execute(pool)
        .await?;

    let id = sqlx::query_scalar::<_, i64>(
        "SELECT id FROM instruments WHERE venue = ? AND symbol = ?",
    )
    .bind(venue)
    .bind(symbol)
    .fetch_one(pool)
    .await?;

    Ok(id)
}

pub async fn list_instruments(pool: &SqlitePool) -> Result<Vec<InstrumentRow>, DbError> {
    sqlx::query_as::<_, InstrumentRow>("SELECT id, venue, symbol FROM instruments ORDER BY id")
        .fetch_all(pool)
        .await
        .map_err(Into::into)
}
