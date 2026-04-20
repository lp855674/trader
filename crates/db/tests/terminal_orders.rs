use db::{Db, NewOrder, amend_order, cancel_order, ensure_mvp_seed, list_raw_orders_for_account, upsert_instrument};

#[tokio::test]
async fn order_rows_expose_limit_price_and_allow_cancel_and_amend() {
    let database = Db::connect("sqlite::memory:").await.expect("db");
    ensure_mvp_seed(database.pool()).await.expect("seed");
    let instrument_id = upsert_instrument(database.pool(), "US_EQUITY", "AAPL.US")
        .await
        .expect("instrument");

    db::insert_order(
        database.pool(),
        &NewOrder {
            order_id: "ord-1",
            account_id: "acc_mvp_paper",
            instrument_id,
            side: "buy",
            qty: 10.0,
            status: "SUBMITTED",
            order_type: "limit",
            limit_price: Some(123.45),
            exchange_ref: Some("paper-ord-1"),
            idempotency_key: Some("client-1"),
            created_at_ms: 100,
            updated_at_ms: 100,
        },
    )
    .await
    .expect("insert order");

    amend_order(database.pool(), "ord-1", 12.0, Some(124.0), 120)
        .await
        .expect("amend");
    cancel_order(database.pool(), "ord-1", 130)
        .await
        .expect("cancel");

    let rows = list_raw_orders_for_account(database.pool(), "acc_mvp_paper")
        .await
        .expect("rows");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].order_type, "limit");
    assert_eq!(rows[0].limit_price, Some(124.0));
    assert_eq!(rows[0].exchange_ref.as_deref(), Some("paper-ord-1"));
    assert_eq!(rows[0].qty, 12.0);
    assert_eq!(rows[0].status, "CANCELLED");
    assert_eq!(rows[0].updated_at_ms, 130);
}
