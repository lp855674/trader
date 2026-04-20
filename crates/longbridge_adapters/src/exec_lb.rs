use std::sync::Arc;

use async_trait::async_trait;
use domain::{OrderIntent, Side};
use exec::{ExecError, ExecutionAdapter, ManualOrderAck, OrderAck};
use longbridge::trade::{
    OrderSide, OrderType, ReplaceOrderOptions, SubmitOrderOptions, TimeInForceType, TradeContext,
};
use rust_decimal::Decimal;

/// 通过 Longbridge **实盘**限价单（`LO`）。  
/// 虚拟成交请仍使用 `PaperAdapter`；本适配器用于 `acc_lb_live` 等账户。
pub struct LongbridgeTradeAdapter {
    trade: Arc<TradeContext>,
}

impl LongbridgeTradeAdapter {
    pub fn new(trade: Arc<TradeContext>) -> Self {
        Self { trade }
    }
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
        _account_id: &str,
        intent: &OrderIntent,
        _idempotency_key: Option<&str>,
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

        Ok(OrderAck {
            order_id: resp.order_id.clone(),
            exchange_ref: format!("longbridge:{}", resp.order_id),
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

    async fn cancel_order(&self, _account_id: &str, order_id: &str) -> Result<(), ExecError> {
        self.trade
            .cancel_order(order_id.to_string())
            .await
            .map_err(|error| ExecError::Longbridge(error.to_string()))?;
        Ok(())
    }

    async fn amend_order(
        &self,
        _account_id: &str,
        order_id: &str,
        qty: f64,
        limit_price: Option<f64>,
    ) -> Result<ManualOrderAck, ExecError> {
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
        Ok(ManualOrderAck {
            order_id: order_id.to_string(),
            exchange_ref: Some(format!("longbridge:{order_id}")),
            status: "SUBMITTED".to_string(),
        })
    }
}
