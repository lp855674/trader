use async_trait::async_trait;
use domain::OrderIntent;

use crate::adapter::{ExecutionAdapter, ManualOrderAck, OrderAck};
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

    async fn submit_manual_order(
        &self,
        _account_id: &str,
        _intent: &OrderIntent,
        _idempotency_key: Option<&str>,
    ) -> Result<ManualOrderAck, ExecError> {
        Err(ExecError::NotConfigured)
    }

    async fn cancel_order(&self, _account_id: &str, _order_id: &str) -> Result<(), ExecError> {
        Err(ExecError::NotConfigured)
    }

    async fn amend_order(
        &self,
        _account_id: &str,
        _order_id: &str,
        _qty: f64,
        _limit_price: Option<f64>,
    ) -> Result<ManualOrderAck, ExecError> {
        Err(ExecError::NotConfigured)
    }
}
