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

    use super::{IngestAdapter, IngestRegistry, MockBarsAdapter};

    #[tokio::test]
    async fn mock_ingest_inserts_one_bar_row() {
        let database = Db::connect("sqlite::memory:").await.expect("db");
        db::ensure_mvp_seed(database.pool()).await.expect("seed");
        let iid = db::upsert_instrument(database.pool(), Venue::UsEquity.as_str(), "TEST")
            .await
            .expect("instrument");
        let adapter = Arc::new(MockBarsAdapter::paper_bars(Venue::UsEquity));
        adapter.ingest_once(&database, iid).await.expect("ingest");
        let n = db::count_bars_for_source(database.pool(), iid, db::PAPER_BARS_DATA_SOURCE_ID)
            .await
            .expect("count");
        assert_eq!(n, 1, "expected exactly one bar row (UNIQUE per ts/source)");
        let close = db::last_bar_close(database.pool(), iid, db::PAPER_BARS_DATA_SOURCE_ID)
            .await
            .expect("last");
        assert_eq!(close, Some(100.0));
    }

    #[tokio::test]
    async fn repeated_mock_ingest_accumulates_bars() {
        let database = Db::connect("sqlite::memory:").await.expect("db");
        db::ensure_mvp_seed(database.pool()).await.expect("seed");
        let iid = db::upsert_instrument(database.pool(), Venue::UsEquity.as_str(), "TEST")
            .await
            .expect("instrument");
        let adapter = Arc::new(MockBarsAdapter::paper_bars(Venue::UsEquity));

        adapter
            .ingest_once(&database, iid)
            .await
            .expect("first ingest");
        adapter
            .ingest_once(&database, iid)
            .await
            .expect("second ingest");

        let rows = db::get_recent_bars(database.pool(), iid, db::PAPER_BARS_DATA_SOURCE_ID, 10)
            .await
            .expect("rows");
        assert_eq!(rows.len(), 2);
        assert!(rows[1].ts_ms > rows[0].ts_ms);
    }

    #[test]
    fn registry_for_venue_filters_adapters() {
        let mut registry = IngestRegistry::default();
        registry.register(Arc::new(MockBarsAdapter::paper_bars(Venue::UsEquity)));
        registry.register(Arc::new(MockBarsAdapter::paper_bars(Venue::Crypto)));
        assert_eq!(registry.for_venue(Venue::UsEquity).count(), 1);
        assert_eq!(registry.for_venue(Venue::Crypto).count(), 1);
        assert_eq!(registry.for_venue(Venue::HkEquity).count(), 0);
        assert!(registry.adapter_for_venue(Venue::Crypto).is_some());
    }
}
