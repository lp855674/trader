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

pub fn order_for_target_delta(
    target: &TargetPosition,
    current_qty: Decimal,
    account_id: impl Into<String>,
) -> Option<OrderRequest> {
    let delta = target.target_qty - current_qty;
    if delta == Decimal::ZERO {
        return None;
    }
    let side = if delta > Decimal::ZERO {
        OrderSide::Buy
    } else {
        OrderSide::Sell
    };
    Some(OrderRequest {
        symbol: target.symbol.clone(),
        side,
        order_type: OrderType::Market,
        qty: delta.abs(),
        price: None,
        account_id: account_id.into(),
    })
}
