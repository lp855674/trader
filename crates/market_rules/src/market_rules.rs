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
    #[error("unsupported symbol {0}")]
    UnsupportedSymbol(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketRuleSet {
    pub lot_size: Decimal,
    pub tick_size: Decimal,
    pub min_qty: Decimal,
    pub min_notional: Decimal,
    pub allow_market_orders: bool,
    pub initial_margin_rate: Decimal,
}

impl MarketRuleSet {
    pub fn cn_equity() -> Self {
        Self {
            lot_size: Decimal::from(100),
            tick_size: Decimal::new(1, 2),
            min_qty: Decimal::from(100),
            min_notional: Decimal::ZERO,
            allow_market_orders: true,
            initial_margin_rate: Decimal::ZERO,
        }
    }

    pub fn hk_equity() -> Self {
        Self {
            lot_size: Decimal::from(100),
            tick_size: Decimal::new(1, 3),
            min_qty: Decimal::from(100),
            min_notional: Decimal::ZERO,
            allow_market_orders: true,
            initial_margin_rate: Decimal::ZERO,
        }
    }

    pub fn us_equity() -> Self {
        Self {
            lot_size: Decimal::ONE,
            tick_size: Decimal::new(1, 2),
            min_qty: Decimal::ONE,
            min_notional: Decimal::ZERO,
            allow_market_orders: true,
            initial_margin_rate: Decimal::ZERO,
        }
    }

    pub fn crypto_spot() -> Self {
        Self {
            lot_size: Decimal::new(1, 6),
            tick_size: Decimal::new(1, 2),
            min_qty: Decimal::new(1, 6),
            min_notional: Decimal::from(10),
            allow_market_orders: true,
            initial_margin_rate: Decimal::ZERO,
        }
    }

    pub fn crypto_perp() -> Self {
        Self {
            lot_size: Decimal::new(1, 3),
            tick_size: Decimal::new(1, 2),
            min_qty: Decimal::new(1, 3),
            min_notional: Decimal::from(5),
            allow_market_orders: true,
            initial_margin_rate: Decimal::new(1, 1),
        }
    }

    pub fn crypto_future() -> Self {
        Self::crypto_perp()
    }

    pub fn for_symbol(symbol: &str) -> Result<Self, MarketRuleError> {
        let mut parts = symbol.split(':');
        let market = parts.next();
        let _exchange = parts.next();
        let _code = parts.next();
        let asset_class = parts.next();
        if parts.next().is_some() {
            return Err(MarketRuleError::UnsupportedSymbol(symbol.to_string()));
        }

        match (market, asset_class) {
            (Some("CN"), Some("EQUITY")) => Ok(Self::cn_equity()),
            (Some("HK"), Some("EQUITY")) => Ok(Self::hk_equity()),
            (Some("US"), Some("EQUITY")) => Ok(Self::us_equity()),
            (Some("CRYPTO"), Some("CRYPTO_SPOT")) => Ok(Self::crypto_spot()),
            (Some("CRYPTO"), Some("CRYPTO_PERP")) => Ok(Self::crypto_perp()),
            (Some("CRYPTO"), Some("CRYPTO_FUTURE")) => Ok(Self::crypto_future()),
            _ => Err(MarketRuleError::UnsupportedSymbol(symbol.to_string())),
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
