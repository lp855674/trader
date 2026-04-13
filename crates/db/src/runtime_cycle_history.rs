use crate::error::DbError;
use sqlx::SqlitePool;

#[derive(Clone, Debug)]
pub struct NewRuntimeCycleRun<'a> {
    pub id: &'a str,
    pub account_id: &'a str,
    pub venue: &'a str,
    pub mode: &'a str,
    pub triggered_at_ms: i64,
}

#[derive(Clone, Debug)]
pub struct NewRuntimeCycleSymbol<'a> {
    pub run_id: &'a str,
    pub symbol: &'a str,
    pub score: Option<f64>,
    pub confidence: Option<f64>,
    pub decision: &'a str,
    pub reason: Option<&'a str>,
    pub order_id: Option<&'a str>,
}

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct RuntimeCycleRunRow {
    pub id: String,
    pub account_id: String,
    pub venue: String,
    pub mode: String,
    pub triggered_at_ms: i64,
}

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct RuntimeCycleSymbolRow {
    pub run_id: String,
    pub symbol: String,
    pub score: Option<f64>,
    pub confidence: Option<f64>,
    pub decision: String,
    pub reason: Option<String>,
    pub order_id: Option<String>,
}

pub async fn insert_runtime_cycle_run(
    pool: &SqlitePool,
    run: &NewRuntimeCycleRun<'_>,
) -> Result<(), DbError> {
    sqlx::query(
        "INSERT INTO runtime_cycle_runs (id, account_id, venue, mode, triggered_at_ms)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(run.id)
    .bind(run.account_id)
    .bind(run.venue)
    .bind(run.mode)
    .bind(run.triggered_at_ms)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn insert_runtime_cycle_symbols(
    pool: &SqlitePool,
    rows: &[NewRuntimeCycleSymbol<'_>],
) -> Result<(), DbError> {
    let mut tx = pool.begin().await?;
    for row in rows {
        sqlx::query(
            "INSERT INTO runtime_cycle_symbols (
                run_id,
                symbol,
                score,
                confidence,
                decision,
                reason,
                order_id
            ) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(row.run_id)
        .bind(row.symbol)
        .bind(row.score)
        .bind(row.confidence)
        .bind(row.decision)
        .bind(row.reason)
        .bind(row.order_id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn list_runtime_cycle_runs(
    pool: &SqlitePool,
    limit: i64,
) -> Result<Vec<RuntimeCycleRunRow>, DbError> {
    let rows = sqlx::query_as::<_, RuntimeCycleRunRow>(
        "SELECT id, account_id, venue, mode, triggered_at_ms
         FROM runtime_cycle_runs
         ORDER BY triggered_at_ms DESC
         LIMIT ?",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn list_runtime_cycle_symbols_for_run(
    pool: &SqlitePool,
    run_id: &str,
) -> Result<Vec<RuntimeCycleSymbolRow>, DbError> {
    let rows = sqlx::query_as::<_, RuntimeCycleSymbolRow>(
        "SELECT run_id, symbol, score, confidence, decision, reason, order_id
         FROM runtime_cycle_symbols
         WHERE run_id = ?
         ORDER BY id ASC",
    )
    .bind(run_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
