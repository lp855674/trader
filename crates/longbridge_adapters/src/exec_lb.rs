use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use domain::{OrderIntent, Side};
use exec::{ExecError, ExecutionAdapter, ManualOrderAck, OrderAck};
use longbridge::trade::{
    OrderDetail, OrderSide, OrderStatus, OrderType, ReplaceOrderOptions, SubmitOrderOptions,
    TimeInForceType, TradeContext,
};
use rust_decimal::Decimal;
use tracing::info;
use uuid::Uuid;

/// 通过 Longbridge **实盘**限价单（`LO`）。  
/// 虚拟成交请仍使用 `PaperAdapter`；本适配器用于 `acc_lb_live` 等账户。
pub struct LongbridgeTradeAdapter {
    db: db::Db,
    trade: Arc<TradeContext>,
}

impl LongbridgeTradeAdapter {
    pub fn new(database: db::Db, trade: Arc<TradeContext>) -> Self {
        Self {
            db: database,
            trade,
        }
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

async fn persist_submitted_order_ack(
    database: &db::Db,
    account_id: &str,
    intent: &OrderIntent,
    order_id: &str,
    exchange_ref: &str,
    idempotency_key: Option<&str>,
) -> Result<(), ExecError> {
    let ts = now_ms();
    db::insert_order(
        database.pool(),
        &db::NewOrder {
            order_id,
            account_id,
            instrument_id: intent.instrument_db_id,
            side: side_str(intent.side),
            qty: intent.qty,
            status: "SUBMITTED",
            order_type: "limit",
            limit_price: Some(intent.limit_price),
            exchange_ref: Some(exchange_ref),
            idempotency_key,
            created_at_ms: ts,
            updated_at_ms: ts,
        },
    )
    .await?;
    Ok(())
}

async fn persist_cancel_ack(database: &db::Db, order_id: &str) -> Result<(), ExecError> {
    db::cancel_order(database.pool(), order_id, now_ms()).await?;
    Ok(())
}

async fn persist_amend_ack(
    database: &db::Db,
    order_id: &str,
    qty: f64,
    limit_price: Option<f64>,
) -> Result<(), ExecError> {
    db::amend_order(database.pool(), order_id, qty, limit_price, now_ms()).await?;
    Ok(())
}

fn map_longbridge_status(status: OrderStatus) -> &'static str {
    match status {
        OrderStatus::Filled => "FILLED",
        OrderStatus::PartialFilled | OrderStatus::PartialWithdrawal => "PARTIALLY_FILLED",
        OrderStatus::Rejected => "REJECTED",
        OrderStatus::Canceled => "CANCELLED",
        OrderStatus::Expired => "EXPIRED",
        OrderStatus::WaitToCancel | OrderStatus::PendingCancel => "PENDING_CANCEL",
        OrderStatus::WaitToReplace | OrderStatus::PendingReplace | OrderStatus::Replaced => {
            "SUBMITTED"
        }
        OrderStatus::NotReported
        | OrderStatus::ReplacedNotReported
        | OrderStatus::ProtectedNotReported
        | OrderStatus::VarietiesNotReported
        | OrderStatus::WaitToNew
        | OrderStatus::New
        | OrderStatus::Unknown => "SUBMITTED",
    }
}

async fn sync_order_detail_to_ledger(
    database: &db::Db,
    order_id: &str,
    detail: &OrderDetail,
) -> Result<(), ExecError> {
    let updated_at_ms = detail
        .updated_at
        .map(|value| (value.unix_timestamp_nanos() / 1_000_000) as i64)
        .unwrap_or_else(now_ms);
    db::update_order_status(
        database.pool(),
        order_id,
        map_longbridge_status(detail.status),
        updated_at_ms,
    )
    .await?;

    let executed_qty = detail.executed_quantity.to_string().parse::<f64>().map_err(|error| {
        ExecError::Longbridge(format!("invalid executed quantity for {order_id}: {error}"))
    })?;
    let executed_price = detail
        .executed_price
        .map(|value| value.to_string().parse::<f64>())
        .transpose()
        .map_err(|error| ExecError::Longbridge(format!("invalid executed price for {order_id}: {error}")))?;
    let local_filled_qty = db::filled_qty_for_order(database.pool(), order_id).await?;
    let delta_qty = executed_qty - local_filled_qty;
    if delta_qty > 0.000_000_1 {
        let price = executed_price.unwrap_or(0.0);
        if price <= 0.0 {
            return Err(ExecError::Longbridge(format!(
                "order {order_id} missing executed price for filled quantity"
            )));
        }
        let fill_id = Uuid::new_v4().to_string();
        db::insert_fill(
            database.pool(),
            &db::NewFill {
                fill_id: &fill_id,
                order_id,
                qty: delta_qty,
                price,
                created_at_ms: updated_at_ms,
            },
        )
        .await?;
    }
    Ok(())
}

fn lb_side(side: Side) -> OrderSide {
    match side {
        Side::Buy => OrderSide::Buy,
        Side::Sell => OrderSide::Sell,
    }
}

#[async_trait]
impl ExecutionAdapter for LongbridgeTradeAdapter {
    async fn place_order(
        &self,
        account_id: &str,
        intent: &OrderIntent,
        idempotency_key: Option<&str>,
    ) -> Result<OrderAck, ExecError> {
        let qty = Decimal::from_f64_retain(intent.qty)
            .ok_or_else(|| ExecError::Longbridge("invalid quantity".to_string()))?;
        if qty <= Decimal::ZERO {
            return Err(ExecError::Longbridge(
                "quantity must be positive".to_string(),
            ));
        }
        let price = Decimal::from_f64_retain(intent.limit_price)
            .ok_or_else(|| ExecError::Longbridge("invalid limit price".to_string()))?;
        if price <= Decimal::ZERO {
            return Err(ExecError::Longbridge(
                "limit price must be positive".to_string(),
            ));
        }

        let symbol = intent.instrument.symbol.as_str();
        let opts = SubmitOrderOptions::new(
            symbol,
            OrderType::LO,
            lb_side(intent.side),
            qty,
            TimeInForceType::Day,
        )
        .submitted_price(price);

        let resp = self
            .trade
            .submit_order(opts)
            .await
            .map_err(|e| ExecError::Longbridge(e.to_string()))?;
        let exchange_ref = format!("longbridge:{}", resp.order_id);
        persist_submitted_order_ack(
            &self.db,
            account_id,
            intent,
            &resp.order_id,
            exchange_ref.as_str(),
            idempotency_key,
        )
        .await?;
        let order_count = db::count_orders_for_account(self.db.pool(), account_id).await?;
        info!(
            channel = "exec_longbridge",
            account_id = %account_id,
            order_id = %resp.order_id,
            symbol = %intent.instrument.symbol,
            instrument_id = intent.instrument_db_id,
            side = side_str(intent.side),
            qty = intent.qty,
            limit_price = intent.limit_price,
            order_count,
            "longbridge order persisted"
        );

        Ok(OrderAck {
            order_id: resp.order_id.clone(),
            exchange_ref,
        })
    }

    async fn submit_manual_order(
        &self,
        account_id: &str,
        intent: &OrderIntent,
        idempotency_key: Option<&str>,
    ) -> Result<ManualOrderAck, ExecError> {
        let ack = self.place_order(account_id, intent, idempotency_key).await?;
        Ok(ManualOrderAck {
            order_id: ack.order_id,
            exchange_ref: Some(ack.exchange_ref),
            status: "SUBMITTED".to_string(),
        })
    }

    async fn cancel_order(&self, account_id: &str, order_id: &str) -> Result<(), ExecError> {
        self.trade
            .cancel_order(order_id.to_string())
            .await
            .map_err(|error| ExecError::Longbridge(error.to_string()))?;
        persist_cancel_ack(&self.db, order_id).await?;
        let order_count = db::count_orders_for_account(self.db.pool(), account_id).await?;
        info!(
            channel = "exec_longbridge",
            account_id = %account_id,
            order_id = %order_id,
            order_count,
            "longbridge cancel synced to local ledger"
        );
        Ok(())
    }

    async fn amend_order(
        &self,
        account_id: &str,
        order_id: &str,
        qty: f64,
        limit_price: Option<f64>,
    ) -> Result<ManualOrderAck, ExecError> {
        let amended_qty = qty;
        let qty = Decimal::from_f64_retain(qty)
            .ok_or_else(|| ExecError::InvalidOrderRequest("invalid quantity".to_string()))?;
        if qty <= Decimal::ZERO {
            return Err(ExecError::InvalidOrderRequest(
                "quantity must be positive".to_string(),
            ));
        }
        let mut options = ReplaceOrderOptions::new(order_id.to_string(), qty);
        if let Some(limit_price) = limit_price {
            let price = Decimal::from_f64_retain(limit_price)
                .ok_or_else(|| ExecError::InvalidOrderRequest("invalid limit price".to_string()))?;
            if price <= Decimal::ZERO {
                return Err(ExecError::InvalidOrderRequest(
                    "limit price must be positive".to_string(),
                ));
            }
            options = options.price(price);
        }
        self.trade
            .replace_order(options)
            .await
            .map_err(|error| ExecError::Longbridge(error.to_string()))?;
        persist_amend_ack(&self.db, order_id, amended_qty, limit_price).await?;
        info!(
            channel = "exec_longbridge",
            account_id = %account_id,
            order_id = %order_id,
            qty = amended_qty,
            limit_price = ?limit_price,
            "longbridge amend synced to local ledger"
        );
        Ok(ManualOrderAck {
            order_id: order_id.to_string(),
            exchange_ref: Some(format!("longbridge:{order_id}")),
            status: "SUBMITTED".to_string(),
        })
    }

    async fn sync_account_orders(&self, account_id: &str) -> Result<(), ExecError> {
        let rows = db::list_open_orders_for_account(self.db.pool(), account_id).await?;
        for row in rows {
            let detail = self
                .trade
                .order_detail(row.order_id.clone())
                .await
                .map_err(|error| ExecError::Longbridge(error.to_string()))?;
            sync_order_detail_to_ledger(&self.db, &row.order_id, &detail).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use db::{Db, list_raw_orders_for_account, upsert_instrument};
    use domain::{InstrumentId, OrderIntent, Side, Venue};
    use longbridge::trade::OrderDetail;

    #[tokio::test]
    async fn longbridge_order_persistence_updates_local_ledger() {
        let database = Db::connect("sqlite::memory:").await.expect("db");
        db::ensure_mvp_seed(database.pool()).await.expect("seed");
        let instrument_id = upsert_instrument(database.pool(), Venue::UsEquity.as_str(), "AAPL.US")
            .await
            .expect("instrument");
        let intent = OrderIntent {
            strategy_id: "manual_terminal".to_string(),
            instrument: InstrumentId::new(Venue::UsEquity, "AAPL.US"),
            instrument_db_id: instrument_id,
            side: Side::Buy,
            qty: 10.0,
            limit_price: 123.45,
        };

        super::persist_submitted_order_ack(
            &database,
            "acc_mvp_paper",
            &intent,
            "lb-order-1",
            "longbridge:lb-order-1",
            Some("client-1"),
        )
        .await
        .expect("persist submit");
        super::persist_cancel_ack(&database, "lb-order-1")
            .await
            .expect("persist cancel");
        super::persist_submitted_order_ack(
            &database,
            "acc_mvp_paper",
            &intent,
            "lb-order-2",
            "longbridge:lb-order-2",
            Some("client-2"),
        )
        .await
        .expect("persist submit 2");
        super::persist_amend_ack(&database, "lb-order-2", 12.0, Some(124.0))
            .await
            .expect("persist amend");

        let rows = list_raw_orders_for_account(database.pool(), "acc_mvp_paper")
            .await
            .expect("rows");
        assert_eq!(rows.len(), 2);
        let cancelled = rows
            .iter()
            .find(|row| row.id == "lb-order-1")
            .expect("cancelled");
        assert_eq!(cancelled.status, "CANCELLED");
        let amended = rows
            .iter()
            .find(|row| row.id == "lb-order-2")
            .expect("amended");
        assert_eq!(amended.status, "SUBMITTED");
        assert_eq!(amended.qty, 12.0);
        assert_eq!(amended.limit_price, Some(124.0));
        assert_eq!(amended.exchange_ref.as_deref(), Some("longbridge:lb-order-2"));
    }

    #[tokio::test]
    async fn sync_order_detail_updates_status_and_inserts_fill_delta() {
        let database = Db::connect("sqlite::memory:").await.expect("db");
        db::ensure_mvp_seed(database.pool()).await.expect("seed");
        let instrument_id = upsert_instrument(database.pool(), Venue::UsEquity.as_str(), "AAPL.US")
            .await
            .expect("instrument");
        db::insert_order(
            database.pool(),
            &db::NewOrder {
                order_id: "lb-order-fill-1",
                account_id: "acc_mvp_paper",
                instrument_id,
                side: "buy",
                qty: 10.0,
                status: "SUBMITTED",
                order_type: "limit",
                limit_price: Some(123.45),
                exchange_ref: Some("longbridge:lb-order-fill-1"),
                idempotency_key: Some("client-1"),
                created_at_ms: 100,
                updated_at_ms: 100,
            },
        )
        .await
        .expect("insert order");

        let detail: OrderDetail = serde_json::from_str(
            r#"{
                "order_id": "lb-order-fill-1",
                "status": "FilledStatus",
                "stock_name": "Apple",
                "quantity": "10",
                "executed_quantity": "10",
                "price": "123.450",
                "executed_price": "123.400",
                "submitted_at": "1680863604",
                "side": "Buy",
                "symbol": "AAPL.US",
                "order_type": "LO",
                "last_done": "123.400",
                "trigger_price": "",
                "msg": "",
                "tag": "Normal",
                "time_in_force": "Day",
                "expire_date": "",
                "updated_at": "1681113000",
                "trigger_at": "0",
                "trailing_amount": "",
                "trailing_percent": "",
                "limit_offset": "",
                "trigger_status": "NOT_USED",
                "outside_rth": "ANY_TIME",
                "currency": "USD",
                "limit_depth_level": 0,
                "trigger_count": 0,
                "monitor_price": "",
                "remark": "",
                "free_status": "None",
                "free_amount": "",
                "free_currency": "",
                "deductions_status": "NONE",
                "deductions_amount": "",
                "deductions_currency": "",
                "platform_deducted_status": "NONE",
                "platform_deducted_amount": "",
                "platform_deducted_currency": "",
                "history": [],
                "charge_detail": {
                    "items": [],
                    "total_amount": "0",
                    "currency": "USD"
                }
            }"#,
        )
        .expect("detail");

        super::sync_order_detail_to_ledger(&database, "lb-order-fill-1", &detail)
            .await
            .expect("sync");
        super::sync_order_detail_to_ledger(&database, "lb-order-fill-1", &detail)
            .await
            .expect("sync idempotent");

        let rows = list_raw_orders_for_account(database.pool(), "acc_mvp_paper")
            .await
            .expect("rows");
        assert_eq!(rows[0].status, "FILLED");
        assert_eq!(
            db::filled_qty_for_order(database.pool(), "lb-order-fill-1")
                .await
                .expect("filled qty"),
            10.0
        );
        assert_eq!(
            db::list_local_positions_for_account(database.pool(), "acc_mvp_paper")
                .await
                .expect("positions")[0]
                .net_qty,
            10.0
        );
        let expected_updated_at = 1_681_113_000_000_i64;
        assert_eq!(rows[0].updated_at_ms, expected_updated_at);
    }
}
