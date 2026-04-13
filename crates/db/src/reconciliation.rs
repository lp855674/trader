use crate::error::DbError;
use sqlx::SqlitePool;

pub struct ReconciliationSnapshot<'a> {
    pub id: &'a str,
    pub account_id: &'a str,
    pub broker_cash: f64,
    pub local_cash: f64,
    pub broker_positions_json: &'a str,
    pub local_positions_json: &'a str,
    pub mismatch_count: i64,
    pub status: &'a str,
}

pub async fn insert_reconciliation_snapshot(
    pool: &SqlitePool,
    snapshot: &ReconciliationSnapshot<'_>,
) -> Result<(), DbError> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    sqlx::query(
        "INSERT INTO reconciliation_snapshots (
            id,
            account_id,
            broker_cash,
            local_cash,
            broker_positions_json,
            local_positions_json,
            mismatch_count,
            status,
            created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(snapshot.id)
    .bind(snapshot.account_id)
    .bind(snapshot.broker_cash)
    .bind(snapshot.local_cash)
    .bind(snapshot.broker_positions_json)
    .bind(snapshot.local_positions_json)
    .bind(snapshot.mismatch_count)
    .bind(snapshot.status)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}
