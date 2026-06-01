use storage::{Db, NewFill, NewOrder, NewPosition, NewStrategyRun};

#[tokio::test]
async fn runtime_records_round_trip() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();

    db.insert_strategy_run(NewStrategyRun {
        id: "run-1".to_string(),
        name: "moving_average_cross".to_string(),
        mode: "backtest".to_string(),
        status: "completed".to_string(),
        started_at_ms: 1,
        ended_at_ms: Some(2),
        config_json: "{}".to_string(),
    })
    .await
    .unwrap();

    db.insert_order(NewOrder {
        id: "order-1".to_string(),
        run_id: "run-1".to_string(),
        client_order_id: "client-1".to_string(),
        broker_order_id: Some("broker-1".to_string()),
        account_id: "paper".to_string(),
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        side: "BUY".to_string(),
        order_type: "MARKET".to_string(),
        price: None,
        qty: "1".to_string(),
        filled_qty: "1".to_string(),
        status: "FILLED".to_string(),
        created_at_ms: 1,
        updated_at_ms: 2,
    })
    .await
    .unwrap();

    db.insert_fill(NewFill {
        id: "fill-1".to_string(),
        order_id: "order-1".to_string(),
        run_id: "run-1".to_string(),
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        side: "BUY".to_string(),
        price: "108".to_string(),
        qty: "1".to_string(),
        fee: "0".to_string(),
        ts_ms: 3,
    })
    .await
    .unwrap();

    db.upsert_position(NewPosition {
        run_id: "run-1".to_string(),
        account_id: "paper".to_string(),
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        qty: "1".to_string(),
        avg_price: "108".to_string(),
        updated_at_ms: 3,
    })
    .await
    .unwrap();

    assert_eq!(db.list_orders("run-1").await.unwrap().len(), 1);
    assert_eq!(db.list_fills("run-1").await.unwrap().len(), 1);
    assert_eq!(db.list_positions("run-1").await.unwrap().len(), 1);
}
