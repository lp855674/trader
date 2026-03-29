use crate::error::DbError;
use sqlx::SqlitePool;

pub async fn insert_signal(
    pool: &SqlitePool,
    signal_id: &str,
    instrument_id: i64,
    strategy_id: &str,
    payload_json: &str,
    created_at_ms: i64,
) -> Result<(), DbError> {
    sqlx::query(
        "INSERT INTO signals (id, instrument_id, strategy_id, payload_json, created_at_ms)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(signal_id)
    .bind(instrument_id)
    .bind(strategy_id)
    .bind(payload_json)
    .bind(created_at_ms)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn insert_risk_decision(
    pool: &SqlitePool,
    decision_id: &str,
    signal_id: &str,
    allow: bool,
    reason: Option<&str>,
    created_at_ms: i64,
) -> Result<(), DbError> {
    sqlx::query(
        "INSERT INTO risk_decisions (id, signal_id, allow, reason, created_at_ms)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(decision_id)
    .bind(signal_id)
    .bind(if allow { 1i32 } else { 0 })
    .bind(reason)
    .bind(created_at_ms)
    .execute(pool)
    .await?;
    Ok(())
}
