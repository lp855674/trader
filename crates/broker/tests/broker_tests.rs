use async_trait::async_trait;
use broker::{
    BinanceHttpClient, BinanceLimitOrderRequest, BinanceOrderSide, BinanceSpotTestnetAdapter,
    BinanceSpotTestnetSettings, Broker, BrokerError, BrokerKind, BrokerOrderStatus,
    BrokerPositionSide, FakeBrokerAdapter, IbkrExecution, IbkrGatewayClient, IbkrLimitOrderRequest,
    IbkrOpenOrder, IbkrOrderAck, IbkrOrderSide, IbkrOrderStatus, IbkrPaperGatewayAdapter,
    IbkrPaperGatewaySettings, IbkrServerVersion, MockBroker, RuntimePositionSnapshot,
    SimulatedBrokerSettings, reconcile_positions, simulate_market_fill,
};
use rust_decimal_macros::dec;
use std::collections::VecDeque;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use trader_core::{OrderRequest, OrderSide, OrderType};

#[derive(Debug, Clone, PartialEq, Eq)]
struct FakeBinanceHttpCall {
    method: &'static str,
    url: String,
    api_key: Option<String>,
}

#[derive(Debug)]
struct FakeBinanceHttpClient {
    calls: Mutex<Vec<FakeBinanceHttpCall>>,
    responses: Mutex<VecDeque<String>>,
}

impl FakeBinanceHttpClient {
    fn new(responses: impl IntoIterator<Item = &'static str>) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            responses: Mutex::new(responses.into_iter().map(str::to_string).collect()),
        }
    }

    fn calls(&self) -> Vec<FakeBinanceHttpCall> {
        self.calls.lock().unwrap().clone()
    }

    fn next_response(&self) -> Result<String, BrokerError> {
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| BrokerError::Config("missing fake Binance HTTP response".to_string()))
    }
}

#[async_trait]
impl BinanceHttpClient for FakeBinanceHttpClient {
    async fn get(&self, url: &str, api_key: Option<&str>) -> Result<String, BrokerError> {
        self.calls.lock().unwrap().push(FakeBinanceHttpCall {
            method: "GET",
            url: url.to_string(),
            api_key: api_key.map(str::to_string),
        });
        self.next_response()
    }

    async fn post(&self, url: &str, api_key: Option<&str>) -> Result<String, BrokerError> {
        self.calls.lock().unwrap().push(FakeBinanceHttpCall {
            method: "POST",
            url: url.to_string(),
            api_key: api_key.map(str::to_string),
        });
        self.next_response()
    }

    async fn delete(&self, url: &str, api_key: Option<&str>) -> Result<String, BrokerError> {
        self.calls.lock().unwrap().push(FakeBinanceHttpCall {
            method: "DELETE",
            url: url.to_string(),
            api_key: api_key.map(str::to_string),
        });
        self.next_response()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum FakeIbkrGatewayCall {
    ConnectProbe,
    ConnectAndHandshake,
    ManagedAccounts,
    OpenOrders,
    AccountSnapshot {
        account_id: String,
    },
    Executions {
        request_id: i64,
        account_id: String,
        symbol: String,
    },
    PositionSnapshots {
        account_id: String,
    },
    NextOrderId,
    CancelOrder {
        order_id: i64,
    },
    PlaceLimitOrder {
        account_id: String,
        symbol: String,
        client_order_id: String,
    },
}

#[derive(Debug)]
struct FakeIbkrGatewayClient {
    calls: Mutex<Vec<FakeIbkrGatewayCall>>,
    accounts: Vec<String>,
    account_snapshot: broker::BrokerAccountSnapshot,
    open_orders: Vec<IbkrOpenOrder>,
    executions: Vec<IbkrExecution>,
    position_snapshots: Vec<broker::BrokerPositionSnapshot>,
    next_order_id: i64,
}

impl FakeIbkrGatewayClient {
    fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            accounts: vec!["DU12345".to_string()],
            account_snapshot: broker::BrokerAccountSnapshot {
                account_id: "DU12345".to_string(),
                cash: dec!(100000.25),
                equity: dec!(120000.50),
                buying_power: dec!(200000),
                margin_used: dec!(5000.75),
            },
            open_orders: vec![IbkrOpenOrder {
                order_id: 42,
                account_id: "DU12345".to_string(),
                symbol: "AAPL".to_string(),
                side: "BUY".to_string(),
                order_type: "LMT".to_string(),
                quantity: dec!(1),
                limit_price: Some(dec!(185.25)),
                status: "Submitted".to_string(),
                client_order_id: "client-42".to_string(),
                filled_qty: dec!(0),
            }],
            executions: vec![IbkrExecution {
                request_id: 7,
                order_id: 42,
                trade_id: "exec-42".to_string(),
                symbol: "AAPL".to_string(),
                side: "BUY".to_string(),
                qty: dec!(1),
                price: dec!(185.25),
                fee: dec!(0.35),
            }],
            position_snapshots: vec![broker::BrokerPositionSnapshot {
                account_id: "DU12345".to_string(),
                exchange: "IBKR".to_string(),
                symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
                position_side: BrokerPositionSide::Long,
                qty: dec!(2),
                avg_price: dec!(185.25),
                margin_used: dec!(0),
                unrealized_pnl: dec!(0),
                ts_ms: 1_700_000_000_000,
            }],
            next_order_id: 43,
        }
    }

    fn calls(&self) -> Vec<FakeIbkrGatewayCall> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait]
impl IbkrGatewayClient for FakeIbkrGatewayClient {
    async fn connect_probe(&self) -> Result<(), BrokerError> {
        self.calls
            .lock()
            .unwrap()
            .push(FakeIbkrGatewayCall::ConnectProbe);
        Ok(())
    }

    async fn connect_and_handshake(&self) -> Result<IbkrServerVersion, BrokerError> {
        self.calls
            .lock()
            .unwrap()
            .push(FakeIbkrGatewayCall::ConnectAndHandshake);
        Ok(IbkrServerVersion {
            server_version: 178,
            connection_time: "20260611 09:30:00 CST".to_string(),
        })
    }

    async fn managed_accounts(&self) -> Result<Vec<String>, BrokerError> {
        self.calls
            .lock()
            .unwrap()
            .push(FakeIbkrGatewayCall::ManagedAccounts);
        Ok(self.accounts.clone())
    }

    async fn open_orders(&self) -> Result<Vec<IbkrOpenOrder>, BrokerError> {
        self.calls
            .lock()
            .unwrap()
            .push(FakeIbkrGatewayCall::OpenOrders);
        Ok(self.open_orders.clone())
    }

    async fn account_snapshot(
        &self,
        account_id: &str,
    ) -> Result<broker::BrokerAccountSnapshot, BrokerError> {
        self.calls
            .lock()
            .unwrap()
            .push(FakeIbkrGatewayCall::AccountSnapshot {
                account_id: account_id.to_string(),
            });
        Ok(self.account_snapshot.clone())
    }

    async fn executions(
        &self,
        request_id: i64,
        account_id: &str,
        symbol: &str,
    ) -> Result<Vec<IbkrExecution>, BrokerError> {
        self.calls
            .lock()
            .unwrap()
            .push(FakeIbkrGatewayCall::Executions {
                request_id,
                account_id: account_id.to_string(),
                symbol: symbol.to_string(),
            });
        Ok(self.executions.clone())
    }

    async fn next_order_id(&self) -> Result<i64, BrokerError> {
        self.calls
            .lock()
            .unwrap()
            .push(FakeIbkrGatewayCall::NextOrderId);
        Ok(self.next_order_id)
    }

    async fn position_snapshots(
        &self,
        account_id: &str,
    ) -> Result<Vec<broker::BrokerPositionSnapshot>, BrokerError> {
        self.calls
            .lock()
            .unwrap()
            .push(FakeIbkrGatewayCall::PositionSnapshots {
                account_id: account_id.to_string(),
            });
        Ok(self
            .position_snapshots
            .iter()
            .filter(|position| position.account_id == account_id)
            .cloned()
            .collect())
    }

    async fn cancel_order(&self, order_id: i64) -> Result<IbkrOrderStatus, BrokerError> {
        self.calls
            .lock()
            .unwrap()
            .push(FakeIbkrGatewayCall::CancelOrder { order_id });
        Ok(IbkrOrderStatus {
            order_id,
            status: "Cancelled".to_string(),
            filled_qty: dec!(0),
            remaining_qty: dec!(1),
            avg_fill_price: dec!(0),
        })
    }

    async fn place_limit_order(
        &self,
        account_id: &str,
        order: &IbkrLimitOrderRequest,
    ) -> Result<IbkrOrderAck, BrokerError> {
        self.calls
            .lock()
            .unwrap()
            .push(FakeIbkrGatewayCall::PlaceLimitOrder {
                account_id: account_id.to_string(),
                symbol: order.symbol.clone(),
                client_order_id: order.client_order_id.clone(),
            });
        Ok(IbkrOrderAck {
            order_id: self.next_order_id,
            client_order_id: order.client_order_id.clone(),
            status: "Submitted".to_string(),
            filled_qty: dec!(0),
        })
    }
}

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

#[tokio::test]
async fn fake_broker_returns_deterministic_position_snapshots() {
    let broker = FakeBrokerAdapter::new(BrokerKind::Binance);

    let positions = broker.position_snapshots("paper").await.unwrap();

    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].account_id, "paper");
    assert_eq!(positions[0].exchange, "BINANCE");
    assert_eq!(
        positions[0].symbol,
        "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP"
    );
    assert_eq!(positions[0].position_side, BrokerPositionSide::Long);
    assert_eq!(positions[0].qty, dec!(0.5));
    assert_eq!(positions[0].avg_price, dec!(65000));
    assert_eq!(positions[0].margin_used, dec!(3250));
    assert_eq!(positions[0].unrealized_pnl, dec!(12.5));
}

#[tokio::test]
async fn fake_broker_open_orders_is_empty_when_startup_injection_is_disabled() {
    let broker = FakeBrokerAdapter::new(BrokerKind::Binance);

    let open_orders = broker.open_orders("paper").await.unwrap();

    assert!(open_orders.is_empty());
}

#[tokio::test]
async fn fake_broker_open_orders_injects_startup_order_when_enabled() {
    let broker =
        FakeBrokerAdapter::new(BrokerKind::Binance).with_startup_unmatched_open_order(true);

    let open_orders = broker.open_orders("paper").await.unwrap();

    assert_eq!(open_orders.len(), 1);
    let order = &open_orders[0];
    assert_eq!(order.broker_order_id, "fake-startup-unmatched-open-order");
    assert_eq!(order.client_order_id, "fake-startup-unmatched-client-order");
    assert_eq!(order.account_id, "paper");
    assert_eq!(order.symbol, "US:NASDAQ:AAPL:EQUITY");
    assert_eq!(order.side, OrderSide::Buy);
    assert_eq!(order.order_type, OrderType::Limit);
    assert_eq!(order.price, Some(dec!(185)));
    assert_eq!(order.qty, dec!(1));
    assert_eq!(order.filled_qty, dec!(0));
    assert_eq!(order.status, "SUBMITTED");
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
fn binance_testnet_adapter_accepts_injected_http_client_without_changing_signing() {
    let client = reqwest::Client::builder()
        .user_agent("trader-binance-client-test")
        .build()
        .unwrap();
    let adapter = BinanceSpotTestnetAdapter::new_with_client(
        BinanceSpotTestnetSettings {
            base_url: "https://testnet.binance.vision/api".to_string(),
            api_key: "test-key".to_string(),
            secret_key: "test-secret".to_string(),
            recv_window_ms: 5000,
        },
        client,
    );

    let request = adapter.signed_account_request(1_700_000_000_000);

    assert_eq!(request.api_key, "test-key");
    assert!(request.url.starts_with(
        "https://testnet.binance.vision/api/v3/account?timestamp=1700000000000&recvWindow=5000&signature="
    ));
}

#[tokio::test]
async fn binance_testnet_adapter_routes_readonly_calls_through_http_client_boundary() {
    let client = Arc::new(FakeBinanceHttpClient::new([
        r#"{"serverTime":1700000000000}"#,
        r#"[{"orderId":42,"clientOrderId":"client-42","symbol":"BTCUSDT","status":"NEW","side":"BUY","price":"10000","origQty":"0.001","executedQty":"0"}]"#,
    ]));
    let adapter = BinanceSpotTestnetAdapter::new_with_http_client(
        BinanceSpotTestnetSettings {
            base_url: "https://testnet.binance.vision/api".to_string(),
            api_key: "test-key".to_string(),
            secret_key: "test-secret".to_string(),
            recv_window_ms: 5000,
        },
        client.clone(),
    );

    let orders = adapter.open_orders("BTCUSDT").await.unwrap();

    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].order_id, 42);
    let calls = client.calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].method, "GET");
    assert!(calls[0].url.ends_with("/v3/time"));
    assert_eq!(calls[0].api_key, None);
    assert_eq!(calls[1].method, "GET");
    assert!(calls[1].url.contains("/v3/openOrders?symbol=BTCUSDT"));
    assert_eq!(calls[1].api_key.as_deref(), Some("test-key"));
}

#[tokio::test]
async fn binance_testnet_adapter_maps_trade_side_to_broker_executions() {
    let client = Arc::new(FakeBinanceHttpClient::new([
        r#"{"serverTime":1700000000000}"#,
        r#"[{"id":8,"orderId":43,"symbol":"BTCUSDT","price":"10001","qty":"0.002","commission":"0.01","commissionAsset":"USDT","time":1700000000002,"isBuyer":false}]"#,
    ]));
    let adapter = BinanceSpotTestnetAdapter::new_with_http_client(
        BinanceSpotTestnetSettings {
            base_url: "https://testnet.binance.vision/api".to_string(),
            api_key: "test-key".to_string(),
            secret_key: "test-secret".to_string(),
            recv_window_ms: 5000,
        },
        client.clone(),
    );

    let executions = adapter
        .executions("paper", Some("CRYPTO:BINANCE:BTCUSDT:CRYPTO_SPOT"))
        .await
        .unwrap();

    assert_eq!(executions.len(), 1);
    assert_eq!(executions[0].side, OrderSide::Sell);
    let calls = client.calls();
    assert!(calls[1].url.contains("/v3/myTrades?symbol=BTCUSDT"));
}

#[tokio::test]
async fn binance_testnet_adapter_routes_order_submit_through_http_client_boundary() {
    let client = Arc::new(FakeBinanceHttpClient::new([
        r#"{"serverTime":1700000000000}"#,
        r#"{"orderId":42,"clientOrderId":"client-42","status":"NEW","executedQty":"0"}"#,
    ]));
    let adapter = BinanceSpotTestnetAdapter::new_with_http_client(
        BinanceSpotTestnetSettings {
            base_url: "https://testnet.binance.vision/api".to_string(),
            api_key: "test-key".to_string(),
            secret_key: "test-secret".to_string(),
            recv_window_ms: 5000,
        },
        client.clone(),
    );
    let order = BinanceLimitOrderRequest {
        symbol: "BTCUSDT".to_string(),
        side: BinanceOrderSide::Buy,
        quantity: dec!(0.001),
        price: dec!(10000),
        client_order_id: "client-42".to_string(),
    };

    let ack = adapter.place_limit_order(&order).await.unwrap();

    assert_eq!(ack.order_id, 42);
    assert_eq!(ack.client_order_id, "client-42");
    let calls = client.calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].method, "GET");
    assert!(calls[0].url.ends_with("/v3/time"));
    assert_eq!(calls[1].method, "POST");
    assert!(calls[1].url.contains("/v3/order?symbol=BTCUSDT"));
    assert!(calls[1].url.contains("newClientOrderId=client-42"));
    assert_eq!(calls[1].api_key.as_deref(), Some("test-key"));
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
fn binance_trade_response_maps_is_buyer_to_trade_side() {
    let trades = BinanceSpotTestnetAdapter::parse_trades_json(
        r#"[{"id":8,"orderId":43,"symbol":"BTCUSDT","price":"10001","qty":"0.002","commission":"0.01","commissionAsset":"USDT","time":1700000000002,"isBuyer":false}]"#,
    )
    .unwrap();

    assert_eq!(trades[0].side, OrderSide::Sell);
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
fn binance_position_risk_response_maps_to_broker_positions() {
    let positions = BinanceSpotTestnetAdapter::parse_position_risk_json(
        "paper",
        r#"[{"symbol":"BTCUSDT","positionAmt":"0.5","entryPrice":"65000","leverage":"10","isolatedMargin":"3250","unRealizedProfit":"12.5","positionSide":"BOTH","updateTime":1700000000000}]"#,
    )
    .unwrap();

    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].account_id, "paper");
    assert_eq!(positions[0].exchange, "BINANCE");
    assert_eq!(
        positions[0].symbol,
        "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP"
    );
    assert_eq!(positions[0].position_side, BrokerPositionSide::Long);
    assert_eq!(positions[0].qty, dec!(0.5));
    assert_eq!(positions[0].avg_price, dec!(65000));
    assert_eq!(positions[0].margin_used, dec!(3250));
    assert_eq!(positions[0].unrealized_pnl, dec!(12.5));
}

#[test]
fn binance_reconciliation_detects_drift() {
    let runtime = vec![RuntimePositionSnapshot {
        account_id: "paper".to_string(),
        exchange: "BINANCE".to_string(),
        symbol: "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP".to_string(),
        position_side: BrokerPositionSide::Long,
        qty: dec!(0.4),
        avg_price: dec!(65000),
        margin_used: dec!(2600),
    }];
    let broker = vec![broker::BrokerPositionSnapshot {
        account_id: "paper".to_string(),
        exchange: "BINANCE".to_string(),
        symbol: "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP".to_string(),
        position_side: BrokerPositionSide::Long,
        qty: dec!(0.5),
        avg_price: dec!(65000),
        margin_used: dec!(3250),
        unrealized_pnl: dec!(12.5),
        ts_ms: 1_700_000_000_000,
    }];

    let report = reconcile_positions(&runtime, &broker);

    assert_eq!(report.drift_count(), 2);
    assert!(
        report
            .drifts
            .iter()
            .any(|drift| drift.reason.contains("qty mismatch"))
    );
    assert!(
        report
            .drifts
            .iter()
            .any(|drift| drift.reason.contains("margin mismatch"))
    );
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

#[test]
fn ibkr_paper_gateway_adapter_rejects_common_live_port() {
    let error = IbkrPaperGatewayAdapter::try_new(IbkrPaperGatewaySettings {
        host: "127.0.0.1".to_string(),
        port: 7496,
        client_id: 1,
        connect_timeout: Duration::from_secs(1),
    })
    .unwrap_err();

    assert!(error.to_string().contains("paper port"));
}

#[tokio::test]
async fn ibkr_paper_gateway_adapter_routes_readonly_calls_through_gateway_client_boundary() {
    let client = Arc::new(FakeIbkrGatewayClient::new());
    let adapter = IbkrPaperGatewayAdapter::new_with_gateway_client(
        IbkrPaperGatewaySettings {
            host: "127.0.0.1".to_string(),
            port: 7497,
            client_id: 7,
            connect_timeout: Duration::from_secs(1),
        },
        client.clone(),
    );

    let accounts = adapter.validate_paper_account("DU12345").await.unwrap();
    let open_orders = adapter.open_orders().await.unwrap();
    let executions = adapter.executions(7, "DU12345", "AAPL").await.unwrap();
    let next_order_id = adapter.next_order_id().await.unwrap();

    assert_eq!(accounts, vec!["DU12345"]);
    assert_eq!(open_orders[0].order_id, 42);
    assert_eq!(executions[0].trade_id, "exec-42");
    assert_eq!(next_order_id, 43);
    assert_eq!(
        client.calls(),
        vec![
            FakeIbkrGatewayCall::ManagedAccounts,
            FakeIbkrGatewayCall::OpenOrders,
            FakeIbkrGatewayCall::Executions {
                request_id: 7,
                account_id: "DU12345".to_string(),
                symbol: "AAPL".to_string(),
            },
            FakeIbkrGatewayCall::NextOrderId,
        ]
    );
}

#[tokio::test]
async fn ibkr_paper_gateway_adapter_returns_gateway_position_snapshots() {
    let client = Arc::new(FakeIbkrGatewayClient::new());
    let adapter = IbkrPaperGatewayAdapter::new_with_gateway_client(
        IbkrPaperGatewaySettings {
            host: "127.0.0.1".to_string(),
            port: 7497,
            client_id: 7,
            connect_timeout: Duration::from_secs(1),
        },
        client.clone(),
    );

    let positions = adapter.position_snapshots("DU12345").await.unwrap();

    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].account_id, "DU12345");
    assert_eq!(positions[0].exchange, "IBKR");
    assert_eq!(positions[0].symbol, "US:NASDAQ:AAPL:EQUITY");
    assert_eq!(positions[0].position_side, BrokerPositionSide::Long);
    assert_eq!(positions[0].qty, dec!(2));
    assert_eq!(positions[0].avg_price, dec!(185.25));
    assert_eq!(
        client.calls(),
        vec![FakeIbkrGatewayCall::PositionSnapshots {
            account_id: "DU12345".to_string(),
        }]
    );
}

#[tokio::test]
async fn ibkr_paper_gateway_adapter_returns_gateway_account_snapshot() {
    let client = Arc::new(FakeIbkrGatewayClient::new());
    let adapter = IbkrPaperGatewayAdapter::new_with_gateway_client(
        IbkrPaperGatewaySettings {
            host: "127.0.0.1".to_string(),
            port: 7497,
            client_id: 7,
            connect_timeout: Duration::from_secs(1),
        },
        client.clone(),
    );

    let account = adapter.account_snapshot("DU12345").await.unwrap();

    assert_eq!(account.account_id, "DU12345");
    assert_eq!(account.cash, dec!(100000.25));
    assert_eq!(account.equity, dec!(120000.50));
    assert_eq!(account.buying_power, dec!(200000));
    assert_eq!(account.margin_used, dec!(5000.75));
    assert_eq!(
        client.calls(),
        vec![FakeIbkrGatewayCall::AccountSnapshot {
            account_id: "DU12345".to_string(),
        }]
    );
}

#[tokio::test]
async fn ibkr_paper_gateway_adapter_routes_order_calls_through_gateway_client_boundary() {
    let client = Arc::new(FakeIbkrGatewayClient::new());
    let adapter = IbkrPaperGatewayAdapter::new_with_gateway_client(
        IbkrPaperGatewaySettings {
            host: "127.0.0.1".to_string(),
            port: 7497,
            client_id: 7,
            connect_timeout: Duration::from_secs(1),
        },
        client.clone(),
    );
    let order = IbkrLimitOrderRequest {
        symbol: "AAPL".to_string(),
        side: IbkrOrderSide::Buy,
        quantity: dec!(1),
        price: dec!(185.25),
        client_order_id: "client-42".to_string(),
    };

    let ack = adapter.place_limit_order("DU12345", &order).await.unwrap();
    let cancelled = adapter.cancel_ibkr_order(42).await.unwrap();

    assert_eq!(ack.order_id, 43);
    assert_eq!(ack.client_order_id, "client-42");
    assert_eq!(cancelled.status, "Cancelled");
    assert_eq!(
        client.calls(),
        vec![
            FakeIbkrGatewayCall::PlaceLimitOrder {
                account_id: "DU12345".to_string(),
                symbol: "AAPL".to_string(),
                client_order_id: "client-42".to_string(),
            },
            FakeIbkrGatewayCall::CancelOrder { order_id: 42 },
        ]
    );
}

#[tokio::test]
async fn ibkr_paper_gateway_adapter_reports_connection_error_when_gateway_is_absent() {
    let adapter = IbkrPaperGatewayAdapter::try_new(IbkrPaperGatewaySettings {
        host: "127.0.0.1".to_string(),
        port: 9,
        client_id: 7,
        connect_timeout: Duration::from_millis(200),
    })
    .unwrap();

    let error = adapter.connect_and_handshake().await.unwrap_err();

    assert!(error.to_string().contains("broker connection error"));
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
