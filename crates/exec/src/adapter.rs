use async_trait::async_trait;
use domain::OrderIntent;

use crate::error::ExecError;

#[derive(Debug, Clone)]
pub struct OrderAck {
    pub order_id: String,
    pub exchange_ref: String,
}

#[derive(Debug, Clone)]
pub struct ManualOrderAck {
    pub order_id: String,
    pub exchange_ref: Option<String>,
    pub status: String,
}

#[async_trait]
pub trait ExecutionAdapter: Send + Sync {
    async fn place_order(
        &self,
        account_id: &str,
        intent: &OrderIntent,
        idempotency_key: Option<&str>,
    ) -> Result<OrderAck, ExecError>;

    async fn submit_manual_order(
        &self,
        account_id: &str,
        intent: &OrderIntent,
        idempotency_key: Option<&str>,
    ) -> Result<ManualOrderAck, ExecError>;

    async fn cancel_order(&self, account_id: &str, order_id: &str) -> Result<(), ExecError>;

    async fn amend_order(
        &self,
        account_id: &str,
        order_id: &str,
        qty: f64,
        limit_price: Option<f64>,
    ) -> Result<ManualOrderAck, ExecError>;
}
