use std::collections::HashMap;
use std::sync::Arc;

use domain::{InstrumentId, OrderIntent, Side, Venue};
use exec::{ExecutionAdapter, ExecutionRouter, PaperAdapter};

#[tokio::test]
async fn router_supports_manual_submit_cancel_and_amend_for_paper_accounts() {
    let database = db::Db::connect("sqlite::memory:").await.expect("db");
    db::ensure_mvp_seed(database.pool()).await.expect("seed");
    let paper = Arc::new(PaperAdapter::new(database.clone()));
    let mut routes = HashMap::new();
    routes.insert(
        "acc_mvp_paper".to_string(),
        paper as Arc<dyn ExecutionAdapter>,
    );
    let router = ExecutionRouter::new(routes);

    let intent = OrderIntent {
        strategy_id: "manual_terminal".to_string(),
        instrument: InstrumentId::new(Venue::UsEquity, "AAPL.US"),
        instrument_db_id: db::upsert_instrument(database.pool(), "US_EQUITY", "AAPL.US")
            .await
            .expect("instrument"),
        side: Side::Buy,
        qty: 10.0,
        limit_price: 123.45,
    };

    let submit = router
        .submit_manual_order("acc_mvp_paper", &intent, Some("client-1"))
        .await
        .expect("submit");
    assert_eq!(submit.status, "SUBMITTED");
    router
        .cancel_order("acc_mvp_paper", &submit.order_id)
        .await
        .expect("cancel");

    let amended = router
        .submit_manual_order("acc_mvp_paper", &intent, Some("client-2"))
        .await
        .expect("submit-2");
    let amend = router
        .amend_order("acc_mvp_paper", &amended.order_id, 12.0, Some(124.0))
        .await
        .expect("amend");
    assert_eq!(amend.order_id, amended.order_id);

    let rows = db::list_raw_orders_for_account(database.pool(), "acc_mvp_paper")
        .await
        .expect("rows");
    assert_eq!(rows.len(), 2);
    let amended_row = rows
        .iter()
        .find(|row| row.id == amended.order_id)
        .expect("amended row");
    assert_eq!(amended_row.qty, 12.0);
    assert_eq!(amended_row.limit_price, Some(124.0));
    assert_eq!(amended_row.status, "SUBMITTED");

    let cancelled_row = rows
        .iter()
        .find(|row| row.id == submit.order_id)
        .expect("cancelled row");
    assert_eq!(cancelled_row.status, "CANCELLED");
}

#[tokio::test]
async fn router_manual_order_operations_fail_when_route_missing() {
    let router = ExecutionRouter::new(HashMap::new());
    let error = router.cancel_order("missing", "order-1").await.expect_err("error");
    assert!(matches!(error, exec::ExecError::NotConfigured));
}
