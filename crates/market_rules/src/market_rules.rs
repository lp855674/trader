#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use std::collections::{BTreeMap, VecDeque};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeeVolumeWindow {
    Run,
    Rolling30d,
    CalendarMonth,
}

impl FeeVolumeWindow {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Run => "run",
            Self::Rolling30d => "rolling_30d",
            Self::CalendarMonth => "calendar_month",
        }
    }
}

impl Default for FeeVolumeWindow {
    fn default() -> Self {
        Self::Run
    }
}

impl std::str::FromStr for FeeVolumeWindow {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "run" => Ok(Self::Run),
            "rolling_30d" => Ok(Self::Rolling30d),
            "calendar_month" => Ok(Self::CalendarMonth),
            other => Err(format!("invalid fee volume_window: {other}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeeRule {
    pub id: String,
    pub volume_window: FeeVolumeWindow,
    pub maker_bps: Decimal,
    pub taker_bps: Decimal,
    pub minimum_fee: Option<Decimal>,
    pub tax_bps: Option<Decimal>,
    pub exchange_fee_bps: Option<Decimal>,
    pub tiers: Vec<FeeTier>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeeBreakdown {
    pub commission: Decimal,
    pub tax: Decimal,
    pub exchange_fee: Decimal,
    pub minimum_fee_adjustment: Decimal,
    pub total: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeeRuleEngine {
    rules_by_symbol: BTreeMap<String, FeeRule>,
    volume_by_rule: BTreeMap<String, Decimal>,
    volume_entries_by_rule: BTreeMap<String, VecDeque<FeeVolumeEntry>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeeVolumeEntry {
    pub ts_ms: i64,
    pub notional: Decimal,
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
            volume_window: FeeVolumeWindow::Run,
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
        self.fee_breakdown(order_type, price, qty, volume).total
    }

    pub fn fee_breakdown(
        &self,
        order_type: OrderType,
        price: Decimal,
        qty: Decimal,
        volume: Decimal,
    ) -> FeeBreakdown {
        let notional = price * qty;
        let commission = bps_amount(notional, self.maker_taker_fee_bps(order_type, volume));
        let tax = bps_amount(notional, self.tax_bps.unwrap_or(Decimal::ZERO));
        let exchange_fee = bps_amount(notional, self.exchange_fee_bps.unwrap_or(Decimal::ZERO));
        let subtotal = commission + tax + exchange_fee;
        let total = self
            .minimum_fee
            .filter(|minimum_fee| subtotal < *minimum_fee)
            .unwrap_or(subtotal);
        FeeBreakdown {
            commission,
            tax,
            exchange_fee,
            minimum_fee_adjustment: total - subtotal,
            total: total.normalize(),
        }
    }
}

impl FeeRuleEngine {
    pub fn new(rules_by_symbol: BTreeMap<String, FeeRule>) -> Self {
        Self {
            rules_by_symbol,
            volume_by_rule: BTreeMap::new(),
            volume_entries_by_rule: BTreeMap::new(),
        }
    }

    pub fn with_volume_by_rule(
        rules_by_symbol: BTreeMap<String, FeeRule>,
        volume_by_rule: BTreeMap<String, Decimal>,
    ) -> Self {
        Self {
            rules_by_symbol,
            volume_by_rule,
            volume_entries_by_rule: BTreeMap::new(),
        }
    }

    pub fn with_volume_entries_by_rule(
        rules_by_symbol: BTreeMap<String, FeeRule>,
        volume_entries_by_rule: BTreeMap<String, Vec<FeeVolumeEntry>>,
    ) -> Self {
        let volume_by_rule = volume_entries_by_rule
            .iter()
            .map(|(rule_id, entries)| {
                (
                    rule_id.clone(),
                    entries
                        .iter()
                        .fold(Decimal::ZERO, |total, entry| total + entry.notional),
                )
            })
            .collect();
        Self {
            rules_by_symbol,
            volume_by_rule,
            volume_entries_by_rule: volume_entries_by_rule
                .into_iter()
                .map(|(rule_id, mut entries)| {
                    entries.sort_by_key(|entry| entry.ts_ms);
                    (rule_id, entries.into())
                })
                .collect(),
        }
    }

    pub fn apply_fill(
        &mut self,
        symbol: &str,
        order_type: OrderType,
        price: Decimal,
        qty: Decimal,
    ) -> Option<FeeBreakdown> {
        self.apply_fill_at(symbol, order_type, price, qty, 0)
    }

    pub fn apply_fill_at(
        &mut self,
        symbol: &str,
        order_type: OrderType,
        price: Decimal,
        qty: Decimal,
        ts_ms: i64,
    ) -> Option<FeeBreakdown> {
        let rule = self.rules_by_symbol.get(symbol)?.clone();
        let rule_id = rule.id.clone();
        let volume_window = rule.volume_window;
        self.prune_volume_window(&rule_id, volume_window, ts_ms);
        let volume = self
            .volume_by_rule
            .get(&rule_id)
            .copied()
            .unwrap_or(Decimal::ZERO);
        let breakdown = rule.fee_breakdown(order_type, price, qty, volume);
        let fill_notional = price * qty;
        self.volume_by_rule
            .entry(rule_id.clone())
            .and_modify(|current| *current += fill_notional)
            .or_insert(fill_notional);
        self.volume_entries_by_rule
            .entry(rule_id)
            .or_default()
            .push_back(FeeVolumeEntry {
                ts_ms,
                notional: fill_notional,
            });
        Some(breakdown)
    }

    pub fn volume_for_rule(&self, rule_id: &str) -> Decimal {
        self.volume_by_rule
            .get(rule_id)
            .copied()
            .unwrap_or(Decimal::ZERO)
    }

    fn prune_volume_window(
        &mut self,
        rule_id: &str,
        volume_window: FeeVolumeWindow,
        as_of_ms: i64,
    ) {
        match volume_window {
            FeeVolumeWindow::Run => {}
            FeeVolumeWindow::Rolling30d => {
                const ROLLING_30D_MS: i64 = 30 * 24 * 60 * 60 * 1_000;
                self.prune_entries_before(rule_id, as_of_ms.saturating_sub(ROLLING_30D_MS));
            }
            FeeVolumeWindow::CalendarMonth => {
                self.prune_entries_before(rule_id, utc_month_start_ms(as_of_ms));
            }
        }
    }

    fn prune_entries_before(&mut self, rule_id: &str, from_ms: i64) {
        let Some(entries) = self.volume_entries_by_rule.get_mut(rule_id) else {
            return;
        };
        let mut removed = Decimal::ZERO;
        while entries.front().is_some_and(|entry| entry.ts_ms < from_ms) {
            if let Some(entry) = entries.pop_front() {
                removed += entry.notional;
            }
        }
        if removed == Decimal::ZERO {
            return;
        }
        let current = self
            .volume_by_rule
            .get(rule_id)
            .copied()
            .unwrap_or(Decimal::ZERO);
        self.volume_by_rule
            .insert(rule_id.to_string(), (current - removed).max(Decimal::ZERO));
    }
}

fn utc_month_start_ms(ts_ms: i64) -> i64 {
    const MS_PER_DAY: i64 = 86_400_000;
    let days = ts_ms.div_euclid(MS_PER_DAY);
    let (year, month, _) = civil_from_days(days);
    days_from_civil(year, month, 1) * MS_PER_DAY
}

fn civil_from_days(days_since_unix_epoch: i64) -> (i32, u32, u32) {
    let z = days_since_unix_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = y + i64::from(month <= 2);
    (year as i32, month as u32, day as u32)
}

fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = i64::from(year) - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let month = i64::from(month);
    let day = i64::from(day);
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
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

fn bps_amount(notional: Decimal, bps: Decimal) -> Decimal {
    notional * bps / Decimal::from(10_000)
}
