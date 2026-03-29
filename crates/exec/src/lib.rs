//! Order execution adapters and routing.

mod adapter;
mod error;
mod live_stub;
mod paper;
mod router;

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
            instrument: InstrumentId {
                venue: Venue::UsEquity,
                symbol: "T".to_string(),
            },
            instrument_db_id: iid,
            side: Side::Buy,
            qty: 1.0,
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
