#![forbid(unsafe_code)]

use async_trait::async_trait;
use hmac::{Hmac, Mac};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::{collections::HashMap, sync::Arc};
use thiserror::Error;
use tokio::sync::Mutex;
use trader_core::{OrderRequest, OrderSide, OrderType};
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum BrokerError {
    #[error("broker rejected order: {0}")]
    Rejected(String),
    #[error("broker order not found: {0}")]
    OrderNotFound(String),
    #[error("broker configuration error: {0}")]
    Config(String),
    #[error("broker http error: {0}")]
    Http(#[from] reqwest::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceOrderResponse {
    pub broker_order_id: String,
    pub accepted: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BrokerOrderStatus {
    Accepted,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BrokerOrder {
    pub broker_order_id: String,
    pub account_id: String,
    pub symbol: String,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub qty: Decimal,
    pub price: Option<Decimal>,
    pub status: BrokerOrderStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BrokerAccountSnapshot {
    pub account_id: String,
    pub cash: Decimal,
    pub equity: Decimal,
    pub buying_power: Decimal,
    pub margin_used: Decimal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BrokerKind {
    Simulated,
    Futu,
    Binance,
    Okx,
    InteractiveBrokers,
}

impl BrokerKind {
    fn slug(self) -> &'static str {
        match self {
            Self::Simulated => "simulated",
            Self::Futu => "futu",
            Self::Binance => "binance",
            Self::Okx => "okx",
            Self::InteractiveBrokers => "ib",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BrokerCapabilities {
    pub market_data: bool,
    pub order_submit: bool,
    pub order_cancel: bool,
    pub paper_trading: bool,
    pub live_trading: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BrokerStatus {
    pub kind: BrokerKind,
    pub connected: bool,
    pub trading_enabled: bool,
    pub capabilities: BrokerCapabilities,
}

#[derive(Debug, Clone)]
pub struct SimulatedBrokerSettings {
    pub slippage_bps: Decimal,
    pub fee_bps: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulatedFill {
    pub price: Decimal,
    pub qty: Decimal,
    pub fee: Decimal,
}

#[async_trait]
pub trait Broker: Send + Sync {
    async fn place_order(&self, request: OrderRequest) -> Result<PlaceOrderResponse, BrokerError>;
    async fn cancel_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError>;
    async fn query_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError>;
    async fn account_snapshot(
        &self,
        account_id: &str,
    ) -> Result<BrokerAccountSnapshot, BrokerError>;
    async fn status(&self) -> Result<BrokerStatus, BrokerError>;
}

#[derive(Default)]
pub struct MockBroker;

#[async_trait]
impl Broker for MockBroker {
    async fn place_order(&self, request: OrderRequest) -> Result<PlaceOrderResponse, BrokerError> {
        if request.qty <= rust_decimal::Decimal::ZERO {
            return Err(BrokerError::Rejected("qty must be positive".to_string()));
        }
        Ok(PlaceOrderResponse {
            broker_order_id: Uuid::new_v4().to_string(),
            accepted: true,
            reason: None,
        })
    }

    async fn status(&self) -> Result<BrokerStatus, BrokerError> {
        Ok(fake_status(BrokerKind::Simulated))
    }

    async fn cancel_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError> {
        Err(BrokerError::OrderNotFound(broker_order_id.to_string()))
    }

    async fn query_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError> {
        Err(BrokerError::OrderNotFound(broker_order_id.to_string()))
    }

    async fn account_snapshot(
        &self,
        account_id: &str,
    ) -> Result<BrokerAccountSnapshot, BrokerError> {
        Ok(fake_account_snapshot(account_id))
    }
}

#[derive(Debug, Clone)]
pub struct FakeBrokerAdapter {
    kind: BrokerKind,
    orders: Arc<Mutex<HashMap<String, BrokerOrder>>>,
}

#[derive(Debug, Clone)]
pub struct BinanceSpotTestnetSettings {
    pub base_url: String,
    pub api_key: String,
    pub secret_key: String,
    pub recv_window_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinanceSignedRequest {
    pub url: String,
    pub api_key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinanceOrderSide {
    Buy,
    Sell,
}

impl BinanceOrderSide {
    fn as_query_value(self) -> &'static str {
        match self {
            Self::Buy => "BUY",
            Self::Sell => "SELL",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinanceLimitOrderRequest {
    pub symbol: String,
    pub side: BinanceOrderSide,
    pub quantity: Decimal,
    pub price: Decimal,
    pub client_order_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BinanceOrderAck {
    pub order_id: u64,
    pub client_order_id: String,
    pub status: String,
    pub executed_qty: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BinanceTrade {
    pub trade_id: u64,
    pub order_id: u64,
    pub symbol: String,
    pub price: Decimal,
    pub qty: Decimal,
    pub fee: Decimal,
    pub fee_asset: String,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BinanceOpenOrder {
    pub order_id: u64,
    pub client_order_id: String,
    pub symbol: String,
    pub status: String,
    pub side: String,
    pub price: Decimal,
    pub orig_qty: Decimal,
    pub executed_qty: Decimal,
}

#[derive(Debug, Clone)]
pub struct BinanceSpotTestnetAdapter {
    settings: BinanceSpotTestnetSettings,
    client: reqwest::Client,
}

impl FakeBrokerAdapter {
    pub fn new(kind: BrokerKind) -> Self {
        Self {
            kind,
            orders: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn futu() -> Self {
        Self::new(BrokerKind::Futu)
    }

    pub fn binance() -> Self {
        Self::new(BrokerKind::Binance)
    }

    pub fn okx() -> Self {
        Self::new(BrokerKind::Okx)
    }

    pub fn interactive_brokers() -> Self {
        Self::new(BrokerKind::InteractiveBrokers)
    }
}

impl BinanceSpotTestnetAdapter {
    pub fn try_new(settings: BinanceSpotTestnetSettings) -> Result<Self, BrokerError> {
        if !settings.base_url.contains("testnet.binance.vision") {
            return Err(BrokerError::Config(
                "Binance paper adapter requires Spot testnet base_url".to_string(),
            ));
        }
        Ok(Self::new(settings))
    }

    pub fn new(settings: BinanceSpotTestnetSettings) -> Self {
        Self {
            settings,
            client: reqwest::Client::new(),
        }
    }

    pub fn signed_account_request(&self, timestamp_ms: i64) -> BinanceSignedRequest {
        let query = format!(
            "timestamp={timestamp_ms}&recvWindow={}",
            self.settings.recv_window_ms
        );
        self.signed_request("/v3/account", &query)
    }

    pub fn signed_limit_order_request(
        &self,
        order: &BinanceLimitOrderRequest,
        timestamp_ms: i64,
    ) -> BinanceSignedRequest {
        let query = format!(
            "symbol={}&side={}&type=LIMIT&timeInForce=GTC&quantity={}&price={}&newClientOrderId={}&timestamp={timestamp_ms}&recvWindow={}",
            order.symbol,
            order.side.as_query_value(),
            order.quantity,
            order.price,
            order.client_order_id,
            self.settings.recv_window_ms
        );
        self.signed_request("/v3/order", &query)
    }

    pub fn signed_query_order_request(
        &self,
        symbol: &str,
        order_id: u64,
        timestamp_ms: i64,
    ) -> BinanceSignedRequest {
        let query = format!(
            "symbol={symbol}&orderId={order_id}&timestamp={timestamp_ms}&recvWindow={}",
            self.settings.recv_window_ms
        );
        self.signed_request("/v3/order", &query)
    }

    pub fn signed_query_order_by_client_order_id_request(
        &self,
        symbol: &str,
        client_order_id: &str,
        timestamp_ms: i64,
    ) -> BinanceSignedRequest {
        let query = format!(
            "symbol={symbol}&origClientOrderId={client_order_id}&timestamp={timestamp_ms}&recvWindow={}",
            self.settings.recv_window_ms
        );
        self.signed_request("/v3/order", &query)
    }

    pub fn signed_cancel_order_request(
        &self,
        symbol: &str,
        order_id: u64,
        timestamp_ms: i64,
    ) -> BinanceSignedRequest {
        let query = format!(
            "symbol={symbol}&orderId={order_id}&timestamp={timestamp_ms}&recvWindow={}",
            self.settings.recv_window_ms
        );
        self.signed_request("/v3/order", &query)
    }

    pub fn signed_my_trades_request(
        &self,
        symbol: &str,
        order_id: u64,
        timestamp_ms: i64,
    ) -> BinanceSignedRequest {
        let query = format!(
            "symbol={symbol}&orderId={order_id}&timestamp={timestamp_ms}&recvWindow={}",
            self.settings.recv_window_ms
        );
        self.signed_request("/v3/myTrades", &query)
    }

    pub fn signed_open_orders_request(
        &self,
        symbol: &str,
        timestamp_ms: i64,
    ) -> BinanceSignedRequest {
        let query = format!(
            "symbol={symbol}&timestamp={timestamp_ms}&recvWindow={}",
            self.settings.recv_window_ms
        );
        self.signed_request("/v3/openOrders", &query)
    }

    pub async fn server_time_ms(&self) -> Result<i64, BrokerError> {
        let body = binance_response_body(
            self.client
                .get(format!("{}/v3/time", self.settings.base_url))
                .send()
                .await?,
        )
        .await?;
        Self::parse_server_time_json(&body)
    }

    pub async fn place_limit_order(
        &self,
        order: &BinanceLimitOrderRequest,
    ) -> Result<BinanceOrderAck, BrokerError> {
        let request = self.signed_limit_order_request(order, self.server_time_ms().await?);
        let body = binance_response_body(
            self.client
                .post(&request.url)
                .header("X-MBX-APIKEY", request.api_key)
                .send()
                .await?,
        )
        .await?;
        let response = serde_json::from_str::<BinanceOrderResponse>(&body)
            .map_err(|error| BrokerError::Config(error.to_string()))?;
        Ok(response.into_ack())
    }

    pub async fn query_binance_order(
        &self,
        symbol: &str,
        order_id: u64,
    ) -> Result<BinanceOrderAck, BrokerError> {
        let request =
            self.signed_query_order_request(symbol, order_id, self.server_time_ms().await?);
        let body = binance_response_body(
            self.client
                .get(&request.url)
                .header("X-MBX-APIKEY", request.api_key)
                .send()
                .await?,
        )
        .await?;
        let response = serde_json::from_str::<BinanceOrderResponse>(&body)
            .map_err(|error| BrokerError::Config(error.to_string()))?;
        Ok(response.into_ack())
    }

    pub async fn query_binance_order_by_client_order_id(
        &self,
        symbol: &str,
        client_order_id: &str,
    ) -> Result<BinanceOrderAck, BrokerError> {
        let request = self.signed_query_order_by_client_order_id_request(
            symbol,
            client_order_id,
            self.server_time_ms().await?,
        );
        let body = binance_response_body(
            self.client
                .get(&request.url)
                .header("X-MBX-APIKEY", request.api_key)
                .send()
                .await?,
        )
        .await?;
        let response = serde_json::from_str::<BinanceOrderResponse>(&body)
            .map_err(|error| BrokerError::Config(error.to_string()))?;
        Ok(response.into_ack())
    }

    pub async fn cancel_binance_order(
        &self,
        symbol: &str,
        order_id: u64,
    ) -> Result<BinanceOrderAck, BrokerError> {
        let request =
            self.signed_cancel_order_request(symbol, order_id, self.server_time_ms().await?);
        let body = binance_response_body(
            self.client
                .delete(&request.url)
                .header("X-MBX-APIKEY", request.api_key)
                .send()
                .await?,
        )
        .await?;
        let response = serde_json::from_str::<BinanceOrderResponse>(&body)
            .map_err(|error| BrokerError::Config(error.to_string()))?;
        Ok(response.into_ack())
    }

    pub async fn my_trades(
        &self,
        symbol: &str,
        order_id: u64,
    ) -> Result<Vec<BinanceTrade>, BrokerError> {
        let request = self.signed_my_trades_request(symbol, order_id, self.server_time_ms().await?);
        let body = binance_response_body(
            self.client
                .get(&request.url)
                .header("X-MBX-APIKEY", request.api_key)
                .send()
                .await?,
        )
        .await?;
        Self::parse_trades_json(&body)
    }

    pub async fn open_orders(&self, symbol: &str) -> Result<Vec<BinanceOpenOrder>, BrokerError> {
        let request = self.signed_open_orders_request(symbol, self.server_time_ms().await?);
        let body = binance_response_body(
            self.client
                .get(&request.url)
                .header("X-MBX-APIKEY", request.api_key)
                .send()
                .await?,
        )
        .await?;
        Self::parse_open_orders_json(&body)
    }

    pub fn parse_server_time_json(input: &str) -> Result<i64, BrokerError> {
        let response = serde_json::from_str::<BinanceServerTimeResponse>(input)
            .map_err(|error| BrokerError::Config(error.to_string()))?;
        Ok(response.server_time)
    }

    pub fn parse_open_orders_json(input: &str) -> Result<Vec<BinanceOpenOrder>, BrokerError> {
        let response = serde_json::from_str::<Vec<BinanceOpenOrderResponse>>(input)
            .map_err(|error| BrokerError::Config(error.to_string()))?;
        response
            .into_iter()
            .map(BinanceOpenOrderResponse::try_into_open_order)
            .collect()
    }

    pub fn parse_trades_json(input: &str) -> Result<Vec<BinanceTrade>, BrokerError> {
        let response = serde_json::from_str::<Vec<BinanceTradeResponse>>(input)
            .map_err(|error| BrokerError::Config(error.to_string()))?;
        response
            .into_iter()
            .map(BinanceTradeResponse::try_into_trade)
            .collect()
    }

    pub fn format_error_body(status: u16, body: &str) -> String {
        match serde_json::from_str::<BinanceErrorResponse>(body) {
            Ok(error) => format!(
                "Binance API error {status} code={} msg={}",
                error.code, error.msg
            ),
            Err(_) => format!("Binance API error {status}: {body}"),
        }
    }

    fn signed_request(&self, path: &str, query: &str) -> BinanceSignedRequest {
        let signature = hmac_sha256_hex(&self.settings.secret_key, query);
        BinanceSignedRequest {
            url: format!(
                "{}{path}?{query}&signature={signature}",
                self.settings.base_url.trim_end_matches('/')
            ),
            api_key: self.settings.api_key.clone(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BinanceOrderResponse {
    order_id: u64,
    client_order_id: Option<String>,
    orig_client_order_id: Option<String>,
    status: String,
    executed_qty: Option<String>,
}

impl BinanceOrderResponse {
    fn into_ack(self) -> BinanceOrderAck {
        BinanceOrderAck {
            order_id: self.order_id,
            client_order_id: self
                .client_order_id
                .or(self.orig_client_order_id)
                .unwrap_or_default(),
            status: self.status,
            executed_qty: self
                .executed_qty
                .and_then(|qty| qty.parse::<Decimal>().ok())
                .unwrap_or(Decimal::ZERO),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BinanceServerTimeResponse {
    server_time: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BinanceOpenOrderResponse {
    order_id: u64,
    client_order_id: String,
    symbol: String,
    status: String,
    side: String,
    price: String,
    orig_qty: String,
    executed_qty: String,
}

impl BinanceOpenOrderResponse {
    fn try_into_open_order(self) -> Result<BinanceOpenOrder, BrokerError> {
        Ok(BinanceOpenOrder {
            order_id: self.order_id,
            client_order_id: self.client_order_id,
            symbol: self.symbol,
            status: self.status,
            side: self.side,
            price: self
                .price
                .parse::<Decimal>()
                .map_err(|error| BrokerError::Config(error.to_string()))?,
            orig_qty: self
                .orig_qty
                .parse::<Decimal>()
                .map_err(|error| BrokerError::Config(error.to_string()))?,
            executed_qty: self
                .executed_qty
                .parse::<Decimal>()
                .map_err(|error| BrokerError::Config(error.to_string()))?,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BinanceTradeResponse {
    id: u64,
    order_id: u64,
    symbol: String,
    price: String,
    qty: String,
    commission: String,
    commission_asset: String,
    time: i64,
}

impl BinanceTradeResponse {
    fn try_into_trade(self) -> Result<BinanceTrade, BrokerError> {
        Ok(BinanceTrade {
            trade_id: self.id,
            order_id: self.order_id,
            symbol: self.symbol,
            price: self
                .price
                .parse::<Decimal>()
                .map_err(|error| BrokerError::Config(error.to_string()))?,
            qty: self
                .qty
                .parse::<Decimal>()
                .map_err(|error| BrokerError::Config(error.to_string()))?,
            fee: self
                .commission
                .parse::<Decimal>()
                .map_err(|error| BrokerError::Config(error.to_string()))?,
            fee_asset: self.commission_asset,
            ts_ms: self.time,
        })
    }
}

#[async_trait]
impl Broker for BinanceSpotTestnetAdapter {
    async fn place_order(&self, _request: OrderRequest) -> Result<PlaceOrderResponse, BrokerError> {
        Err(BrokerError::Rejected(
            "Binance testnet order submit is not enabled yet".to_string(),
        ))
    }

    async fn cancel_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError> {
        Err(BrokerError::OrderNotFound(broker_order_id.to_string()))
    }

    async fn query_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError> {
        Err(BrokerError::OrderNotFound(broker_order_id.to_string()))
    }

    async fn account_snapshot(
        &self,
        account_id: &str,
    ) -> Result<BrokerAccountSnapshot, BrokerError> {
        let request = self.signed_account_request(self.server_time_ms().await?);
        let body = binance_response_body(
            self.client
                .get(&request.url)
                .header("X-MBX-APIKEY", request.api_key)
                .send()
                .await?,
        )
        .await?;
        let response = serde_json::from_str::<BinanceAccountResponse>(&body)
            .map_err(|error| BrokerError::Config(error.to_string()))?;
        Ok(BrokerAccountSnapshot {
            account_id: account_id.to_string(),
            cash: response
                .balances
                .iter()
                .find(|balance| balance.asset == "USDT")
                .and_then(|balance| balance.free.parse::<Decimal>().ok())
                .unwrap_or(Decimal::ZERO),
            equity: Decimal::ZERO,
            buying_power: Decimal::ZERO,
            margin_used: Decimal::ZERO,
        })
    }

    async fn status(&self) -> Result<BrokerStatus, BrokerError> {
        self.client
            .get(format!(
                "{}/v3/ping",
                self.settings.base_url.trim_end_matches('/')
            ))
            .send()
            .await?
            .error_for_status()?;
        Ok(fake_status(BrokerKind::Binance))
    }
}

#[derive(Debug, Deserialize)]
struct BinanceAccountResponse {
    balances: Vec<BinanceBalance>,
}

#[derive(Debug, Deserialize)]
struct BinanceBalance {
    asset: String,
    free: String,
}

#[derive(Debug, Deserialize)]
struct BinanceErrorResponse {
    code: i64,
    msg: String,
}

async fn binance_response_body(response: reqwest::Response) -> Result<String, BrokerError> {
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(BrokerError::Rejected(
            BinanceSpotTestnetAdapter::format_error_body(status.as_u16(), &body),
        ));
    }
    Ok(body)
}

fn hmac_sha256_hex(secret_key: &str, payload: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret_key.as_bytes())
        .expect("HMAC accepts keys of any length");
    mac.update(payload.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

#[async_trait]
impl Broker for FakeBrokerAdapter {
    async fn place_order(&self, request: OrderRequest) -> Result<PlaceOrderResponse, BrokerError> {
        if request.qty <= Decimal::ZERO {
            return Err(BrokerError::Rejected("qty must be positive".to_string()));
        }
        let broker_order_id = format!("fake-{}-{}", self.kind.slug(), Uuid::new_v4());
        let order = BrokerOrder {
            broker_order_id: broker_order_id.clone(),
            account_id: request.account_id,
            symbol: request.symbol,
            side: request.side,
            order_type: request.order_type,
            qty: request.qty,
            price: request.price,
            status: BrokerOrderStatus::Accepted,
        };
        self.orders
            .lock()
            .await
            .insert(broker_order_id.clone(), order);
        Ok(PlaceOrderResponse {
            broker_order_id,
            accepted: true,
            reason: None,
        })
    }

    async fn cancel_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError> {
        let mut orders = self.orders.lock().await;
        let order = orders
            .get_mut(broker_order_id)
            .ok_or_else(|| BrokerError::OrderNotFound(broker_order_id.to_string()))?;
        order.status = BrokerOrderStatus::Cancelled;
        Ok(order.clone())
    }

    async fn query_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError> {
        self.orders
            .lock()
            .await
            .get(broker_order_id)
            .cloned()
            .ok_or_else(|| BrokerError::OrderNotFound(broker_order_id.to_string()))
    }

    async fn account_snapshot(
        &self,
        account_id: &str,
    ) -> Result<BrokerAccountSnapshot, BrokerError> {
        Ok(fake_account_snapshot(account_id))
    }

    async fn status(&self) -> Result<BrokerStatus, BrokerError> {
        Ok(fake_status(self.kind))
    }
}

fn fake_account_snapshot(account_id: &str) -> BrokerAccountSnapshot {
    BrokerAccountSnapshot {
        account_id: account_id.to_string(),
        cash: Decimal::from(100_000),
        equity: Decimal::from(100_000),
        buying_power: Decimal::from(100_000),
        margin_used: Decimal::ZERO,
    }
}

fn fake_status(kind: BrokerKind) -> BrokerStatus {
    BrokerStatus {
        kind,
        connected: true,
        trading_enabled: true,
        capabilities: BrokerCapabilities {
            market_data: true,
            order_submit: true,
            order_cancel: true,
            paper_trading: true,
            live_trading: false,
        },
    }
}

pub fn simulate_market_fill(
    request: OrderRequest,
    mark_price: Decimal,
    settings: SimulatedBrokerSettings,
) -> Result<SimulatedFill, BrokerError> {
    if request.order_type != OrderType::Market {
        return Err(BrokerError::Rejected(
            "only market orders can be simulated".to_string(),
        ));
    }
    if request.qty <= Decimal::ZERO {
        return Err(BrokerError::Rejected("qty must be positive".to_string()));
    }
    if mark_price <= Decimal::ZERO {
        return Err(BrokerError::Rejected(
            "mark price must be positive".to_string(),
        ));
    }

    let bps_unit = Decimal::new(10_000, 0);
    let slippage = settings.slippage_bps / bps_unit;
    let fee_rate = settings.fee_bps / bps_unit;
    let price = match request.side {
        OrderSide::Buy => mark_price * (Decimal::ONE + slippage),
        OrderSide::Sell => mark_price * (Decimal::ONE - slippage),
    };
    let notional = price * request.qty;

    Ok(SimulatedFill {
        price,
        qty: request.qty,
        fee: notional * fee_rate,
    })
}
