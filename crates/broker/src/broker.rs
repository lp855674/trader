#![forbid(unsafe_code)]

pub mod binance;
pub mod ibkr;

pub use binance::{
    BinanceAssetBalance, BinanceHttpClient, BinanceKlineBar, BinanceLimitOrderRequest,
    BinanceOpenOrder, BinanceOrderAck, BinanceOrderSide, BinanceSignedRequest,
    BinanceSpotTestnetAdapter, BinanceSpotTestnetSettings, BinanceTrade, ReqwestBinanceHttpClient,
};

pub use ibkr::{
    IbapiIbkrGatewayClient, IbkrExecution, IbkrGatewayClient, IbkrLimitOrderRequest, IbkrOpenOrder,
    IbkrOrderAck, IbkrOrderSide, IbkrOrderStatus, IbkrPaperGatewayAdapter,
    IbkrPaperGatewaySettings, IbkrServerVersion, IbkrTrade,
};

use async_trait::async_trait;
use rust_decimal::Decimal;
use serde::Serialize;
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
    #[error("broker connection error: {0}")]
    Connection(String),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BrokerPositionSide {
    Long,
    Short,
}

impl BrokerPositionSide {
    pub fn from_signed_qty(qty: Decimal) -> Option<Self> {
        if qty > Decimal::ZERO {
            Some(Self::Long)
        } else if qty < Decimal::ZERO {
            Some(Self::Short)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimePositionSnapshot {
    pub account_id: String,
    pub exchange: String,
    pub symbol: String,
    pub position_side: BrokerPositionSide,
    pub qty: Decimal,
    pub avg_price: Decimal,
    pub margin_used: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BrokerPositionSnapshot {
    pub account_id: String,
    pub exchange: String,
    pub symbol: String,
    pub position_side: BrokerPositionSide,
    pub qty: Decimal,
    pub avg_price: Decimal,
    pub margin_used: Decimal,
    pub unrealized_pnl: Decimal,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PositionReconciliationDrift {
    pub account_id: String,
    pub exchange: String,
    pub symbol: String,
    pub position_side: BrokerPositionSide,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct PositionReconciliationReport {
    pub drifts: Vec<PositionReconciliationDrift>,
}

impl PositionReconciliationReport {
    pub fn drift_count(&self) -> usize {
        self.drifts.len()
    }
}

pub fn reconcile_positions(
    runtime: &[RuntimePositionSnapshot],
    broker: &[BrokerPositionSnapshot],
) -> PositionReconciliationReport {
    let mut report = PositionReconciliationReport::default();

    for broker_position in broker {
        let runtime_position = runtime.iter().find(|runtime_position| {
            runtime_position.account_id == broker_position.account_id
                && runtime_position.exchange == broker_position.exchange
                && runtime_position.symbol == broker_position.symbol
                && runtime_position.position_side == broker_position.position_side
        });
        let Some(runtime_position) = runtime_position else {
            report.drifts.push(PositionReconciliationDrift {
                account_id: broker_position.account_id.clone(),
                exchange: broker_position.exchange.clone(),
                symbol: broker_position.symbol.clone(),
                position_side: broker_position.position_side,
                reason: "missing runtime position".to_string(),
            });
            continue;
        };

        if runtime_position.qty != broker_position.qty {
            report.drifts.push(PositionReconciliationDrift {
                account_id: broker_position.account_id.clone(),
                exchange: broker_position.exchange.clone(),
                symbol: broker_position.symbol.clone(),
                position_side: broker_position.position_side,
                reason: format!(
                    "qty mismatch runtime={} broker={}",
                    runtime_position.qty, broker_position.qty
                ),
            });
        }
        if runtime_position.avg_price != broker_position.avg_price {
            report.drifts.push(PositionReconciliationDrift {
                account_id: broker_position.account_id.clone(),
                exchange: broker_position.exchange.clone(),
                symbol: broker_position.symbol.clone(),
                position_side: broker_position.position_side,
                reason: format!(
                    "avg_price mismatch runtime={} broker={}",
                    runtime_position.avg_price, broker_position.avg_price
                ),
            });
        }
        if runtime_position.margin_used != broker_position.margin_used {
            report.drifts.push(PositionReconciliationDrift {
                account_id: broker_position.account_id.clone(),
                exchange: broker_position.exchange.clone(),
                symbol: broker_position.symbol.clone(),
                position_side: broker_position.position_side,
                reason: format!(
                    "margin mismatch runtime={} broker={}",
                    runtime_position.margin_used, broker_position.margin_used
                ),
            });
        }
    }

    for runtime_position in runtime {
        if broker.iter().any(|broker_position| {
            broker_position.account_id == runtime_position.account_id
                && broker_position.exchange == runtime_position.exchange
                && broker_position.symbol == runtime_position.symbol
                && broker_position.position_side == runtime_position.position_side
        }) {
            continue;
        }
        report.drifts.push(PositionReconciliationDrift {
            account_id: runtime_position.account_id.clone(),
            exchange: runtime_position.exchange.clone(),
            symbol: runtime_position.symbol.clone(),
            position_side: runtime_position.position_side,
            reason: "missing broker position".to_string(),
        });
    }

    report
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
