use async_trait::async_trait;
use broker::{
    BinanceLimitOrderRequest, BinanceOrderAck, BinanceOrderSide, BinanceTrade, BrokerError,
    IbkrLimitOrderRequest, IbkrOrderAck, IbkrOrderSide, IbkrTrade,
};
use data::{Bar, MarketSlice, SymbolBar};
use paper::{
    BinancePaperOrderClient, BinancePaperOrderExecutor, ExecutedPaperOrder, IbkrPaperOrderClient,
    IbkrPaperOrderExecutor, PaperOrderExecutor, PaperRuntime, PaperSettings, binance_spot_symbol,
    ibkr_stock_symbol,
};
use rust_decimal_macros::dec;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use storage::Db;
use strategies::StrategyUniverseFilterConfig;
use trader_core::{OrderRequest, OrderSide, OrderType};

#[tokio::test]
async fn paper_runtime_counts_orders() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let bars = vec![
        Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
        Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)),
        Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)),
    ];

    let summary = PaperRuntime::new(db, PaperSettings::sample())
        .run_bars(bars)
        .await
        .unwrap();

    assert_eq!(summary.orders, 1);
}

#[tokio::test]
async fn paper_runtime_rejects_order_above_max_order_qty() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let mut settings = PaperSettings::sample();
    settings.order_qty = dec!(2);
    settings.max_order_qty = dec!(1);
    let bars = vec![
        Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
        Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)),
        Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)),
    ];

    let result = PaperRuntime::new(db, settings).run_bars(bars).await;

    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("max order quantity")
    );
}

#[tokio::test]
async fn paper_runtime_uses_market_rules_for_crypto_spot_symbols() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let mut settings = PaperSettings::sample();
    settings.symbol = "CRYPTO:BINANCE:BTCUSDT:CRYPTO_SPOT".to_string();
    settings.order_qty = dec!(0.001);
    settings.max_abs_qty = dec!(1);
    settings.max_order_qty = dec!(1);
    let bars = vec![
        Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10000), dec!(1)),
        Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11000), dec!(1)),
        Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(12000), dec!(1)),
    ];

    let summary = PaperRuntime::new(db, settings)
        .run_bars(bars)
        .await
        .unwrap();

    assert_eq!(summary.orders, 1);
}

#[tokio::test]
async fn paper_runtime_rejects_projected_exposure_above_limit() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let mut settings = PaperSettings::sample();
    settings.max_exposure = dec!(10);
    let bars = vec![
        Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
        Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)),
        Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)),
    ];

    let result = PaperRuntime::new(db, settings).run_bars(bars).await;

    assert!(result.unwrap_err().to_string().contains("max exposure"));
}

#[tokio::test]
async fn paper_runtime_rejects_drawdown_above_limit() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let mut settings = PaperSettings::sample();
    settings.max_drawdown = dec!(0);
    let bars = vec![
        Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
        Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)),
        Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)),
        Bar::new(4, dec!(1), dec!(1), dec!(1), dec!(5), dec!(1)),
    ];

    let result = PaperRuntime::new(db, settings).run_bars(bars).await;

    assert!(result.unwrap_err().to_string().contains("max drawdown"));
}

#[tokio::test]
async fn paper_runtime_uses_configured_universe_and_alpha_names() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let mut settings = PaperSettings::sample();
    settings.universe_name = "unknown_universe".to_string();
    let bars = vec![Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1))];

    let error = PaperRuntime::new(db.clone(), settings)
        .run_bars(bars.clone())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("unknown universe"));

    let mut settings = PaperSettings::sample();
    settings.alpha_name = "unknown_alpha".to_string();
    let error = PaperRuntime::new(db, settings)
        .run_bars(bars)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("unknown strategy unknown_alpha"));
}

#[tokio::test]
async fn paper_runtime_runs_market_slices_for_multiple_symbols() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let mut settings = PaperSettings::sample();
    settings.symbols = vec![
        "US:NASDAQ:AAPL:EQUITY".to_string(),
        "US:NASDAQ:MSFT:EQUITY".to_string(),
    ];
    let slices = vec![
        market_slice(1, dec!(10), dec!(30)),
        market_slice(2, dec!(11), dec!(31)),
        market_slice(3, dec!(20), dec!(40)),
    ];

    let summary = PaperRuntime::new(db.clone(), settings)
        .run_market_slices(slices)
        .await
        .unwrap();

    assert_eq!(summary.signals, 2);
    assert_eq!(summary.orders, 2);
    let orders = db.list_orders("sample-ma-cross").await.unwrap();
    assert_eq!(orders.len(), 2);
    assert_eq!(orders[0].symbol, "US:NASDAQ:AAPL:EQUITY");
    assert_eq!(orders[1].symbol, "US:NASDAQ:MSFT:EQUITY");
    let fills = db.list_fills("sample-ma-cross").await.unwrap();
    assert_eq!(fills.len(), 2);
    let positions = db.list_positions("sample-ma-cross").await.unwrap();
    assert_eq!(positions.len(), 2);
}

#[tokio::test]
async fn paper_runtime_applies_filtered_universe_rules() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let mut settings = PaperSettings::sample();
    settings.universe_name = "filtered".to_string();
    settings.symbols = vec![
        "US:NASDAQ:AAPL:EQUITY".to_string(),
        "US:NASDAQ:MSFT:EQUITY".to_string(),
    ];
    settings.universe_filter = StrategyUniverseFilterConfig {
        include_symbols: Vec::new(),
        exclude_symbols: vec!["US:NASDAQ:MSFT:EQUITY".to_string()],
        symbol_prefixes: Vec::new(),
        require_current_data: false,
        max_symbols: None,
        feature_rank: None,
    };
    let slices = vec![
        market_slice(1, dec!(10), dec!(30)),
        market_slice(2, dec!(11), dec!(31)),
        market_slice(3, dec!(20), dec!(40)),
    ];

    let summary = PaperRuntime::new(db.clone(), settings)
        .run_market_slices(slices)
        .await
        .unwrap();

    assert_eq!(summary.signals, 1);
    assert_eq!(summary.orders, 1);
    let orders = db.list_orders("sample-ma-cross").await.unwrap();
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].symbol, "US:NASDAQ:AAPL:EQUITY");
    let fills = db.list_fills("sample-ma-cross").await.unwrap();
    assert_eq!(fills.len(), 1);
    assert_eq!(fills[0].symbol, "US:NASDAQ:AAPL:EQUITY");
    let positions = db.list_positions("sample-ma-cross").await.unwrap();
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].symbol, "US:NASDAQ:AAPL:EQUITY");
}

#[tokio::test]
async fn paper_runtime_can_use_injected_order_executor() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let bars = vec![
        Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
        Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)),
        Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)),
    ];

    let summary = PaperRuntime::new_with_executor(
        db.clone(),
        PaperSettings::sample(),
        Box::new(FixedExecutor),
    )
    .run_bars(bars)
    .await
    .unwrap();

    assert_eq!(summary.orders, 1);
    let orders = db.list_orders("sample-ma-cross").await.unwrap();
    assert_eq!(orders[0].client_order_id, "external-client-1");
    assert_eq!(orders[0].broker_order_id.as_deref(), Some("external-1"));
    assert_eq!(orders[0].status, "FILLED");
    let fills = db.list_fills("sample-ma-cross").await.unwrap();
    assert_eq!(fills[0].price, "19.5");
    assert_eq!(fills[0].fee, "0.01");
    let events = db.list_events_by_source("sample-ma-cross").await.unwrap();
    let mut categories = events
        .iter()
        .map(|event| event.category.as_str())
        .collect::<Vec<_>>();
    categories.sort_unstable();
    assert!(categories.contains(&"algorithm.universe.selected"));
    assert!(categories.contains(&"algorithm.alpha.generated"));
    assert!(categories.contains(&"algorithm.risk.approved"));
    assert!(categories.contains(&"algorithm.oms.submitted"));
    assert!(categories.contains(&"broker.order.submitted"));
    assert!(categories.contains(&"broker.order.filled"));
    assert!(categories.contains(&"accounting.updated"));
    assert!(events.iter().any(|event| {
        event
            .payload_json
            .contains("\"broker_order_id\":\"external-1\"")
    }));
    let order_events = db.list_order_events("sample-ma-cross").await.unwrap();
    assert!(
        order_events.iter().any(
            |event| event.event_type == "broker.order.submitted" && event.status == "SUBMITTED"
        )
    );
    assert!(
        order_events
            .iter()
            .any(|event| event.event_type == "broker.order.filled" && event.status == "FILLED")
    );
}

#[tokio::test]
async fn paper_runtime_persists_failed_order_after_executor_error() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let bars = vec![
        Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
        Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)),
        Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)),
    ];

    let result = PaperRuntime::new_with_executor(
        db.clone(),
        PaperSettings::sample(),
        Box::new(FailingExecutor),
    )
    .run_bars(bars)
    .await;

    assert!(result.unwrap_err().to_string().contains("broker down"));
    let orders = db.list_orders("sample-ma-cross").await.unwrap();
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].client_order_id, "pending-client-1");
    assert_eq!(orders[0].broker_order_id, None);
    assert_eq!(orders[0].status, "FAILED");
    assert_eq!(orders[0].filled_qty, "0");
    let events = db.list_events_by_source("sample-ma-cross").await.unwrap();
    assert!(
        events
            .iter()
            .any(|event| event.category == "broker.order.submitted")
    );
    assert!(
        events
            .iter()
            .any(|event| event.category == "broker.order.failed")
    );
}

#[tokio::test]
async fn paper_runtime_keeps_unfilled_broker_order_without_fill() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let bars = vec![
        Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
        Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)),
        Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)),
    ];

    let summary = PaperRuntime::new_with_executor(
        db.clone(),
        PaperSettings::sample(),
        Box::new(UnfilledExecutor),
    )
    .run_bars(bars)
    .await
    .unwrap();

    assert_eq!(summary.orders, 1);
    let orders = db.list_orders("sample-ma-cross").await.unwrap();
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].client_order_id, "unfilled-client-1");
    assert_eq!(orders[0].broker_order_id.as_deref(), Some("external-1"));
    assert_eq!(orders[0].status, "CANCELED");
    assert_eq!(orders[0].filled_qty, "0");
    let fills = db.list_fills("sample-ma-cross").await.unwrap();
    assert!(fills.is_empty());
    let events = db.list_events_by_source("sample-ma-cross").await.unwrap();
    let mut categories = events
        .iter()
        .map(|event| event.category.as_str())
        .collect::<Vec<_>>();
    categories.sort_unstable();
    assert!(categories.contains(&"algorithm.universe.selected"));
    assert!(categories.contains(&"algorithm.alpha.generated"));
    assert!(categories.contains(&"algorithm.risk.approved"));
    assert!(categories.contains(&"algorithm.oms.submitted"));
    assert!(categories.contains(&"broker.order.submitted"));
    assert!(categories.contains(&"broker.order.unfilled"));
    assert!(categories.contains(&"accounting.updated"));
    assert!(
        events
            .iter()
            .any(|event| event.payload_json.contains("\"status\":\"CANCELED\""))
    );
}

#[tokio::test]
async fn paper_runtime_records_partial_broker_order_with_fill_and_accounting() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let bars = vec![
        Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
        Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)),
        Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)),
    ];

    let summary = PaperRuntime::new_with_executor(
        db.clone(),
        PaperSettings::sample(),
        Box::new(PartiallyFilledExecutor),
    )
    .run_bars(bars)
    .await
    .unwrap();

    assert_eq!(summary.orders, 1);
    let orders = db.list_orders("sample-ma-cross").await.unwrap();
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].status, "PARTIALLY_FILLED");
    assert_eq!(orders[0].filled_qty, "0.5");
    let fills = db.list_fills("sample-ma-cross").await.unwrap();
    assert_eq!(fills.len(), 1);
    assert_eq!(fills[0].qty, "0.5");
    let events = db.list_events_by_source("sample-ma-cross").await.unwrap();
    assert!(
        events
            .iter()
            .any(|event| event.category == "broker.order.partially_filled")
    );
    assert!(
        events
            .iter()
            .any(|event| event.category == "accounting.updated")
    );
}

#[tokio::test]
async fn paper_runtime_marks_order_failed_when_executor_returns_error() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let bars = vec![
        Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
        Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)),
        Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)),
    ];

    let result = PaperRuntime::new_with_executor(
        db.clone(),
        PaperSettings::sample(),
        Box::new(FailingExecutor),
    )
    .run_bars(bars)
    .await;

    assert!(result.unwrap_err().to_string().contains("broker down"));
    let orders = db.list_orders("sample-ma-cross").await.unwrap();
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].client_order_id, "pending-client-1");
    assert_eq!(orders[0].status, "FAILED");
    assert_eq!(orders[0].filled_qty, "0");
    let fills = db.list_fills("sample-ma-cross").await.unwrap();
    assert!(fills.is_empty());
    let events = db.list_events_by_source("sample-ma-cross").await.unwrap();
    assert!(
        events
            .iter()
            .any(|event| event.category == "broker.order.failed")
    );
    assert!(
        events
            .iter()
            .any(|event| event.payload_json.contains("\"error\":\"broker down\""))
    );
}

#[test]
fn binance_spot_symbol_maps_strategy_symbol_to_exchange_symbol() {
    assert_eq!(
        binance_spot_symbol("CRYPTO:BINANCE:BTCUSDT:CRYPTO_SPOT").unwrap(),
        "BTCUSDT"
    );
    assert_eq!(binance_spot_symbol("BTCUSDT").unwrap(), "BTCUSDT");
}

#[test]
fn ibkr_stock_symbol_maps_strategy_symbol_to_exchange_symbol() {
    assert_eq!(ibkr_stock_symbol("US:NASDAQ:AAPL:EQUITY").unwrap(), "AAPL");
    assert_eq!(ibkr_stock_symbol("AAPL").unwrap(), "AAPL");
}

#[tokio::test]
async fn binance_paper_executor_uses_actual_testnet_trades_as_fill() {
    let executor =
        BinancePaperOrderExecutor::new_with_client_order_prefix(FakeBinanceClient, "paper-run-1");

    let fill = executor
        .execute_order(
            OrderRequest {
                symbol: "CRYPTO:BINANCE:BTCUSDT:CRYPTO_SPOT".to_string(),
                side: OrderSide::Buy,
                order_type: OrderType::Market,
                qty: dec!(0.002),
                price: None,
                account_id: "binance-testnet".to_string(),
            },
            dec!(100000),
            1,
        )
        .await
        .unwrap();

    assert_eq!(fill.broker_order_id, "42");
    assert_eq!(fill.client_order_id, "trader-paper-paper-run-1-1");
    assert_eq!(fill.status, "FILLED");
    assert_eq!(fill.qty, dec!(0.002));
    assert_eq!(fill.price, dec!(100010));
    assert_eq!(fill.fee, dec!(0.000002));
}

#[tokio::test]
async fn binance_paper_executor_returns_zero_fill_for_unfilled_testnet_order() {
    let executor = BinancePaperOrderExecutor::new(UnfilledBinanceClient);

    let fill = executor
        .execute_order(
            OrderRequest {
                symbol: "BTCUSDT".to_string(),
                side: OrderSide::Buy,
                order_type: OrderType::Market,
                qty: dec!(0.001),
                price: None,
                account_id: "binance-testnet".to_string(),
            },
            dec!(100000),
            1,
        )
        .await
        .unwrap();

    assert_eq!(fill.broker_order_id, "99");
    assert_eq!(fill.status, "CANCELED");
    assert_eq!(fill.qty, dec!(0));
    assert_eq!(fill.price, dec!(100000));
    assert_eq!(fill.fee, dec!(0));
}

#[tokio::test]
async fn binance_paper_executor_cancels_unfilled_open_testnet_order() {
    let cancelled = Arc::new(AtomicBool::new(false));
    let executor = BinancePaperOrderExecutor::new(CancellableUnfilledBinanceClient {
        cancelled: Arc::clone(&cancelled),
    });

    let fill = executor
        .execute_order(
            OrderRequest {
                symbol: "BTCUSDT".to_string(),
                side: OrderSide::Buy,
                order_type: OrderType::Market,
                qty: dec!(0.001),
                price: None,
                account_id: "binance-testnet".to_string(),
            },
            dec!(100000),
            1,
        )
        .await
        .unwrap();

    assert_eq!(fill.status, "CANCELED");
    assert_eq!(fill.qty, dec!(0));
    assert!(cancelled.load(Ordering::SeqCst));
}

#[tokio::test]
async fn binance_paper_executor_recovers_trade_when_cancel_races_with_fill() {
    let executor = BinancePaperOrderExecutor::new(CancelRaceFilledBinanceClient {
        query_calls: AtomicUsize::new(0),
        trade_calls: AtomicUsize::new(0),
    });

    let fill = executor
        .execute_order(
            OrderRequest {
                symbol: "BTCUSDT".to_string(),
                side: OrderSide::Buy,
                order_type: OrderType::Market,
                qty: dec!(0.001),
                price: None,
                account_id: "binance-testnet".to_string(),
            },
            dec!(100000),
            1,
        )
        .await
        .unwrap();

    assert_eq!(fill.status, "FILLED");
    assert_eq!(fill.qty, dec!(0.001));
    assert_eq!(fill.price, dec!(100000));
}

#[tokio::test]
async fn binance_paper_executor_recovers_existing_order_by_client_order_id() {
    let executor = BinancePaperOrderExecutor::new_with_client_order_prefix(
        RecoveringBinanceClient,
        "paper-run-1",
    );

    let fill = executor
        .execute_order(
            OrderRequest {
                symbol: "BTCUSDT".to_string(),
                side: OrderSide::Buy,
                order_type: OrderType::Market,
                qty: dec!(0.001),
                price: None,
                account_id: "binance-testnet".to_string(),
            },
            dec!(100000),
            1,
        )
        .await
        .unwrap();

    assert_eq!(fill.client_order_id, "trader-paper-paper-run-1-1");
    assert_eq!(fill.broker_order_id, "77");
    assert_eq!(fill.status, "FILLED");
    assert_eq!(fill.qty, dec!(0.001));
}

#[tokio::test]
async fn ibkr_paper_executor_uses_actual_paper_executions_as_fill() {
    let executor =
        IbkrPaperOrderExecutor::new_with_client_order_prefix(FakeIbkrClient, "paper-run-1");

    let fill = executor
        .execute_order(
            OrderRequest {
                symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
                side: OrderSide::Buy,
                order_type: OrderType::Market,
                qty: dec!(2),
                price: None,
                account_id: "ibkr-paper".to_string(),
            },
            dec!(195),
            1,
        )
        .await
        .unwrap();

    assert_eq!(fill.broker_order_id, "1001");
    assert_eq!(fill.client_order_id, "trader-paper-paper-run-1-1");
    assert_eq!(fill.status, "Filled");
    assert_eq!(fill.qty, dec!(2));
    assert_eq!(fill.price, dec!(195.25));
    assert_eq!(fill.fee, dec!(0.02));
}

#[tokio::test]
async fn ibkr_paper_executor_cancels_unfilled_open_paper_order() {
    let cancelled = Arc::new(AtomicBool::new(false));
    let executor = IbkrPaperOrderExecutor::new(CancellableUnfilledIbkrClient {
        cancelled: Arc::clone(&cancelled),
    });

    let fill = executor
        .execute_order(
            OrderRequest {
                symbol: "AAPL".to_string(),
                side: OrderSide::Buy,
                order_type: OrderType::Market,
                qty: dec!(1),
                price: None,
                account_id: "ibkr-paper".to_string(),
            },
            dec!(195),
            1,
        )
        .await
        .unwrap();

    assert_eq!(fill.broker_order_id, "2002");
    assert_eq!(fill.status, "Cancelled");
    assert_eq!(fill.qty, dec!(0));
    assert_eq!(fill.price, dec!(195));
    assert_eq!(fill.fee, dec!(0));
    assert!(cancelled.load(Ordering::SeqCst));
}

#[tokio::test]
async fn ibkr_paper_executor_recovers_existing_order_by_client_order_id() {
    let executor =
        IbkrPaperOrderExecutor::new_with_client_order_prefix(RecoveringIbkrClient, "paper-run-1");

    let fill = executor
        .execute_order(
            OrderRequest {
                symbol: "AAPL".to_string(),
                side: OrderSide::Buy,
                order_type: OrderType::Market,
                qty: dec!(1),
                price: None,
                account_id: "ibkr-paper".to_string(),
            },
            dec!(195),
            1,
        )
        .await
        .unwrap();

    assert_eq!(fill.client_order_id, "trader-paper-paper-run-1-1");
    assert_eq!(fill.broker_order_id, "3003");
    assert_eq!(fill.status, "Filled");
    assert_eq!(fill.qty, dec!(1));
}

struct FixedExecutor;

#[async_trait]
impl PaperOrderExecutor for FixedExecutor {
    fn client_order_id(&self, _run_id: &str, _order_number: usize) -> String {
        "external-client-1".to_string()
    }

    async fn execute_order(
        &self,
        order: OrderRequest,
        _mark_price: rust_decimal::Decimal,
        _order_number: usize,
    ) -> anyhow::Result<ExecutedPaperOrder> {
        Ok(ExecutedPaperOrder {
            client_order_id: "external-client-1".to_string(),
            broker_order_id: "external-1".to_string(),
            status: "FILLED".to_string(),
            price: dec!(19.5),
            qty: order.qty,
            fee: dec!(0.01),
        })
    }
}

fn market_slice(
    ts_ms: i64,
    aapl_close: rust_decimal::Decimal,
    msft_close: rust_decimal::Decimal,
) -> MarketSlice {
    MarketSlice::new(
        ts_ms,
        vec![
            SymbolBar::new(
                "US:NASDAQ:AAPL:EQUITY",
                Bar::new(
                    ts_ms,
                    aapl_close,
                    aapl_close,
                    aapl_close,
                    aapl_close,
                    dec!(1),
                ),
            ),
            SymbolBar::new(
                "US:NASDAQ:MSFT:EQUITY",
                Bar::new(
                    ts_ms,
                    msft_close,
                    msft_close,
                    msft_close,
                    msft_close,
                    dec!(1),
                ),
            ),
        ],
    )
}

struct PartiallyFilledExecutor;

#[async_trait]
impl PaperOrderExecutor for PartiallyFilledExecutor {
    fn client_order_id(&self, _run_id: &str, _order_number: usize) -> String {
        "partial-client-1".to_string()
    }

    async fn execute_order(
        &self,
        _order: OrderRequest,
        _mark_price: rust_decimal::Decimal,
        _order_number: usize,
    ) -> anyhow::Result<ExecutedPaperOrder> {
        Ok(ExecutedPaperOrder {
            client_order_id: "partial-client-1".to_string(),
            broker_order_id: "external-partial-1".to_string(),
            status: "PARTIALLY_FILLED".to_string(),
            price: dec!(19.5),
            qty: dec!(0.5),
            fee: dec!(0.005),
        })
    }
}

struct FailingExecutor;

#[async_trait]
impl PaperOrderExecutor for FailingExecutor {
    fn client_order_id(&self, _run_id: &str, _order_number: usize) -> String {
        "pending-client-1".to_string()
    }

    async fn execute_order(
        &self,
        _order: OrderRequest,
        _mark_price: rust_decimal::Decimal,
        _order_number: usize,
    ) -> anyhow::Result<ExecutedPaperOrder> {
        anyhow::bail!("broker down")
    }
}

struct UnfilledExecutor;

#[async_trait]
impl PaperOrderExecutor for UnfilledExecutor {
    fn client_order_id(&self, _run_id: &str, _order_number: usize) -> String {
        "unfilled-client-1".to_string()
    }

    async fn execute_order(
        &self,
        _order: OrderRequest,
        mark_price: rust_decimal::Decimal,
        _order_number: usize,
    ) -> anyhow::Result<ExecutedPaperOrder> {
        Ok(ExecutedPaperOrder {
            client_order_id: "unfilled-client-1".to_string(),
            broker_order_id: "external-1".to_string(),
            status: "CANCELED".to_string(),
            price: mark_price,
            qty: dec!(0),
            fee: dec!(0),
        })
    }
}

struct FakeBinanceClient;

#[async_trait]
impl BinancePaperOrderClient for FakeBinanceClient {
    async fn query_order_by_client_order_id(
        &self,
        symbol: &str,
        client_order_id: &str,
    ) -> Result<Option<BinanceOrderAck>, BrokerError> {
        assert_eq!(symbol, "BTCUSDT");
        assert_eq!(client_order_id, "trader-paper-paper-run-1-1");
        Ok(None)
    }

    async fn place_limit_order(
        &self,
        order: &BinanceLimitOrderRequest,
    ) -> Result<BinanceOrderAck, BrokerError> {
        assert_eq!(order.symbol, "BTCUSDT");
        assert_eq!(order.side, BinanceOrderSide::Buy);
        assert_eq!(order.quantity, dec!(0.002));
        assert_eq!(order.price, dec!(100000));
        assert_eq!(order.client_order_id, "trader-paper-paper-run-1-1");
        Ok(BinanceOrderAck {
            order_id: 42,
            client_order_id: order.client_order_id.clone(),
            status: "NEW".to_string(),
            executed_qty: dec!(0),
        })
    }

    async fn query_order(
        &self,
        symbol: &str,
        order_id: u64,
    ) -> Result<BinanceOrderAck, BrokerError> {
        assert_eq!(symbol, "BTCUSDT");
        assert_eq!(order_id, 42);
        Ok(BinanceOrderAck {
            order_id,
            client_order_id: "trader-paper-1-test".to_string(),
            status: "FILLED".to_string(),
            executed_qty: dec!(0.002),
        })
    }

    async fn cancel_order(
        &self,
        _symbol: &str,
        _order_id: u64,
    ) -> Result<BinanceOrderAck, BrokerError> {
        panic!("cancel_order must not be called after filled trades")
    }

    async fn my_trades(
        &self,
        symbol: &str,
        order_id: u64,
    ) -> Result<Vec<BinanceTrade>, BrokerError> {
        assert_eq!(symbol, "BTCUSDT");
        assert_eq!(order_id, 42);
        Ok(vec![
            BinanceTrade {
                trade_id: 1,
                order_id,
                symbol: symbol.to_string(),
                price: dec!(100000),
                qty: dec!(0.001),
                fee: dec!(0.000001),
                fee_asset: "BTC".to_string(),
                ts_ms: 1,
            },
            BinanceTrade {
                trade_id: 2,
                order_id,
                symbol: symbol.to_string(),
                price: dec!(100020),
                qty: dec!(0.001),
                fee: dec!(0.000001),
                fee_asset: "BTC".to_string(),
                ts_ms: 2,
            },
        ])
    }
}

struct UnfilledBinanceClient;

#[async_trait]
impl BinancePaperOrderClient for UnfilledBinanceClient {
    async fn query_order_by_client_order_id(
        &self,
        _symbol: &str,
        _client_order_id: &str,
    ) -> Result<Option<BinanceOrderAck>, BrokerError> {
        Ok(None)
    }

    async fn place_limit_order(
        &self,
        order: &BinanceLimitOrderRequest,
    ) -> Result<BinanceOrderAck, BrokerError> {
        Ok(BinanceOrderAck {
            order_id: 99,
            client_order_id: order.client_order_id.clone(),
            status: "NEW".to_string(),
            executed_qty: dec!(0),
        })
    }

    async fn query_order(
        &self,
        _symbol: &str,
        order_id: u64,
    ) -> Result<BinanceOrderAck, BrokerError> {
        Ok(BinanceOrderAck {
            order_id,
            client_order_id: "trader-paper-1-test".to_string(),
            status: "NEW".to_string(),
            executed_qty: dec!(0),
        })
    }

    async fn cancel_order(
        &self,
        _symbol: &str,
        _order_id: u64,
    ) -> Result<BinanceOrderAck, BrokerError> {
        Ok(BinanceOrderAck {
            order_id: 99,
            client_order_id: "trader-paper-run-1".to_string(),
            status: "CANCELED".to_string(),
            executed_qty: dec!(0),
        })
    }

    async fn my_trades(
        &self,
        _symbol: &str,
        _order_id: u64,
    ) -> Result<Vec<BinanceTrade>, BrokerError> {
        Ok(Vec::new())
    }
}

struct CancellableUnfilledBinanceClient {
    cancelled: Arc<AtomicBool>,
}

#[async_trait]
impl BinancePaperOrderClient for CancellableUnfilledBinanceClient {
    async fn query_order_by_client_order_id(
        &self,
        _symbol: &str,
        _client_order_id: &str,
    ) -> Result<Option<BinanceOrderAck>, BrokerError> {
        Ok(None)
    }

    async fn place_limit_order(
        &self,
        order: &BinanceLimitOrderRequest,
    ) -> Result<BinanceOrderAck, BrokerError> {
        Ok(BinanceOrderAck {
            order_id: 101,
            client_order_id: order.client_order_id.clone(),
            status: "NEW".to_string(),
            executed_qty: dec!(0),
        })
    }

    async fn query_order(
        &self,
        _symbol: &str,
        order_id: u64,
    ) -> Result<BinanceOrderAck, BrokerError> {
        Ok(BinanceOrderAck {
            order_id,
            client_order_id: "trader-paper-run-1".to_string(),
            status: "NEW".to_string(),
            executed_qty: dec!(0),
        })
    }

    async fn cancel_order(
        &self,
        symbol: &str,
        order_id: u64,
    ) -> Result<BinanceOrderAck, BrokerError> {
        assert_eq!(symbol, "BTCUSDT");
        assert_eq!(order_id, 101);
        self.cancelled.store(true, Ordering::SeqCst);
        Ok(BinanceOrderAck {
            order_id,
            client_order_id: "trader-paper-run-1".to_string(),
            status: "CANCELED".to_string(),
            executed_qty: dec!(0),
        })
    }

    async fn my_trades(
        &self,
        _symbol: &str,
        _order_id: u64,
    ) -> Result<Vec<BinanceTrade>, BrokerError> {
        Ok(Vec::new())
    }
}

struct CancelRaceFilledBinanceClient {
    query_calls: AtomicUsize,
    trade_calls: AtomicUsize,
}

#[async_trait]
impl BinancePaperOrderClient for CancelRaceFilledBinanceClient {
    async fn query_order_by_client_order_id(
        &self,
        _symbol: &str,
        _client_order_id: &str,
    ) -> Result<Option<BinanceOrderAck>, BrokerError> {
        Ok(None)
    }

    async fn place_limit_order(
        &self,
        order: &BinanceLimitOrderRequest,
    ) -> Result<BinanceOrderAck, BrokerError> {
        Ok(BinanceOrderAck {
            order_id: 202,
            client_order_id: order.client_order_id.clone(),
            status: "NEW".to_string(),
            executed_qty: dec!(0),
        })
    }

    async fn query_order(
        &self,
        _symbol: &str,
        order_id: u64,
    ) -> Result<BinanceOrderAck, BrokerError> {
        let status = if self.query_calls.fetch_add(1, Ordering::SeqCst) == 0 {
            "NEW"
        } else {
            "FILLED"
        };
        Ok(BinanceOrderAck {
            order_id,
            client_order_id: "trader-paper-run-1".to_string(),
            status: status.to_string(),
            executed_qty: if status == "FILLED" {
                dec!(0.001)
            } else {
                dec!(0)
            },
        })
    }

    async fn cancel_order(
        &self,
        _symbol: &str,
        _order_id: u64,
    ) -> Result<BinanceOrderAck, BrokerError> {
        Err(BrokerError::Rejected(
            "Binance API error 400 code=-2011 msg=Unknown order sent.".to_string(),
        ))
    }

    async fn my_trades(
        &self,
        symbol: &str,
        order_id: u64,
    ) -> Result<Vec<BinanceTrade>, BrokerError> {
        if self.trade_calls.fetch_add(1, Ordering::SeqCst) == 0 {
            return Ok(Vec::new());
        }
        Ok(vec![BinanceTrade {
            trade_id: 202,
            order_id,
            symbol: symbol.to_string(),
            price: dec!(100000),
            qty: dec!(0.001),
            fee: dec!(0.000001),
            fee_asset: "BTC".to_string(),
            ts_ms: 1,
        }])
    }
}

struct RecoveringBinanceClient;

#[async_trait]
impl BinancePaperOrderClient for RecoveringBinanceClient {
    async fn query_order_by_client_order_id(
        &self,
        symbol: &str,
        client_order_id: &str,
    ) -> Result<Option<BinanceOrderAck>, BrokerError> {
        assert_eq!(symbol, "BTCUSDT");
        assert_eq!(client_order_id, "trader-paper-paper-run-1-1");
        Ok(Some(BinanceOrderAck {
            order_id: 77,
            client_order_id: client_order_id.to_string(),
            status: "FILLED".to_string(),
            executed_qty: dec!(0.001),
        }))
    }

    async fn place_limit_order(
        &self,
        _order: &BinanceLimitOrderRequest,
    ) -> Result<BinanceOrderAck, BrokerError> {
        panic!("place_limit_order must not be called for recoverable client_order_id")
    }

    async fn query_order(
        &self,
        _symbol: &str,
        _order_id: u64,
    ) -> Result<BinanceOrderAck, BrokerError> {
        panic!("query_order must not be called after client id recovery")
    }

    async fn cancel_order(
        &self,
        _symbol: &str,
        _order_id: u64,
    ) -> Result<BinanceOrderAck, BrokerError> {
        panic!("cancel_order must not be called after client id recovery")
    }

    async fn my_trades(
        &self,
        symbol: &str,
        order_id: u64,
    ) -> Result<Vec<BinanceTrade>, BrokerError> {
        assert_eq!(symbol, "BTCUSDT");
        assert_eq!(order_id, 77);
        Ok(vec![BinanceTrade {
            trade_id: 77,
            order_id,
            symbol: symbol.to_string(),
            price: dec!(100000),
            qty: dec!(0.001),
            fee: dec!(0.000001),
            fee_asset: "BTC".to_string(),
            ts_ms: 1,
        }])
    }
}

struct FakeIbkrClient;

#[async_trait]
impl IbkrPaperOrderClient for FakeIbkrClient {
    async fn query_order_by_client_order_id(
        &self,
        symbol: &str,
        client_order_id: &str,
    ) -> Result<Option<IbkrOrderAck>, BrokerError> {
        assert_eq!(symbol, "AAPL");
        assert_eq!(client_order_id, "trader-paper-paper-run-1-1");
        Ok(None)
    }

    async fn place_limit_order(
        &self,
        order: &IbkrLimitOrderRequest,
    ) -> Result<IbkrOrderAck, BrokerError> {
        assert_eq!(order.symbol, "AAPL");
        assert_eq!(order.side, IbkrOrderSide::Buy);
        assert_eq!(order.quantity, dec!(2));
        assert_eq!(order.price, dec!(195));
        assert_eq!(order.client_order_id, "trader-paper-paper-run-1-1");
        Ok(IbkrOrderAck {
            order_id: 1001,
            client_order_id: order.client_order_id.clone(),
            status: "Submitted".to_string(),
            filled_qty: dec!(0),
        })
    }

    async fn query_order(&self, symbol: &str, order_id: i64) -> Result<IbkrOrderAck, BrokerError> {
        assert_eq!(symbol, "AAPL");
        assert_eq!(order_id, 1001);
        Ok(IbkrOrderAck {
            order_id,
            client_order_id: "trader-paper-paper-run-1-1".to_string(),
            status: "Filled".to_string(),
            filled_qty: dec!(2),
        })
    }

    async fn cancel_order(
        &self,
        _symbol: &str,
        _order_id: i64,
    ) -> Result<IbkrOrderAck, BrokerError> {
        panic!("cancel_order must not be called after filled executions")
    }

    async fn executions(&self, symbol: &str, order_id: i64) -> Result<Vec<IbkrTrade>, BrokerError> {
        assert_eq!(symbol, "AAPL");
        assert_eq!(order_id, 1001);
        Ok(vec![
            IbkrTrade {
                trade_id: "exec-1".to_string(),
                order_id,
                symbol: symbol.to_string(),
                price: dec!(195),
                qty: dec!(1),
                fee: dec!(0.01),
                ts_ms: 1,
            },
            IbkrTrade {
                trade_id: "exec-2".to_string(),
                order_id,
                symbol: symbol.to_string(),
                price: dec!(195.5),
                qty: dec!(1),
                fee: dec!(0.01),
                ts_ms: 2,
            },
        ])
    }
}

struct CancellableUnfilledIbkrClient {
    cancelled: Arc<AtomicBool>,
}

#[async_trait]
impl IbkrPaperOrderClient for CancellableUnfilledIbkrClient {
    async fn query_order_by_client_order_id(
        &self,
        _symbol: &str,
        _client_order_id: &str,
    ) -> Result<Option<IbkrOrderAck>, BrokerError> {
        Ok(None)
    }

    async fn place_limit_order(
        &self,
        order: &IbkrLimitOrderRequest,
    ) -> Result<IbkrOrderAck, BrokerError> {
        Ok(IbkrOrderAck {
            order_id: 2002,
            client_order_id: order.client_order_id.clone(),
            status: "Submitted".to_string(),
            filled_qty: dec!(0),
        })
    }

    async fn query_order(&self, _symbol: &str, order_id: i64) -> Result<IbkrOrderAck, BrokerError> {
        Ok(IbkrOrderAck {
            order_id,
            client_order_id: "trader-paper-run-1".to_string(),
            status: "Submitted".to_string(),
            filled_qty: dec!(0),
        })
    }

    async fn cancel_order(&self, symbol: &str, order_id: i64) -> Result<IbkrOrderAck, BrokerError> {
        assert_eq!(symbol, "AAPL");
        assert_eq!(order_id, 2002);
        self.cancelled.store(true, Ordering::SeqCst);
        Ok(IbkrOrderAck {
            order_id,
            client_order_id: "trader-paper-run-1".to_string(),
            status: "Cancelled".to_string(),
            filled_qty: dec!(0),
        })
    }

    async fn executions(
        &self,
        _symbol: &str,
        _order_id: i64,
    ) -> Result<Vec<IbkrTrade>, BrokerError> {
        Ok(Vec::new())
    }
}

struct RecoveringIbkrClient;

#[async_trait]
impl IbkrPaperOrderClient for RecoveringIbkrClient {
    async fn query_order_by_client_order_id(
        &self,
        symbol: &str,
        client_order_id: &str,
    ) -> Result<Option<IbkrOrderAck>, BrokerError> {
        assert_eq!(symbol, "AAPL");
        assert_eq!(client_order_id, "trader-paper-paper-run-1-1");
        Ok(Some(IbkrOrderAck {
            order_id: 3003,
            client_order_id: client_order_id.to_string(),
            status: "Filled".to_string(),
            filled_qty: dec!(1),
        }))
    }

    async fn place_limit_order(
        &self,
        _order: &IbkrLimitOrderRequest,
    ) -> Result<IbkrOrderAck, BrokerError> {
        panic!("place_limit_order must not be called for recoverable client_order_id")
    }

    async fn query_order(
        &self,
        _symbol: &str,
        _order_id: i64,
    ) -> Result<IbkrOrderAck, BrokerError> {
        panic!("query_order must not be called after client id recovery")
    }

    async fn cancel_order(
        &self,
        _symbol: &str,
        _order_id: i64,
    ) -> Result<IbkrOrderAck, BrokerError> {
        panic!("cancel_order must not be called after client id recovery")
    }

    async fn executions(&self, symbol: &str, order_id: i64) -> Result<Vec<IbkrTrade>, BrokerError> {
        assert_eq!(symbol, "AAPL");
        assert_eq!(order_id, 3003);
        Ok(vec![IbkrTrade {
            trade_id: "exec-3003".to_string(),
            order_id,
            symbol: symbol.to_string(),
            price: dec!(195),
            qty: dec!(1),
            fee: dec!(0.01),
            ts_ms: 1,
        }])
    }
}
