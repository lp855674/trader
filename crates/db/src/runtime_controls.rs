use crate::error::DbError;
use sqlx::SqlitePool;

fn current_ts() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub async fn get_runtime_control(pool: &SqlitePool, key: &str) -> Result<Option<String>, DbError> {
    let val = sqlx::query_scalar("SELECT value FROM runtime_controls WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await?;
    Ok(val)
}

pub async fn set_runtime_control(pool: &SqlitePool, key: &str, value: &str) -> Result<(), DbError> {
    let now = current_ts();
    sqlx::query(
        "INSERT INTO runtime_controls (key, value, updated_at)
         VALUES (?, ?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(key)
    .bind(value)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_symbol_allowlist(pool: &SqlitePool) -> Result<Vec<(String, bool)>, DbError> {
    let rows = sqlx::query_as::<_, (String, i64)>(
        "SELECT symbol, enabled FROM symbol_allowlist ORDER BY symbol",
    )
    .fetch_all(pool)
    .await?;
    let converted = rows
        .into_iter()
        .map(|(symbol, enabled)| (symbol, enabled != 0))
        .collect();
    Ok(converted)
}

pub async fn replace_symbol_allowlist(
    pool: &SqlitePool,
    entries: &[(String, bool)],
) -> Result<(), DbError> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM symbol_allowlist")
        .execute(&mut *tx)
        .await?;
    let now = current_ts();
    for (symbol, enabled) in entries {
        sqlx::query("INSERT INTO symbol_allowlist (symbol, enabled, updated_at) VALUES (?, ?, ?)")
            .bind(symbol)
            .bind(*enabled as i64)
            .bind(now)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}
