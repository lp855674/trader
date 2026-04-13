//! Order execution adapters and routing.

mod adapter;
mod error;
mod live_stub;
mod paper;
mod router;

pub mod adapters;
pub mod api;
pub mod config;
pub mod core;
pub mod monitor;
pub mod orders;
pub mod persistence;
pub mod quality;
pub mod queue;
pub mod report;
pub mod system;
pub mod trading;

pub use adapter::{ExecutionAdapter, OrderAck};
pub use error::ExecError;
pub use live_stub::LiveStubAdapter;
pub use paper::PaperAdapter;
pub use router::ExecutionRouter;

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use domain::{InstrumentId, OrderIntent, Side, Venue};

    use super::{ExecutionRouter, PaperAdapter};
    use crate::adapter::ExecutionAdapter;

    #[tokio::test]
    async fn paper_places_order_and_fill() {
        let database = db::Db::connect("sqlite::memory:").await.expect("db");
        db::ensure_mvp_seed(database.pool()).await.expect("seed");
        db::ensure_account(database.pool(), "acc_test", "paper", "paper")
            .await
            .expect("account seed");
        let iid = db::upsert_instrument(database.pool(), Venue::UsEquity.as_str(), "T")
            .await
            .expect("instrument");

        let paper = Arc::new(PaperAdapter::new(database.clone()));
        let mut routes = HashMap::new();
        routes.insert("acc_test".to_string(), paper as Arc<dyn ExecutionAdapter>);
        let router = ExecutionRouter::new(routes);

        let intent = OrderIntent {
            strategy_id: "s1".to_string(),
            instrument: InstrumentId::new(Venue::UsEquity, "T"),
            instrument_db_id: iid,
            side: Side::Buy,
            qty: 1.0,
            limit_price: 100.0,
        };

        router
            .place_order("acc_test", &intent, None)
            .await
            .expect("place");

        let count = db::count_orders_for_account(database.pool(), "acc_test")
            .await
            .expect("count");
        assert_eq!(count, 1);
    }
}
