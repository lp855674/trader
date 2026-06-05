use storage::{
    Db, NewAccountBalance, NewFill, NewOrder, NewPortfolioSnapshot, NewPosition, NewStrategyRun,
};

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
        error: None,
        config_json: "{}".to_string(),
    })
    .await
    .unwrap();

    let run = db.get_strategy_run("run-1").await.unwrap().unwrap();
    assert_eq!(run.id, "run-1");
    assert_eq!(run.status, "completed");
    assert_eq!(db.list_strategy_runs().await.unwrap().len(), 1);

    db.update_strategy_run_status("run-1", "failed", Some(9), Some("boom"))
        .await
        .unwrap();
    let failed = db.get_strategy_run("run-1").await.unwrap().unwrap();
    assert_eq!(failed.status, "failed");
    assert_eq!(failed.ended_at_ms, Some(9));
    assert_eq!(failed.error, Some("boom".to_string()));

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

    db.upsert_account_balance(NewAccountBalance {
        run_id: "run-1".to_string(),
        account_id: "paper".to_string(),
        asset: "USD".to_string(),
        total: "9990".to_string(),
        available: "9990".to_string(),
        frozen: "0".to_string(),
        updated_at_ms: 3,
    })
    .await
    .unwrap();

    db.insert_portfolio_snapshot(NewPortfolioSnapshot {
        id: "snapshot-1".to_string(),
        run_id: "run-1".to_string(),
        account_id: "paper".to_string(),
        ts_ms: 3,
        cash: "9990".to_string(),
        market_value: "108".to_string(),
        equity: "10098".to_string(),
        realized_pnl: "0".to_string(),
        unrealized_pnl: "0".to_string(),
    })
    .await
    .unwrap();

    assert_eq!(db.list_orders("run-1").await.unwrap().len(), 1);
    let order_by_client_id = db
        .get_order_by_client_order_id("client-1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(order_by_client_id.id, "order-1");
    assert_eq!(
        order_by_client_id.broker_order_id.as_deref(),
        Some("broker-1")
    );
    assert_eq!(db.list_fills("run-1").await.unwrap().len(), 1);
    assert_eq!(db.list_positions("run-1").await.unwrap().len(), 1);
    assert_eq!(db.list_account_balances("run-1").await.unwrap().len(), 1);
    assert_eq!(db.list_portfolio_snapshots("run-1").await.unwrap().len(), 1);

    let recovered = db
        .recover_order_state("run-1", "order-1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(recovered.order_qty, "1");
    assert_eq!(recovered.filled_qty, "1");
    assert_eq!(recovered.status, "FILLED");

    db.update_order_status_by_broker_id("run-1", "broker-1", "CANCELLED", 9)
        .await
        .unwrap();
    let updated = db.list_orders("run-1").await.unwrap();
    assert_eq!(updated[0].status, "CANCELLED");
    assert_eq!(updated[0].updated_at_ms, 9);

    db.update_order_execution_by_broker_id("run-1", "broker-1", "FILLED", "1", 10)
        .await
        .unwrap();
    let executed = db.list_orders("run-1").await.unwrap();
    assert_eq!(executed[0].status, "FILLED");
    assert_eq!(executed[0].filled_qty, "1");
    assert_eq!(executed[0].updated_at_ms, 10);

    db.insert_order(NewOrder {
        id: "order-2".to_string(),
        run_id: "run-1".to_string(),
        client_order_id: "client-2".to_string(),
        broker_order_id: None,
        account_id: "paper".to_string(),
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        side: "BUY".to_string(),
        order_type: "MARKET".to_string(),
        price: None,
        qty: "2".to_string(),
        filled_qty: "0".to_string(),
        status: "SUBMITTED".to_string(),
        created_at_ms: 11,
        updated_at_ms: 11,
    })
    .await
    .unwrap();

    let recoverable = db.list_recoverable_orders("run-1").await.unwrap();
    assert_eq!(recoverable.len(), 1);
    assert_eq!(recoverable[0].client_order_id, "client-2");

    db.update_order_execution_by_client_order_id("client-2", "broker-2", "FILLED", "2", 12)
        .await
        .unwrap();
    let recovered = db
        .get_order_by_client_order_id("client-2")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(recovered.broker_order_id.as_deref(), Some("broker-2"));
    assert_eq!(recovered.status, "FILLED");
    assert_eq!(recovered.filled_qty, "2");

    let updated = db
        .update_order_status_by_client_order_id("run-1", "client-2", "broker-2", "CANCELLED", 13)
        .await
        .unwrap();
    assert_eq!(updated, 1);
    let cancelled = db
        .get_order_by_client_order_id("client-2")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(cancelled.broker_order_id.as_deref(), Some("broker-2"));
    assert_eq!(cancelled.status, "CANCELLED");
    assert_eq!(cancelled.updated_at_ms, 13);

    db.insert_order(NewOrder {
        id: "order-3".to_string(),
        run_id: "run-1".to_string(),
        client_order_id: "client-3".to_string(),
        broker_order_id: Some("broker-3".to_string()),
        account_id: "paper".to_string(),
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        side: "BUY".to_string(),
        order_type: "MARKET".to_string(),
        price: None,
        qty: "3".to_string(),
        filled_qty: "0".to_string(),
        status: "NEW".to_string(),
        created_at_ms: 14,
        updated_at_ms: 14,
    })
    .await
    .unwrap();
    let recoverable = db.list_recoverable_orders("run-1").await.unwrap();
    assert_eq!(recoverable.len(), 1);
    assert_eq!(recoverable[0].client_order_id, "client-3");
}

#[tokio::test]
async fn migrate_adds_error_column_to_existing_strategy_runs_table() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    sqlx::query(
        r#"
        CREATE TABLE strategy_runs (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            mode TEXT NOT NULL,
            status TEXT NOT NULL,
            started_at_ms INTEGER NOT NULL,
            ended_at_ms INTEGER,
            config_json TEXT NOT NULL
        )
        "#,
    )
    .execute(db.pool())
    .await
    .unwrap();

    db.migrate().await.unwrap();
    db.insert_strategy_run(NewStrategyRun {
        id: "run-old-schema".to_string(),
        name: "moving_average_cross".to_string(),
        mode: "paper".to_string(),
        status: "running".to_string(),
        started_at_ms: 1,
        ended_at_ms: None,
        error: None,
        config_json: "{}".to_string(),
    })
    .await
    .unwrap();

    db.update_strategy_run_status("run-old-schema", "failed", Some(2), Some("boom"))
        .await
        .unwrap();
    let run = db
        .get_strategy_run("run-old-schema")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.error, Some("boom".to_string()));
}
