use storage::{
    Db, NewAccountBalance, NewCryptoPosition, NewEventRecord, NewFill, NewFundingRate,
    NewLotSizeRule, NewOrder, NewOrderEvent, NewPortfolioSnapshot, NewPosition, NewPriceLimitRule,
    NewRiskEvent, NewStrategyRun,
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
async fn audit_projection_records_round_trip_in_time_order() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();

    db.insert_strategy_run(NewStrategyRun {
        id: "run-a".to_string(),
        name: "moving_average_cross".to_string(),
        mode: "paper".to_string(),
        status: "completed".to_string(),
        started_at_ms: 1,
        ended_at_ms: Some(3),
        error: None,
        config_json: "{}".to_string(),
    })
    .await
    .unwrap();

    db.insert_event(NewEventRecord {
        event_id: "event-order".to_string(),
        ts_ms: 2,
        source: "run-a".to_string(),
        category: "audit.raw.order".to_string(),
        payload_json: r#"{"run_id":"run-a","status":"SUBMITTED"}"#.to_string(),
    })
    .await
    .unwrap();
    db.insert_event(NewEventRecord {
        event_id: "event-risk".to_string(),
        ts_ms: 1,
        source: "run-a".to_string(),
        category: "audit.raw.risk".to_string(),
        payload_json: r#"{"run_id":"run-a","decision":"approved"}"#.to_string(),
    })
    .await
    .unwrap();

    db.insert_order_event(NewOrderEvent {
        id: "order-event".to_string(),
        event_id: "event-order".to_string(),
        run_id: "run-a".to_string(),
        order_id: Some("order-a".to_string()),
        client_order_id: Some("client-a".to_string()),
        broker_order_id: None,
        account_id: Some("paper".to_string()),
        symbol: Some("US:NASDAQ:AAPL:EQUITY".to_string()),
        status: "SUBMITTED".to_string(),
        event_type: "broker.order.submitted".to_string(),
        message: None,
        ts_ms: 2,
        payload_json: r#"{"run_id":"run-a","status":"SUBMITTED"}"#.to_string(),
    })
    .await
    .unwrap();
    db.insert_risk_event(NewRiskEvent {
        id: "risk-event".to_string(),
        event_id: "event-risk".to_string(),
        run_id: "run-a".to_string(),
        account_id: Some("paper".to_string()),
        symbol: Some("US:NASDAQ:AAPL:EQUITY".to_string()),
        risk_type: "max_exposure".to_string(),
        decision: "approved".to_string(),
        reason: None,
        threshold: Some("10000".to_string()),
        observed_value: Some("100".to_string()),
        ts_ms: 1,
        payload_json: r#"{"run_id":"run-a","decision":"approved"}"#.to_string(),
    })
    .await
    .unwrap();

    let order_events = db.list_order_events("run-a").await.unwrap();
    assert_eq!(order_events.len(), 1);
    assert_eq!(order_events[0].event_type, "broker.order.submitted");
    assert_eq!(order_events[0].client_order_id.as_deref(), Some("client-a"));

    let risk_events = db.list_risk_events("run-a").await.unwrap();
    assert_eq!(risk_events.len(), 1);
    assert_eq!(risk_events[0].risk_type, "max_exposure");
    assert_eq!(risk_events[0].threshold.as_deref(), Some("10000"));
}

#[tokio::test]
async fn market_rule_reference_records_prefer_symbol_specific_rules() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();

    db.insert_lot_size_rule(NewLotSizeRule {
        id: "lot-generic".to_string(),
        market: "US".to_string(),
        exchange: "NASDAQ".to_string(),
        asset_class: "EQUITY".to_string(),
        symbol: None,
        lot_size: "1".to_string(),
        min_qty: "1".to_string(),
        min_notional: "0".to_string(),
        effective_from_ms: 1,
        effective_to_ms: None,
    })
    .await
    .unwrap();
    db.insert_lot_size_rule(NewLotSizeRule {
        id: "lot-aapl".to_string(),
        market: "US".to_string(),
        exchange: "NASDAQ".to_string(),
        asset_class: "EQUITY".to_string(),
        symbol: Some("US:NASDAQ:AAPL:EQUITY".to_string()),
        lot_size: "0.0001".to_string(),
        min_qty: "0.0001".to_string(),
        min_notional: "5.25".to_string(),
        effective_from_ms: 2,
        effective_to_ms: None,
    })
    .await
    .unwrap();
    db.insert_price_limit_rule(NewPriceLimitRule {
        id: "price-aapl".to_string(),
        market: "US".to_string(),
        exchange: "NASDAQ".to_string(),
        asset_class: "EQUITY".to_string(),
        symbol: Some("US:NASDAQ:AAPL:EQUITY".to_string()),
        tick_size: "0.0001".to_string(),
        limit_up_bps: Some("1000".to_string()),
        limit_down_bps: Some("1000".to_string()),
        effective_from_ms: 2,
        effective_to_ms: None,
    })
    .await
    .unwrap();

    let lot_rule = db
        .find_lot_size_rule("US", "NASDAQ", "EQUITY", "US:NASDAQ:AAPL:EQUITY", 3)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(lot_rule.id, "lot-aapl");
    assert_eq!(lot_rule.lot_size, "0.0001");
    assert_eq!(lot_rule.min_notional, "5.25");

    let price_rule = db
        .find_price_limit_rule("US", "NASDAQ", "EQUITY", "US:NASDAQ:AAPL:EQUITY", 3)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(price_rule.id, "price-aapl");
    assert_eq!(price_rule.tick_size, "0.0001");
    assert_eq!(price_rule.limit_up_bps.as_deref(), Some("1000"));
}

#[tokio::test]
async fn contract_accounting_records_round_trip_decimal_strings() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();

    db.upsert_crypto_position(NewCryptoPosition {
        run_id: "run-contract".to_string(),
        account_id: "paper".to_string(),
        exchange: "BINANCE".to_string(),
        symbol: "BTCUSDT_PERP".to_string(),
        asset_class: "CRYPTO_PERP".to_string(),
        margin_mode: "cross".to_string(),
        position_side: "short".to_string(),
        leverage: "3.5".to_string(),
        qty: "-0.125".to_string(),
        avg_price: "65000.1234".to_string(),
        margin_used: "812.5015425".to_string(),
        funding_fee: "-1.25".to_string(),
        realized_pnl: "0".to_string(),
        unrealized_pnl: "12.3456".to_string(),
        updated_at_ms: 10,
    })
    .await
    .unwrap();
    db.upsert_crypto_position(NewCryptoPosition {
        run_id: "run-contract".to_string(),
        account_id: "paper".to_string(),
        exchange: "BINANCE".to_string(),
        symbol: "BTCUSDT_PERP".to_string(),
        asset_class: "CRYPTO_PERP".to_string(),
        margin_mode: "cross".to_string(),
        position_side: "short".to_string(),
        leverage: "3.5".to_string(),
        qty: "-0.250".to_string(),
        avg_price: "65001.0000".to_string(),
        margin_used: "1625.025".to_string(),
        funding_fee: "-1.50".to_string(),
        realized_pnl: "2.00".to_string(),
        unrealized_pnl: "20.0001".to_string(),
        updated_at_ms: 11,
    })
    .await
    .unwrap();

    let positions = db.list_crypto_positions("run-contract").await.unwrap();
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].qty, "-0.250");
    assert_eq!(positions[0].avg_price, "65001.0000");
    assert_eq!(positions[0].position_side, "short");

    db.upsert_funding_rate(NewFundingRate {
        id: "funding-1".to_string(),
        exchange: "BINANCE".to_string(),
        symbol: "BTCUSDT_PERP".to_string(),
        funding_time_ms: 1000,
        funding_rate: "0.0001".to_string(),
        mark_price: Some("65000.1234".to_string()),
        source: "testnet".to_string(),
    })
    .await
    .unwrap();
    db.upsert_funding_rate(NewFundingRate {
        id: "funding-1-replacement".to_string(),
        exchange: "BINANCE".to_string(),
        symbol: "BTCUSDT_PERP".to_string(),
        funding_time_ms: 1000,
        funding_rate: "0.0002".to_string(),
        mark_price: Some("65001.0000".to_string()),
        source: "testnet".to_string(),
    })
    .await
    .unwrap();

    let rates = db
        .list_funding_rates("BINANCE", "BTCUSDT_PERP", 0, 2000)
        .await
        .unwrap();
    assert_eq!(rates.len(), 1);
    assert_eq!(rates[0].funding_rate, "0.0002");
    assert_eq!(rates[0].mark_price.as_deref(), Some("65001.0000"));
}

#[tokio::test]
async fn backtest_repository_records_completed_run_execution_position_and_events() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();

    db.insert_runtime_events(
        "backtest-run",
        &[storage::StoredRuntimeEvent {
            ts_ms: 1,
            category: "algorithm.alpha.generated".to_string(),
            payload_json: "{}".to_string(),
        }],
    )
    .await
    .unwrap();

    db.insert_filled_backtest_execution(storage::BacktestExecutionRecord {
        run_id: "backtest-run".to_string(),
        order_id: "order-1".to_string(),
        fill_id: "fill-1".to_string(),
        broker_order_id: "broker-1".to_string(),
        account_id: "backtest".to_string(),
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        side: "BUY".to_string(),
        order_type: "MARKET".to_string(),
        price: None,
        qty: "1".to_string(),
        fill_price: "20".to_string(),
        fee: "0".to_string(),
        ts_ms: 3,
    })
    .await
    .unwrap();

    db.upsert_backtest_position(storage::BacktestPositionRecord {
        run_id: "backtest-run".to_string(),
        account_id: "backtest".to_string(),
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        qty: "1".to_string(),
        avg_price: "20".to_string(),
        updated_at_ms: 3,
    })
    .await
    .unwrap();

    db.complete_backtest_run(storage::BacktestCompletedRun {
        run_id: "backtest-run".to_string(),
        strategy_name: "moving_average_cross".to_string(),
        started_at_ms: 1,
        ended_at_ms: 3,
        config_json: "{}".to_string(),
    })
    .await
    .unwrap();

    let run = db.get_strategy_run("backtest-run").await.unwrap().unwrap();
    assert_eq!(run.status, "completed");
    assert_eq!(
        db.list_events_by_source("backtest-run")
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(db.list_orders("backtest-run").await.unwrap().len(), 1);
    assert_eq!(db.list_fills("backtest-run").await.unwrap().len(), 1);
    assert_eq!(db.list_positions("backtest-run").await.unwrap().len(), 1);
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
