use std::collections::HashMap;
use std::sync::Arc;

use domain::OrderIntent;

use crate::adapter::{ExecutionAdapter, OrderAck};
use crate::error::ExecError;

#[derive(Clone)]
pub struct ExecutionRouter {
    routes: HashMap<String, Arc<dyn ExecutionAdapter>>,
}

impl ExecutionRouter {
    pub fn new(routes: HashMap<String, Arc<dyn ExecutionAdapter>>) -> Self {
        Self { routes }
    }

    pub fn resolve(&self, account_id: &str) -> Result<&Arc<dyn ExecutionAdapter>, ExecError> {
        self.routes
            .get(account_id)
            .ok_or_else(|| ExecError::NotConfigured)
    }

    pub async fn place_order(
        &self,
        account_id: &str,
        intent: &OrderIntent,
        idempotency_key: Option<&str>,
    ) -> Result<OrderAck, ExecError> {
        let adapter = self.resolve(account_id)?;
        adapter
            .place_order(account_id, intent, idempotency_key)
            .await
    }
}
