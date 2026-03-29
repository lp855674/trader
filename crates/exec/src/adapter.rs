use async_trait::async_trait;
use domain::OrderIntent;

use crate::error::ExecError;

#[derive(Debug, Clone)]
pub struct OrderAck {
    pub order_id: String,
    pub exchange_ref: String,
}

#[async_trait]
pub trait ExecutionAdapter: Send + Sync {
    async fn place_order(
        &self,
        account_id: &str,
        intent: &OrderIntent,
        idempotency_key: Option<&str>,
    ) -> Result<OrderAck, ExecError>;
}
