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

#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct ReconciliationSnapshotRow {
    pub id: String,
    pub account_id: String,
    pub broker_cash: f64,
    pub local_cash: f64,
    pub broker_positions_json: String,
    pub local_positions_json: String,
    pub mismatch_count: i64,
    pub status: String,
    pub created_at: i64,
}

pub async fn load_latest_reconciliation_snapshot(
    pool: &SqlitePool,
    account_id: &str,
) -> Result<Option<ReconciliationSnapshotRow>, DbError> {
    sqlx::query_as::<_, ReconciliationSnapshotRow>(
        "SELECT
            id,
            account_id,
            broker_cash,
            local_cash,
            broker_positions_json,
            local_positions_json,
            mismatch_count,
            status,
            created_at
         FROM reconciliation_snapshots
         WHERE account_id = ?
         ORDER BY created_at DESC
         LIMIT 1",
    )
    .bind(account_id)
    .fetch_optional(pool)
    .await
    .map_err(Into::into)
}
