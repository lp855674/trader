//! Task 3: connect + migrate from a separate test crate (integration-style).

#[tokio::test]
async fn migrate_runs_clean() {
    let db = db::Db::connect("sqlite::memory:").await.expect("migrate");
    drop(db);
}
