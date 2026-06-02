#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use thiserror::Error;
use trader_core::{OrderRequest, OrderType};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MarketRuleError {
    #[error("quantity is below minimum quantity")]
    MinQuantity,
    #[error("quantity is not a multiple of lot size")]
    InvalidLotSize,
    #[error("price is not a multiple of tick size")]
    InvalidTickSize,
    #[error("order notional is below minimum notional")]
    MinNotional,
    #[error("market orders are not allowed")]
    MarketOrdersDisabled,
    #[error("reference price must be positive")]
    InvalidReferencePrice,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketRuleSet {
    pub lot_size: Decimal,
    pub tick_size: Decimal,
    pub min_qty: Decimal,
    pub min_notional: Decimal,
    pub allow_market_orders: bool,
}

impl MarketRuleSet {
    pub fn us_equity() -> Self {
        Self {
            lot_size: Decimal::ONE,
            tick_size: Decimal::new(1, 2),
            min_qty: Decimal::ONE,
            min_notional: Decimal::ZERO,
            allow_market_orders: true,
        }
    }

    pub fn validate_order(
        &self,
        order: &OrderRequest,
        reference_price: Decimal,
    ) -> Result<(), MarketRuleError> {
        if reference_price <= Decimal::ZERO {
            return Err(MarketRuleError::InvalidReferencePrice);
        }
        if order.order_type == OrderType::Market && !self.allow_market_orders {
            return Err(MarketRuleError::MarketOrdersDisabled);
        }
        if !is_multiple(order.qty, self.lot_size) {
            return Err(MarketRuleError::InvalidLotSize);
        }
        if order.qty < self.min_qty {
            return Err(MarketRuleError::MinQuantity);
        }
        if let Some(price) = order.price
            && !is_multiple(price, self.tick_size)
        {
            return Err(MarketRuleError::InvalidTickSize);
        }

        let price = order.price.unwrap_or(reference_price);
        if price * order.qty < self.min_notional {
            return Err(MarketRuleError::MinNotional);
        }
        Ok(())
    }
}

fn is_multiple(value: Decimal, step: Decimal) -> bool {
    if step <= Decimal::ZERO {
        return false;
    }
    value % step == Decimal::ZERO
}
