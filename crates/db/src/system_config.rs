use crate::error::DbError;
use sqlx::SqlitePool;

pub async fn get_system_config(pool: &SqlitePool, key: &str) -> Result<Option<String>, DbError> {
    let val = sqlx::query_scalar::<_, String>(
        "SELECT value FROM system_config WHERE key = ?",
    )
    .bind(key)
    .fetch_optional(pool)
    .await?;
    Ok(val)
}

pub async fn set_system_config(pool: &SqlitePool, key: &str, value: &str) -> Result<(), DbError> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    sqlx::query(
        "INSERT INTO system_config (id, key, value, updated_at, created_at)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(key)
    .bind(key)
    .bind(value)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}
