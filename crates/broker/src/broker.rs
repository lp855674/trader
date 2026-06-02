#![forbid(unsafe_code)]

use async_trait::async_trait;
use thiserror::Error;
use trader_core::OrderRequest;
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
