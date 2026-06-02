#![forbid(unsafe_code)]

use portfolio::TargetPosition;
use rust_decimal::Decimal;
use trader_core::{OrderRequest, OrderSide, OrderType};

pub fn immediate_order(target: &TargetPosition, account_id: impl Into<String>) -> OrderRequest {
    let side = if target.target_qty >= Decimal::ZERO {
        OrderSide::Buy
    } else {
        OrderSide::Sell
    };
    OrderRequest {
        symbol: target.symbol.clone(),
        side,
        order_type: OrderType::Market,
        qty: target.target_qty.abs(),
        price: None,
        account_id: account_id.into(),
    }
}
