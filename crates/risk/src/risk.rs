#![forbid(unsafe_code)]

use portfolio::TargetPosition;
use rust_decimal::Decimal;
use thiserror::Error;
use trader_core::{OrderRequest, OrderSide};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RiskError {
    #[error("target quantity exceeds max position")]
    MaxPosition,
    #[error("order quantity exceeds max order quantity")]
    MaxOrderQuantity,
    #[error("order notional exceeds max order notional")]
    MaxOrderNotional,
    #[error("buy order requires more cash than available")]
    InsufficientCash,
    #[error("trading is halted")]
    TradingHalted,
}

pub fn check_max_position(target: &TargetPosition, max_abs_qty: Decimal) -> Result<(), RiskError> {
    if target.target_qty.abs() > max_abs_qty {
        return Err(RiskError::MaxPosition);
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RiskPolicy {
    pub max_order_qty: Decimal,
    pub max_order_notional: Decimal,
    pub min_cash_after_order: Decimal,
}

impl RiskPolicy {
    pub fn new(
        max_order_qty: Decimal,
        max_order_notional: Decimal,
        min_cash_after_order: Decimal,
    ) -> Self {
        Self {
            max_order_qty,
            max_order_notional,
            min_cash_after_order,
        }
    }

    pub fn check_order(
        &self,
        order: &OrderRequest,
        reference_price: Decimal,
        available_cash: Decimal,
        trading_halted: bool,
    ) -> Result<(), RiskError> {
        if trading_halted {
            return Err(RiskError::TradingHalted);
        }
        if order.qty > self.max_order_qty {
            return Err(RiskError::MaxOrderQuantity);
        }
        let notional = order.qty * order.price.unwrap_or(reference_price);
        if notional > self.max_order_notional {
            return Err(RiskError::MaxOrderNotional);
        }
        if order.side == OrderSide::Buy && available_cash - notional < self.min_cash_after_order {
            return Err(RiskError::InsufficientCash);
        }
        Ok(())
    }
}
