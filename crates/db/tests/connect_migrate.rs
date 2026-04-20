//! Task 3: connect + migrate from a separate test crate (integration-style).

#[tokio::test]
async fn migrate_runs_clean() {
    let db = db::Db::connect("sqlite::memory:").await.expect("migrate");
    let pool = db.pool();
    let tables: Vec<(String,)> = sqlx::query_as(
        "SELECT name FROM sqlite_master WHERE type='table' AND name IN (?, ?, ?, ?, ?)",
    )
    .bind("runtime_controls")
    .bind("symbol_allowlist")
    .bind("reconciliation_snapshots")
    .bind("runtime_cycle_runs")
    .bind("runtime_cycle_symbols")
    .fetch_all(pool)
    .await
    .expect("query tables");
    assert_eq!(
        tables.len(),
        5,
        "expected runtime tables to exist after migration"
    );
    drop(db);
}

#[test]
fn sqlite_memory_uses_single_connection_pool() {
    assert_eq!(db::sqlite_max_connections("sqlite::memory:"), 1);
    assert_eq!(db::sqlite_max_connections("sqlite::memory:?cache=shared"), 1);
    assert_eq!(db::sqlite_max_connections("sqlite:quantd.db"), 5);
}
