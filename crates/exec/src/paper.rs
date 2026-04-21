use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use domain::{OrderIntent, Side};
use tracing::info;
use uuid::Uuid;

use crate::adapter::{ExecutionAdapter, ManualOrderAck, OrderAck};
use crate::error::ExecError;

pub struct PaperAdapter {
    db: db::Db,
}

impl PaperAdapter {
    pub fn new(database: db::Db) -> Self {
        Self { db: database }
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn side_str(side: Side) -> &'static str {
    match side {
        Side::Buy => "buy",
        Side::Sell => "sell",
    }
}

fn validate_manual_order(intent: &OrderIntent) -> Result<(), ExecError> {
    if intent.qty <= 0.0 {
        return Err(ExecError::InvalidOrderRequest(
            "quantity must be positive".to_string(),
        ));
    }
    if intent.limit_price <= 0.0 {
        return Err(ExecError::InvalidOrderRequest(
            "limit price must be positive".to_string(),
        ));
    }
    Ok(())
}

#[async_trait]
impl ExecutionAdapter for PaperAdapter {
    async fn place_order(
        &self,
        account_id: &str,
        intent: &OrderIntent,
        idempotency_key: Option<&str>,
    ) -> Result<OrderAck, ExecError> {
        let pool = self.db.pool();
        let order_id = Uuid::new_v4().to_string();
        let ts = now_ms();
        let order_row = db::NewOrder {
            order_id: &order_id,
            account_id,
            instrument_id: intent.instrument_db_id,
            side: side_str(intent.side),
            qty: intent.qty,
            status: "FILLED",
            order_type: "limit",
            limit_price: Some(intent.limit_price),
            exchange_ref: Some(&format!("paper-{order_id}")),
            idempotency_key,
            created_at_ms: ts,
            updated_at_ms: ts,
        };
        db::insert_order(pool, &order_row).await?;

        let fill_id = Uuid::new_v4().to_string();
        let price = intent.limit_price;
        let fill_row = db::NewFill {
            fill_id: &fill_id,
            order_id: &order_id,
            qty: intent.qty,
            price,
            created_at_ms: ts,
        };
        db::insert_fill(pool, &fill_row).await?;

        Ok(OrderAck {
            order_id: order_id.clone(),
            exchange_ref: format!("paper-{}", order_id),
        })
    }

    async fn submit_manual_order(
        &self,
        account_id: &str,
        intent: &OrderIntent,
        idempotency_key: Option<&str>,
    ) -> Result<ManualOrderAck, ExecError> {
        validate_manual_order(intent)?;
        let order_id = Uuid::new_v4().to_string();
        let exchange_ref = format!("paper-{order_id}");
        let ts = now_ms();
        db::insert_order(
            self.db.pool(),
            &db::NewOrder {
                order_id: &order_id,
                account_id,
                instrument_id: intent.instrument_db_id,
                side: side_str(intent.side),
                qty: intent.qty,
                status: "SUBMITTED",
                order_type: "limit",
                limit_price: Some(intent.limit_price),
                exchange_ref: Some(exchange_ref.as_str()),
                idempotency_key,
                created_at_ms: ts,
                updated_at_ms: ts,
            },
        )
        .await?;
        let order_count = db::count_orders_for_account(self.db.pool(), account_id).await?;
        info!(
            channel = "exec_paper",
            account_id = %account_id,
            order_id = %order_id,
            symbol = %intent.instrument.symbol,
            instrument_id = intent.instrument_db_id,
            side = side_str(intent.side),
            qty = intent.qty,
            limit_price = intent.limit_price,
            order_count,
            "paper manual order persisted"
        );
        Ok(ManualOrderAck {
            order_id,
            exchange_ref: Some(exchange_ref),
            status: "SUBMITTED".to_string(),
        })
    }

    async fn cancel_order(&self, _account_id: &str, order_id: &str) -> Result<(), ExecError> {
        db::cancel_order(self.db.pool(), order_id, now_ms()).await?;
        Ok(())
    }

    async fn amend_order(
        &self,
        _account_id: &str,
        order_id: &str,
        qty: f64,
        limit_price: Option<f64>,
    ) -> Result<ManualOrderAck, ExecError> {
        if qty <= 0.0 {
            return Err(ExecError::InvalidOrderRequest(
                "quantity must be positive".to_string(),
            ));
        }
        if matches!(limit_price, Some(price) if price <= 0.0) {
            return Err(ExecError::InvalidOrderRequest(
                "limit price must be positive".to_string(),
            ));
        }
        db::amend_order(self.db.pool(), order_id, qty, limit_price, now_ms()).await?;
        Ok(ManualOrderAck {
            order_id: order_id.to_string(),
            exchange_ref: None,
            status: "SUBMITTED".to_string(),
        })
    }
}
