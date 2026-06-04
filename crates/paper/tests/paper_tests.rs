use async_trait::async_trait;
use broker::{
    BinanceLimitOrderRequest, BinanceOrderAck, BinanceOrderSide, BinanceTrade, BrokerError,
};
use data::Bar;
use paper::{
    BinancePaperOrderClient, BinancePaperOrderExecutor, ExecutedPaperOrder, PaperOrderExecutor,
    PaperRuntime, PaperSettings, binance_spot_symbol,
};
use rust_decimal_macros::dec;
use storage::Db;
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
}

#[test]
fn binance_spot_symbol_maps_strategy_symbol_to_exchange_symbol() {
    assert_eq!(
        binance_spot_symbol("CRYPTO:BINANCE:BTCUSDT:CRYPTO_SPOT").unwrap(),
        "BTCUSDT"
    );
    assert_eq!(binance_spot_symbol("BTCUSDT").unwrap(), "BTCUSDT");
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
async fn binance_paper_executor_rejects_unfilled_testnet_order() {
    let executor = BinancePaperOrderExecutor::new(UnfilledBinanceClient);

    let error = executor
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
        .unwrap_err();

    assert!(error.to_string().contains("no fills"));
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

struct FixedExecutor;

#[async_trait]
impl PaperOrderExecutor for FixedExecutor {
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

    async fn my_trades(
        &self,
        _symbol: &str,
        _order_id: u64,
    ) -> Result<Vec<BinanceTrade>, BrokerError> {
        Ok(Vec::new())
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
