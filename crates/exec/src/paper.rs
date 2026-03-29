use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use domain::{OrderIntent, Side};
use uuid::Uuid;

use crate::adapter::{ExecutionAdapter, OrderAck};
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
            idempotency_key,
            created_at_ms: ts,
        };
        db::insert_order(pool, &order_row).await?;

        let fill_id = Uuid::new_v4().to_string();
        let price = 100.0_f64;
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
}
