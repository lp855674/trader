#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use std::collections::BTreeMap;
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

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ContractRiskError {
    #[error("contract leverage exceeds max leverage")]
    MaxLeverage,
    #[error("contract margin ratio is below minimum")]
    InsufficientMargin,
    #[error("contract position notional exceeds max notional")]
    MaxPositionNotional,
    #[error("contract liquidation buffer is below minimum")]
    LiquidationBuffer,
    #[error("contract funding rate is outside allowed bounds")]
    FundingRateBounds,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractRiskLimits {
    pub max_leverage: Decimal,
    pub min_margin_ratio: Decimal,
    pub max_position_notional: Decimal,
    pub liquidation_buffer_bps: Decimal,
    pub max_abs_funding_rate: Decimal,
}

impl ContractRiskLimits {
    pub fn crypto_perp() -> Self {
        Self {
            max_leverage: Decimal::from(125),
            min_margin_ratio: Decimal::new(105, 2),
            max_position_notional: Decimal::from(10_000_000),
            liquidation_buffer_bps: Decimal::from(100),
            max_abs_funding_rate: Decimal::new(1, 2),
        }
    }

    pub fn crypto_future() -> Self {
        Self::crypto_perp()
    }

    pub fn for_symbol(symbol: &str) -> Option<Self> {
        let mut parts = symbol.split(':');
        let market = parts.next();
        let _exchange = parts.next();
        let _code = parts.next();
        let asset_class = parts.next();
        if parts.next().is_some() {
            return None;
        }
        match (market, asset_class) {
            (Some("CRYPTO"), Some("CRYPTO_PERP")) => Some(Self::crypto_perp()),
            (Some("CRYPTO"), Some("CRYPTO_FUTURE")) => Some(Self::crypto_future()),
            _ => None,
        }
    }

    pub fn validate(
        &self,
        leverage: Decimal,
        position_notional: Decimal,
        margin_ratio: Decimal,
        liquidation_buffer_bps: Decimal,
        funding_rate: Decimal,
    ) -> Result<(), ContractRiskError> {
        if leverage > self.max_leverage {
            return Err(ContractRiskError::MaxLeverage);
        }
        if margin_ratio < self.min_margin_ratio {
            return Err(ContractRiskError::InsufficientMargin);
        }
        if position_notional > self.max_position_notional {
            return Err(ContractRiskError::MaxPositionNotional);
        }
        if liquidation_buffer_bps < self.liquidation_buffer_bps {
            return Err(ContractRiskError::LiquidationBuffer);
        }
        if funding_rate.abs() > self.max_abs_funding_rate {
            return Err(ContractRiskError::FundingRateBounds);
        }
        Ok(())
    }
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

pub trait MarketRuleProvider {
    fn rules_for_symbol(&self, symbol: &str) -> Result<MarketRuleSet, MarketRuleError>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct StaticMarketRuleProvider;

impl MarketRuleProvider for StaticMarketRuleProvider {
    fn rules_for_symbol(&self, symbol: &str) -> Result<MarketRuleSet, MarketRuleError> {
        MarketRuleSet::for_symbol(symbol)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ConfiguredMarketRuleProvider {
    rules_by_symbol: BTreeMap<String, MarketRuleSet>,
    fallback: StaticMarketRuleProvider,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiquidityRole {
    Maker,
    Taker,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeeRule {
    pub id: String,
    pub maker_bps: Decimal,
    pub taker_bps: Decimal,
    pub minimum_fee: Option<Decimal>,
    pub tax_bps: Option<Decimal>,
    pub exchange_fee_bps: Option<Decimal>,
    pub tiers: Vec<FeeTier>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeeTier {
    pub volume_from: Decimal,
    pub volume_to: Option<Decimal>,
    pub maker_bps: Decimal,
    pub taker_bps: Decimal,
}

impl FeeTier {
    pub fn contains(self, volume: Decimal) -> bool {
        volume >= self.volume_from && self.volume_to.is_none_or(|volume_to| volume < volume_to)
    }
}

impl FeeRule {
    pub fn flat(id: impl Into<String>, maker_bps: Decimal, taker_bps: Decimal) -> Self {
        Self {
            id: id.into(),
            maker_bps,
            taker_bps,
            minimum_fee: None,
            tax_bps: None,
            exchange_fee_bps: None,
            tiers: Vec::new(),
        }
    }

    pub fn maker_taker_fee_bps(&self, order_type: OrderType, volume: Decimal) -> Decimal {
        let (maker_bps, taker_bps) = self
            .tiers
            .iter()
            .copied()
            .find(|tier| tier.contains(volume))
            .map(|tier| (tier.maker_bps, tier.taker_bps))
            .unwrap_or((self.maker_bps, self.taker_bps));
        match liquidity_role_for_order_type(order_type) {
            LiquidityRole::Maker => maker_bps,
            LiquidityRole::Taker => taker_bps,
        }
    }

    pub fn total_fee_bps(&self, order_type: OrderType, volume: Decimal) -> Decimal {
        self.maker_taker_fee_bps(order_type, volume)
            + self.tax_bps.unwrap_or(Decimal::ZERO)
            + self.exchange_fee_bps.unwrap_or(Decimal::ZERO)
    }

    pub fn fee(
        &self,
        order_type: OrderType,
        price: Decimal,
        qty: Decimal,
        volume: Decimal,
    ) -> Decimal {
        let fee = price * qty * self.total_fee_bps(order_type, volume) / Decimal::from(10_000);
        self.minimum_fee
            .filter(|minimum_fee| fee < *minimum_fee)
            .unwrap_or(fee)
    }
}

pub fn liquidity_role_for_order_type(order_type: OrderType) -> LiquidityRole {
    match order_type {
        OrderType::Market | OrderType::Stop => LiquidityRole::Taker,
        OrderType::Limit | OrderType::StopLimit | OrderType::PostOnly => LiquidityRole::Maker,
    }
}

impl ConfiguredMarketRuleProvider {
    pub fn new(rules_by_symbol: BTreeMap<String, MarketRuleSet>) -> Self {
        Self {
            rules_by_symbol,
            fallback: StaticMarketRuleProvider,
        }
    }

    pub fn insert(&mut self, symbol: impl Into<String>, rules: MarketRuleSet) {
        self.rules_by_symbol.insert(symbol.into(), rules);
    }
}

impl MarketRuleProvider for ConfiguredMarketRuleProvider {
    fn rules_for_symbol(&self, symbol: &str) -> Result<MarketRuleSet, MarketRuleError> {
        self.rules_by_symbol
            .get(symbol)
            .cloned()
            .map(Ok)
            .unwrap_or_else(|| self.fallback.rules_for_symbol(symbol))
    }
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
