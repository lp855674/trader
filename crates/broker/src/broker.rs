#![forbid(unsafe_code)]

use async_trait::async_trait;
use rust_decimal::Decimal;
use thiserror::Error;
use trader_core::{OrderRequest, OrderSide, OrderType};
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum BrokerError {
    #[error("broker rejected order: {0}")]
    Rejected(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceOrderResponse {
    pub broker_order_id: String,
    pub accepted: bool,
    pub reason: Option<String>,
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
