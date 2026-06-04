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
        let signature = hmac_sha256_hex(&self.settings.secret_key, &query);
        BinanceSignedRequest {
            url: format!(
                "{}/v3/account?{query}&signature={signature}",
                self.settings.base_url.trim_end_matches('/')
            ),
            api_key: self.settings.api_key.clone(),
        }
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
        let request = self.signed_account_request(chrono::Utc::now().timestamp_millis());
        let response = self
            .client
            .get(&request.url)
            .header("X-MBX-APIKEY", request.api_key)
            .send()
            .await?
            .error_for_status()?
            .json::<BinanceAccountResponse>()
            .await?;
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
