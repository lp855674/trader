//! Market data ingest adapters.

mod adapter;
mod mock_bars;
mod registry;

pub use adapter::{IngestAdapter, IngestError};
pub use mock_bars::MockBarsAdapter;
pub use registry::IngestRegistry;

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use db::Db;
    use domain::Venue;

    use super::{IngestAdapter, MockBarsAdapter};

    #[tokio::test]
    async fn mock_ingest_inserts_bar() {
        let database = Db::connect("sqlite::memory:").await.expect("db");
        db::ensure_mvp_seed(database.pool()).await.expect("seed");
        let iid = db::upsert_instrument(database.pool(), Venue::UsEquity.as_str(), "TEST")
            .await
            .expect("instrument");
        let adapter = Arc::new(MockBarsAdapter::new(Venue::UsEquity, "mock_us"));
        adapter
            .ingest_once(&database, iid)
            .await
            .expect("ingest");
        let close = db::last_bar_close(database.pool(), iid, "mock_us")
            .await
            .expect("last");
        assert_eq!(close, Some(100.0));
    }
}
