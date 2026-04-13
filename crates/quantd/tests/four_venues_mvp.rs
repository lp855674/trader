use std::collections::HashMap;
use std::sync::Arc;

use domain::Venue;
use exec::{ExecutionAdapter, ExecutionRouter, PaperAdapter};
use ingest::{IngestRegistry, MockBarsAdapter};
use quantd::{RiskLimits, VenueTickParams, run_one_tick_for_venue};
use strategy::AlwaysLongOne;

#[tokio::test]
async fn four_venues_minimal_closed_loop() {
    let database = db::Db::connect("sqlite::memory:").await.expect("db");
    db::ensure_mvp_seed(database.pool()).await.expect("seed");

    let paper = Arc::new(PaperAdapter::new(database.clone()));
    let mut routes = HashMap::new();
    routes.insert(
        "acc_mvp_paper".to_string(),
        paper as Arc<dyn ExecutionAdapter>,
    );
    let router = ExecutionRouter::new(routes);

    let mut registry = IngestRegistry::default();
    registry.register(Arc::new(MockBarsAdapter::paper_bars(Venue::UsEquity)));
    registry.register(Arc::new(MockBarsAdapter::paper_bars(Venue::HkEquity)));
    registry.register(Arc::new(MockBarsAdapter::paper_bars(Venue::Crypto)));
    registry.register(Arc::new(MockBarsAdapter::paper_bars(Venue::Polymarket)));

    let strategy = AlwaysLongOne;
    let ts_ms = 1_i64;
    let risk_limits = RiskLimits::default();

    for venue in [
        Venue::UsEquity,
        Venue::HkEquity,
        Venue::Crypto,
        Venue::Polymarket,
    ] {
        let adapter = registry.for_venue(venue).next().expect("adapter for venue");
        let tick = VenueTickParams {
            account_id: "acc_mvp_paper".to_string(),
            venue,
            symbol: "MVP".to_string(),
            ts_ms,
        };
        run_one_tick_for_venue(
            &database,
            adapter.as_ref(),
            &router,
            &strategy,
            risk_limits,
            &tick,
            None,
        )
        .await
        .expect("pipeline tick");
    }

    let count = db::count_orders_for_account(database.pool(), "acc_mvp_paper")
        .await
        .expect("count");
    assert_eq!(count, 4, "expected one paper order per venue");
}
