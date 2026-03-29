use async_trait::async_trait;
use domain::OrderIntent;

use crate::adapter::{ExecutionAdapter, OrderAck};
use crate::error::ExecError;

pub struct LiveStubAdapter;

#[async_trait]
impl ExecutionAdapter for LiveStubAdapter {
    async fn place_order(
        &self,
        _account_id: &str,
        _intent: &OrderIntent,
        _idempotency_key: Option<&str>,
    ) -> Result<OrderAck, ExecError> {
        Err(ExecError::NotConfigured)
    }
}
