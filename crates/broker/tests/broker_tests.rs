use broker::{
    BinanceLimitOrderRequest, BinanceOrderSide, BinanceSpotTestnetAdapter,
    BinanceSpotTestnetSettings, Broker, BrokerKind, BrokerOrderStatus, FakeBrokerAdapter,
    MockBroker, SimulatedBrokerSettings, simulate_market_fill,
};
use rust_decimal_macros::dec;
use trader_core::{OrderRequest, OrderSide, OrderType};

#[tokio::test]
async fn mock_broker_accepts_order() {
    let broker = MockBroker;
    let ack = broker
        .place_order(OrderRequest {
            symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
            side: OrderSide::Buy,
            order_type: OrderType::Market,
            qty: dec!(1),
            price: None,
            account_id: "paper".to_string(),
        })
        .await
        .unwrap();

    assert!(ack.accepted);
}

#[test]
fn simulated_buy_market_fill_applies_slippage_and_fee() {
    let fill = simulate_market_fill(
        OrderRequest {
            symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
            side: OrderSide::Buy,
            order_type: OrderType::Market,
            qty: dec!(1),
            price: None,
            account_id: "paper".to_string(),
        },
        dec!(100),
        SimulatedBrokerSettings {
            slippage_bps: dec!(100),
            fee_bps: dec!(10),
        },
    )
    .unwrap();

    assert_eq!(fill.price, dec!(101));
    assert_eq!(fill.qty, dec!(1));
    assert_eq!(fill.fee, dec!(0.101));
}

#[tokio::test]
async fn fake_v1_connector_adapters_report_status_and_accept_orders() {
    for kind in [
        BrokerKind::Futu,
        BrokerKind::Binance,
        BrokerKind::Okx,
        BrokerKind::InteractiveBrokers,
    ] {
        let broker = FakeBrokerAdapter::new(kind);
        let status = broker.status().await.unwrap();

        assert_eq!(status.kind, kind);
        assert!(status.connected);
        assert!(status.capabilities.paper_trading);
        assert!(!status.capabilities.live_trading);

        let ack = broker.place_order(order()).await.unwrap();
        assert!(ack.accepted);
        assert!(ack.broker_order_id.starts_with("fake-"));
    }
}

#[tokio::test]
async fn fake_broker_tracks_query_and_cancel_order() {
    let broker = FakeBrokerAdapter::new(BrokerKind::Futu);
    let ack = broker.place_order(order()).await.unwrap();

    let placed = broker.query_order(&ack.broker_order_id).await.unwrap();
    assert_eq!(placed.broker_order_id, ack.broker_order_id);
    assert_eq!(placed.status, BrokerOrderStatus::Accepted);
    assert_eq!(placed.symbol, "US:NASDAQ:AAPL:EQUITY");

    let cancelled = broker.cancel_order(&ack.broker_order_id).await.unwrap();
    assert_eq!(cancelled.status, BrokerOrderStatus::Cancelled);

    let queried_again = broker.query_order(&ack.broker_order_id).await.unwrap();
    assert_eq!(queried_again.status, BrokerOrderStatus::Cancelled);
}

#[tokio::test]
async fn fake_broker_returns_deterministic_account_snapshot() {
    let broker = FakeBrokerAdapter::new(BrokerKind::Binance);

    let account = broker.account_snapshot("paper").await.unwrap();

    assert_eq!(account.account_id, "paper");
    assert_eq!(account.cash, dec!(100000));
    assert_eq!(account.equity, dec!(100000));
    assert_eq!(account.margin_used, dec!(0));
}

#[test]
fn binance_testnet_adapter_builds_signed_account_url() {
    let adapter = BinanceSpotTestnetAdapter::new(BinanceSpotTestnetSettings {
        base_url: "https://testnet.binance.vision/api".to_string(),
        api_key: "test-key".to_string(),
        secret_key: "test-secret".to_string(),
        recv_window_ms: 5000,
    });

    let request = adapter.signed_account_request(1_700_000_000_000);

    assert_eq!(request.api_key, "test-key");
    assert!(request.url.starts_with(
        "https://testnet.binance.vision/api/v3/account?timestamp=1700000000000&recvWindow=5000&signature="
    ));
    assert!(
        request
            .url
            .ends_with("3c006375c631729ab444c2afb86bee2999c35b6eeec838b8f96697e8f096d7b3")
    );
}

#[test]
fn binance_testnet_adapter_rejects_live_base_url() {
    let result = BinanceSpotTestnetAdapter::try_new(BinanceSpotTestnetSettings {
        base_url: "https://api.binance.com/api".to_string(),
        api_key: "test-key".to_string(),
        secret_key: "test-secret".to_string(),
        recv_window_ms: 5000,
    });

    assert!(result.unwrap_err().to_string().contains("testnet"));
}

#[test]
fn binance_testnet_adapter_builds_signed_limit_order_cancel_and_query_urls() {
    let adapter = BinanceSpotTestnetAdapter::new(BinanceSpotTestnetSettings {
        base_url: "https://testnet.binance.vision/api".to_string(),
        api_key: "test-key".to_string(),
        secret_key: "test-secret".to_string(),
        recv_window_ms: 5000,
    });
    let order = BinanceLimitOrderRequest {
        symbol: "BTCUSDT".to_string(),
        side: BinanceOrderSide::Buy,
        quantity: dec!(0.001),
        price: dec!(10000),
        client_order_id: "trader-test-1".to_string(),
    };

    let place = adapter.signed_limit_order_request(&order, 1_700_000_000_000);
    assert!(
        place
            .url
            .starts_with("https://testnet.binance.vision/api/v3/order?")
    );
    assert!(place.url.contains("symbol=BTCUSDT"));
    assert!(place.url.contains("side=BUY"));
    assert!(place.url.contains("type=LIMIT"));
    assert!(place.url.contains("timeInForce=GTC"));
    assert!(place.url.contains("quantity=0.001"));
    assert!(place.url.contains("price=10000"));
    assert!(place.url.contains("newClientOrderId=trader-test-1"));
    assert!(place.url.contains("signature="));

    let query = adapter.signed_query_order_request("BTCUSDT", 42, 1_700_000_000_000);
    assert!(
        query
            .url
            .starts_with("https://testnet.binance.vision/api/v3/order?")
    );
    assert!(query.url.contains("symbol=BTCUSDT"));
    assert!(query.url.contains("orderId=42"));
    assert!(query.url.contains("signature="));

    let query_by_client_id = adapter.signed_query_order_by_client_order_id_request(
        "BTCUSDT",
        "trader-paper-paper-run-1-1",
        1_700_000_000_000,
    );
    assert!(
        query_by_client_id
            .url
            .starts_with("https://testnet.binance.vision/api/v3/order?")
    );
    assert!(query_by_client_id.url.contains("symbol=BTCUSDT"));
    assert!(
        query_by_client_id
            .url
            .contains("origClientOrderId=trader-paper-paper-run-1-1")
    );
    assert!(query_by_client_id.url.contains("signature="));

    let cancel = adapter.signed_cancel_order_request("BTCUSDT", 42, 1_700_000_000_000);
    assert!(
        cancel
            .url
            .starts_with("https://testnet.binance.vision/api/v3/order?")
    );
    assert!(cancel.url.contains("symbol=BTCUSDT"));
    assert!(cancel.url.contains("orderId=42"));
    assert!(cancel.url.contains("signature="));

    let trades = adapter.signed_my_trades_request("BTCUSDT", 42, 1_700_000_000_000);
    assert!(
        trades
            .url
            .starts_with("https://testnet.binance.vision/api/v3/myTrades?")
    );
    assert!(trades.url.contains("symbol=BTCUSDT"));
    assert!(trades.url.contains("orderId=42"));
    assert!(trades.url.contains("signature="));

    let open_orders = adapter.signed_open_orders_request("BTCUSDT", 1_700_000_000_000);
    assert!(
        open_orders
            .url
            .starts_with("https://testnet.binance.vision/api/v3/openOrders?")
    );
    assert!(open_orders.url.contains("symbol=BTCUSDT"));
    assert!(open_orders.url.contains("signature="));
}

#[test]
fn binance_trade_response_maps_to_domain_trade() {
    let trades = BinanceSpotTestnetAdapter::parse_trades_json(
        r#"[{"id":7,"orderId":42,"symbol":"BTCUSDT","price":"10000.5","qty":"0.001","commission":"0.000001","commissionAsset":"BTC","time":1700000000001}]"#,
    )
    .unwrap();

    assert_eq!(trades.len(), 1);
    assert_eq!(trades[0].trade_id, 7);
    assert_eq!(trades[0].order_id, 42);
    assert_eq!(trades[0].symbol, "BTCUSDT");
    assert_eq!(trades[0].price, dec!(10000.5));
    assert_eq!(trades[0].qty, dec!(0.001));
    assert_eq!(trades[0].fee, dec!(0.000001));
    assert_eq!(trades[0].ts_ms, 1700000000001);
}

#[test]
fn binance_open_orders_response_maps_to_domain_orders() {
    let orders = BinanceSpotTestnetAdapter::parse_open_orders_json(
        r#"[{"symbol":"BTCUSDT","orderId":42,"clientOrderId":"trader-paper-1","price":"10000.5","origQty":"0.001","executedQty":"0.0004","status":"PARTIALLY_FILLED","side":"BUY"}]"#,
    )
    .unwrap();

    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].order_id, 42);
    assert_eq!(orders[0].client_order_id, "trader-paper-1");
    assert_eq!(orders[0].symbol, "BTCUSDT");
    assert_eq!(orders[0].status, "PARTIALLY_FILLED");
    assert_eq!(orders[0].side, "BUY");
    assert_eq!(orders[0].price, dec!(10000.5));
    assert_eq!(orders[0].orig_qty, dec!(0.001));
    assert_eq!(orders[0].executed_qty, dec!(0.0004));
}

#[test]
fn binance_account_response_maps_to_asset_balances() {
    let balances = BinanceSpotTestnetAdapter::parse_account_balances_json(
        r#"{"balances":[{"asset":"BTC","free":"0.001","locked":"0.0004"},{"asset":"USDT","free":"100.5","locked":"2"}]}"#,
    )
    .unwrap();

    assert_eq!(balances.len(), 2);
    assert_eq!(balances[0].asset, "BTC");
    assert_eq!(balances[0].free, dec!(0.001));
    assert_eq!(balances[0].locked, dec!(0.0004));
    assert_eq!(balances[0].total(), dec!(0.0014));
    assert_eq!(balances[1].asset, "USDT");
    assert_eq!(balances[1].total(), dec!(102.5));
}

#[test]
fn binance_klines_response_maps_to_bars() {
    let bars = BinanceSpotTestnetAdapter::parse_klines_json(
        r#"[[1700000000000,"10000.1","10010.2","9990.3","10005.4","12.5",1700000059999,"0","0","0","0","0"]]"#,
    )
    .unwrap();

    assert_eq!(bars.len(), 1);
    assert_eq!(bars[0].ts_ms, 1_700_000_000_000);
    assert_eq!(bars[0].open, dec!(10000.1));
    assert_eq!(bars[0].high, dec!(10010.2));
    assert_eq!(bars[0].low, dec!(9990.3));
    assert_eq!(bars[0].close, dec!(10005.4));
    assert_eq!(bars[0].volume, dec!(12.5));
}

#[test]
fn binance_error_response_preserves_code_and_message() {
    let message = BinanceSpotTestnetAdapter::format_error_body(
        400,
        r#"{"code":-1013,"msg":"Filter failure: NOTIONAL"}"#,
    );

    assert_eq!(
        message,
        "Binance API error 400 code=-1013 msg=Filter failure: NOTIONAL"
    );
}

#[test]
fn binance_server_time_response_maps_to_timestamp_ms() {
    let timestamp_ms =
        BinanceSpotTestnetAdapter::parse_server_time_json(r#"{"serverTime":1700000000123}"#)
            .unwrap();

    assert_eq!(timestamp_ms, 1_700_000_000_123);
}

fn order() -> OrderRequest {
    OrderRequest {
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        side: OrderSide::Buy,
        order_type: OrderType::Market,
        qty: dec!(1),
        price: None,
        account_id: "paper".to_string(),
    }
}
