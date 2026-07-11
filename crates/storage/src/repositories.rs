use std::{
    collections::{BTreeMap, BTreeSet},
    str::FromStr,
};

use crate::{Db, StorageError, StorageResult};
use chrono::{Datelike, TimeZone, Utc};
use events::{
    AnyEventEnvelope, EventBus, EventCategory, EventEnvelope, LogSink, LogSinkError, RuntimeEvent,
    StructuredLogEntry, TraderEvent,
};
use market_rules::{FeeRule, FeeTier, FeeVolumeEntry, FeeVolumeWindow};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::{QueryBuilder, Row, Sqlite};
use trader_core::OrderRequest;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewInstrument {
    pub symbol: String,
    pub market: String,
    pub exchange: String,
    pub asset_class: String,
    pub currency: String,
    pub lot_size: String,
    pub tick_size: String,
    pub tradable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstrumentRecord {
    pub symbol: String,
    pub market: String,
    pub exchange: String,
    pub asset_class: String,
    pub currency: String,
    pub lot_size: String,
    pub tick_size: String,
    pub tradable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewLotSizeRule {
    pub id: String,
    pub market: String,
    pub exchange: String,
    pub asset_class: String,
    pub symbol: Option<String>,
    pub lot_size: String,
    pub min_qty: String,
    pub min_notional: String,
    pub effective_from_ms: i64,
    pub effective_to_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LotSizeRuleCommand {
    pub id: String,
    pub market: String,
    pub exchange: String,
    pub asset_class: String,
    pub symbol: Option<String>,
    pub lot_size: String,
    pub min_qty: String,
    pub min_notional: String,
    pub effective_from_ms: i64,
    pub effective_to_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredLotSizeRule {
    pub id: String,
    pub market: String,
    pub exchange: String,
    pub asset_class: String,
    pub symbol: Option<String>,
    pub lot_size: String,
    pub min_qty: String,
    pub min_notional: String,
    pub effective_from_ms: i64,
    pub effective_to_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewPriceLimitRule {
    pub id: String,
    pub market: String,
    pub exchange: String,
    pub asset_class: String,
    pub symbol: Option<String>,
    pub tick_size: String,
    pub limit_up_bps: Option<String>,
    pub limit_down_bps: Option<String>,
    pub effective_from_ms: i64,
    pub effective_to_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredPriceLimitRule {
    pub id: String,
    pub market: String,
    pub exchange: String,
    pub asset_class: String,
    pub symbol: Option<String>,
    pub tick_size: String,
    pub limit_up_bps: Option<String>,
    pub limit_down_bps: Option<String>,
    pub effective_from_ms: i64,
    pub effective_to_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriceLimitRuleCommand {
    pub id: String,
    pub market: String,
    pub exchange: String,
    pub asset_class: String,
    pub symbol: Option<String>,
    pub tick_size: String,
    pub limit_up_bps: Option<String>,
    pub limit_down_bps: Option<String>,
    pub effective_from_ms: i64,
    pub effective_to_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewFeeRule {
    pub id: String,
    pub market: String,
    pub exchange: String,
    pub asset_class: String,
    pub symbol: Option<String>,
    pub volume_window: String,
    pub maker_bps: String,
    pub taker_bps: String,
    pub minimum_fee: Option<String>,
    pub tax_bps: Option<String>,
    pub exchange_fee_bps: Option<String>,
    pub effective_from_ms: i64,
    pub effective_to_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredFeeRule {
    pub id: String,
    pub market: String,
    pub exchange: String,
    pub asset_class: String,
    pub symbol: Option<String>,
    pub volume_window: String,
    pub maker_bps: String,
    pub taker_bps: String,
    pub minimum_fee: Option<String>,
    pub tax_bps: Option<String>,
    pub exchange_fee_bps: Option<String>,
    pub effective_from_ms: i64,
    pub effective_to_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewFeeRuleWithTiers {
    pub rule: NewFeeRule,
    pub tiers: Vec<NewFeeRuleTier>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredFeeRuleWithTiers {
    pub rule: StoredFeeRule,
    pub tiers: Vec<StoredFeeRuleTier>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewFeeRuleTier {
    pub id: String,
    pub fee_rule_id: String,
    pub volume_from: String,
    pub volume_to: Option<String>,
    pub maker_bps: String,
    pub taker_bps: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredFeeRuleTier {
    pub id: String,
    pub fee_rule_id: String,
    pub volume_from: String,
    pub volume_to: Option<String>,
    pub maker_bps: String,
    pub taker_bps: String,
}

fn fee_rule_with_tiers(rule_with_tiers: StoredFeeRuleWithTiers) -> StorageResult<FeeRule> {
    let rule = rule_with_tiers.rule;
    let tiers = rule_with_tiers
        .tiers
        .into_iter()
        .map(|tier| {
            Ok(FeeTier {
                volume_from: parse_rule_decimal("volume_from", &tier.volume_from)?,
                volume_to: tier
                    .volume_to
                    .as_deref()
                    .map(|value| parse_rule_decimal("volume_to", value))
                    .transpose()?,
                maker_bps: parse_rule_decimal("tier_maker_bps", &tier.maker_bps)?,
                taker_bps: parse_rule_decimal("tier_taker_bps", &tier.taker_bps)?,
            })
        })
        .collect::<StorageResult<Vec<_>>>()?;

    Ok(FeeRule {
        id: rule.id,
        volume_window: parse_volume_window(&rule.volume_window)?,
        maker_bps: parse_rule_decimal("maker_bps", &rule.maker_bps)?,
        taker_bps: parse_rule_decimal("taker_bps", &rule.taker_bps)?,
        minimum_fee: rule
            .minimum_fee
            .as_deref()
            .map(|value| parse_rule_decimal("minimum_fee", value))
            .transpose()?,
        tax_bps: rule
            .tax_bps
            .as_deref()
            .map(|value| parse_rule_decimal("tax_bps", value))
            .transpose()?,
        exchange_fee_bps: rule
            .exchange_fee_bps
            .as_deref()
            .map(|value| parse_rule_decimal("exchange_fee_bps", value))
            .transpose()?,
        tiers,
    })
}

fn parse_rule_decimal(field: &str, value: &str) -> StorageResult<Decimal> {
    Decimal::from_str(value)
        .map_err(|error| StorageError::Protocol(format!("invalid {field} {value}: {error}")))
}

fn parse_volume_window(value: &str) -> StorageResult<FeeVolumeWindow> {
    value
        .parse()
        .map_err(|error: String| StorageError::Protocol(error))
}

fn market_rule_symbol_parts(symbol: &str) -> Option<(String, String, String)> {
    let mut parts = symbol.split(':');
    let market = parts.next()?;
    let exchange = parts.next()?;
    let _code = parts.next()?;
    let asset_class = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    Some((
        market.to_string(),
        exchange.to_string(),
        asset_class.to_string(),
    ))
}

fn fee_volume_symbol_matches_scope(
    symbol: &str,
    market: &str,
    exchange: &str,
    asset_class: &str,
) -> bool {
    let Some((symbol_market, symbol_exchange, symbol_asset_class)) =
        market_rule_symbol_parts(symbol)
    else {
        return false;
    };
    symbol_market == market
        && symbol_exchange == exchange
        && (asset_class == "*" || symbol_asset_class == asset_class)
}

fn fee_volume_seed_window(
    volume_window: FeeVolumeWindow,
    as_of_ms: i64,
) -> StorageResult<Option<(i64, i64)>> {
    match volume_window {
        FeeVolumeWindow::Run => Ok(None),
        FeeVolumeWindow::Rolling30d => {
            const ROLLING_30D_MS: i64 = 30 * 24 * 60 * 60 * 1_000;
            Ok(Some((as_of_ms.saturating_sub(ROLLING_30D_MS), as_of_ms)))
        }
        FeeVolumeWindow::CalendarMonth => {
            let as_of = Utc
                .timestamp_millis_opt(as_of_ms)
                .single()
                .ok_or_else(|| StorageError::Protocol(format!("invalid as_of_ms {as_of_ms}")))?;
            let month_start = Utc
                .with_ymd_and_hms(as_of.year(), as_of.month(), 1, 0, 0, 0)
                .single()
                .ok_or_else(|| {
                    StorageError::Protocol(format!("invalid calendar month for {as_of_ms}"))
                })?
                .timestamp_millis();
            Ok(Some((month_start, as_of_ms)))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewMarketCalendar {
    pub id: String,
    pub market: String,
    pub trading_day: String,
    pub is_open: bool,
    pub session_template: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredMarketCalendar {
    pub id: String,
    pub market: String,
    pub trading_day: String,
    pub is_open: bool,
    pub session_template: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewTradingSessionRule {
    pub id: String,
    pub market: String,
    pub trading_day: String,
    pub session_name: String,
    pub open_time: String,
    pub close_time: String,
    pub timezone: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredTradingSessionRule {
    pub id: String,
    pub market: String,
    pub trading_day: String,
    pub session_name: String,
    pub open_time: String,
    pub close_time: String,
    pub timezone: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewCryptoPosition {
    pub run_id: String,
    pub account_id: String,
    pub exchange: String,
    pub symbol: String,
    pub asset_class: String,
    pub margin_mode: String,
    pub position_side: String,
    pub leverage: String,
    pub qty: String,
    pub avg_price: String,
    pub margin_used: String,
    pub funding_fee: String,
    pub realized_pnl: String,
    pub unrealized_pnl: String,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredCryptoPosition {
    pub run_id: String,
    pub account_id: String,
    pub exchange: String,
    pub symbol: String,
    pub asset_class: String,
    pub margin_mode: String,
    pub position_side: String,
    pub leverage: String,
    pub qty: String,
    pub avg_price: String,
    pub margin_used: String,
    pub funding_fee: String,
    pub realized_pnl: String,
    pub unrealized_pnl: String,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewFundingRate {
    pub id: String,
    pub exchange: String,
    pub symbol: String,
    pub funding_time_ms: i64,
    pub funding_rate: String,
    pub mark_price: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredFundingRate {
    pub id: String,
    pub exchange: String,
    pub symbol: String,
    pub funding_time_ms: i64,
    pub funding_rate: String,
    pub mark_price: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewCryptoMarketMeta {
    pub exchange: String,
    pub symbol: String,
    pub base_asset: String,
    pub quote_asset: String,
    pub instrument_type: String,
    pub contract_type: Option<String>,
    pub contract_size: Option<String>,
    pub settlement_asset: Option<String>,
    pub min_notional: Option<String>,
    pub min_qty: Option<String>,
    pub max_qty: Option<String>,
    pub price_precision: Option<i64>,
    pub qty_precision: Option<i64>,
    pub price_tick: Option<String>,
    pub qty_step: Option<String>,
    pub maker_fee_rate: Option<String>,
    pub taker_fee_rate: Option<String>,
    pub funding_interval_hours: Option<i64>,
    pub max_leverage: Option<String>,
    pub margin_modes: Option<String>,
    pub is_inverse: bool,
    pub is_active: bool,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredCryptoMarketMeta {
    pub id: i64,
    pub exchange: String,
    pub symbol: String,
    pub base_asset: String,
    pub quote_asset: String,
    pub instrument_type: String,
    pub contract_type: Option<String>,
    pub contract_size: Option<String>,
    pub settlement_asset: Option<String>,
    pub min_notional: Option<String>,
    pub min_qty: Option<String>,
    pub max_qty: Option<String>,
    pub price_precision: Option<i64>,
    pub qty_precision: Option<i64>,
    pub price_tick: Option<String>,
    pub qty_step: Option<String>,
    pub maker_fee_rate: Option<String>,
    pub taker_fee_rate: Option<String>,
    pub funding_interval_hours: Option<i64>,
    pub max_leverage: Option<String>,
    pub margin_modes: Option<String>,
    pub is_inverse: bool,
    pub is_active: bool,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewCorporateActionMeta {
    pub market: String,
    pub exchange: String,
    pub symbol: String,
    pub action_type: String,
    pub ex_date_ms: i64,
    pub record_date_ms: Option<i64>,
    pub payable_date_ms: Option<i64>,
    pub ratio: Option<String>,
    pub cash_amount: Option<String>,
    pub currency: Option<String>,
    pub source: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredCorporateActionMeta {
    pub id: i64,
    pub market: String,
    pub exchange: String,
    pub symbol: String,
    pub action_type: String,
    pub ex_date_ms: i64,
    pub record_date_ms: Option<i64>,
    pub payable_date_ms: Option<i64>,
    pub ratio: Option<String>,
    pub cash_amount: Option<String>,
    pub currency: Option<String>,
    pub source: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewCashSnapshot {
    pub run_id: String,
    pub ts_ms: i64,
    pub currency: String,
    pub cash: String,
    pub available_cash: String,
    pub frozen_cash: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BrokerAccountBalanceCommand {
    pub run_id: String,
    pub account_id: String,
    pub broker_kind: String,
    pub ts_ms: i64,
    pub currency: String,
    pub cash: Decimal,
    pub available_cash: Decimal,
    pub frozen_cash: Decimal,
    pub equity: Option<Decimal>,
    pub buying_power: Option<Decimal>,
    pub margin_used: Option<Decimal>,
    pub source_ts_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredBrokerAccountBalance {
    pub id: i64,
    pub run_id: String,
    pub account_id: String,
    pub broker_kind: String,
    pub ts_ms: i64,
    pub currency: String,
    pub cash: String,
    pub available_cash: String,
    pub frozen_cash: String,
    pub equity: Option<String>,
    pub buying_power: Option<String>,
    pub margin_used: Option<String>,
    pub source_ts_ms: i64,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredCashSnapshot {
    pub id: i64,
    pub run_id: String,
    pub ts_ms: i64,
    pub currency: String,
    pub cash: String,
    pub available_cash: String,
    pub frozen_cash: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewPositionSnapshot {
    pub run_id: String,
    pub ts_ms: i64,
    pub market: String,
    pub exchange: String,
    pub symbol: String,
    pub asset_class: String,
    pub position_side: Option<String>,
    pub qty: String,
    pub available_qty: String,
    pub avg_price: Option<String>,
    pub entry_price: Option<String>,
    pub market_price: Option<String>,
    pub mark_price: Option<String>,
    pub market_value: Option<String>,
    pub unrealized_pnl: Option<String>,
    pub realized_pnl: Option<String>,
    pub currency: String,
    pub contract_metadata_json: Option<String>,
    pub liquidation_price: Option<String>,
    pub open_interest: Option<String>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredPositionSnapshot {
    pub id: i64,
    pub run_id: String,
    pub ts_ms: i64,
    pub market: String,
    pub exchange: String,
    pub symbol: String,
    pub asset_class: String,
    pub position_side: Option<String>,
    pub qty: String,
    pub available_qty: String,
    pub avg_price: Option<String>,
    pub entry_price: Option<String>,
    pub market_price: Option<String>,
    pub mark_price: Option<String>,
    pub market_value: Option<String>,
    pub unrealized_pnl: Option<String>,
    pub realized_pnl: Option<String>,
    pub currency: String,
    pub contract_metadata_json: Option<String>,
    pub liquidation_price: Option<String>,
    pub open_interest: Option<String>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReconciliationAuditCommand {
    pub id: String,
    pub run_id: String,
    pub account_id: String,
    pub broker_kind: String,
    pub ts_ms: i64,
    pub severity: String,
    pub cash_drift_count: i64,
    pub position_drift_count: i64,
    pub open_order_drift_count: i64,
    pub execution_drift_count: i64,
    pub stale_input_count: i64,
    pub payload_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredReconciliationAudit {
    pub id: String,
    pub run_id: String,
    pub account_id: String,
    pub broker_kind: String,
    pub ts_ms: i64,
    pub severity: String,
    pub cash_drift_count: i64,
    pub position_drift_count: i64,
    pub open_order_drift_count: i64,
    pub execution_drift_count: i64,
    pub stale_input_count: i64,
    pub payload_json: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewConfigRecord {
    pub id: String,
    pub name: String,
    pub config_type: String,
    pub content: String,
    pub format: String,
    pub checksum: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConfigRecordCommand {
    pub id: String,
    pub name: String,
    pub config_type: String,
    pub content: String,
    pub format: String,
    pub checksum: Option<String>,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredConfigRecord {
    pub id: String,
    pub name: String,
    pub config_type: String,
    pub content: String,
    pub format: String,
    pub checksum: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewConfigVersion {
    pub name: String,
    pub content_json: String,
    pub created_by: String,
    pub parent_version: Option<u32>,
    pub target_env: Option<String>,
    pub rollout: Option<String>,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigState {
    Draft,
    PendingReview,
    Approved,
    Published,
    Archived,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigActorRole {
    ReleaseManager,
    Approver,
    Viewer,
    Unknown,
}

impl ConfigActorRole {
    fn as_str(self) -> &'static str {
        match self {
            Self::ReleaseManager => "release_manager",
            Self::Approver => "approver",
            Self::Viewer => "viewer",
            Self::Unknown => "unknown",
        }
    }

    fn can_satisfy(self, required_role: &str) -> bool {
        self.as_str() == required_role
    }
}

impl FromStr for ConfigActorRole {
    type Err = StorageError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "release_manager" | "release" => Ok(Self::ReleaseManager),
            "approver" | "reviewer" | "risk_owner" | "risk-owner" => Ok(Self::Approver),
            "viewer" => Ok(Self::Viewer),
            _ => Ok(Self::Unknown),
        }
    }
}

impl ConfigState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::PendingReview => "pending_review",
            Self::Approved => "approved",
            Self::Published => "published",
            Self::Archived => "archived",
        }
    }

    fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Draft, Self::PendingReview)
                | (Self::Draft, Self::Archived)
                | (Self::PendingReview, Self::Approved)
                | (Self::Approved, Self::Published)
                | (Self::Approved, Self::Archived)
                | (Self::Published, Self::Archived)
        )
    }
}

impl FromStr for ConfigState {
    type Err = StorageError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "draft" => Ok(Self::Draft),
            "pending_review" => Ok(Self::PendingReview),
            "approved" => Ok(Self::Approved),
            "published" => Ok(Self::Published),
            "archived" => Ok(Self::Archived),
            other => Err(StorageError::Protocol(format!(
                "unknown config state {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConfigGovernanceRule {
    pub target_env: String,
    pub transition_to: ConfigState,
    pub required_role: String,
    pub required_approvals: u32,
    pub requires_independent_actor: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConfigGovernancePolicy {
    pub rules: Vec<ConfigGovernanceRule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConfigVersion {
    pub id: String,
    pub name: String,
    pub version: u32,
    pub content_json: String,
    pub state: ConfigState,
    pub parent_version: Option<u32>,
    pub created_by: String,
    pub created_at_ms: i64,
    pub state_changed_at_ms: i64,
    pub state_changed_by: String,
    pub state_change_reason: Option<String>,
    pub target_env: Option<String>,
    pub rollout: Option<String>,
    pub approved_by: Option<String>,
    pub approved_at_ms: Option<i64>,
    pub published_by: Option<String>,
    pub published_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConfigDiff {
    pub name: String,
    pub version_a: u32,
    pub version_b: u32,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub changed: Vec<ConfigDiffEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConfigDiffEntry {
    pub path: String,
    pub before: serde_json::Value,
    pub after: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConfigReleaseCommand {
    pub config_id: String,
    pub version: String,
    pub status: String,
    pub released_by: Option<String>,
    pub notes: Option<String>,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredConfigRelease {
    pub id: String,
    pub config_id: String,
    pub version: String,
    pub status: String,
    pub released_by: Option<String>,
    pub notes: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RunConfigVersionBindingCommand {
    pub run_id: String,
    pub config_id: String,
    pub version: String,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredRunConfigVersionBinding {
    pub run_id: String,
    pub config_id: String,
    pub version: String,
    pub bound_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConfigAuditCommand {
    pub config_id: String,
    pub version: Option<String>,
    pub action: String,
    pub actor: Option<String>,
    pub reason: Option<String>,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredConfigAudit {
    pub id: String,
    pub config_id: String,
    pub version: Option<String>,
    pub action: String,
    pub actor: Option<String>,
    pub reason: Option<String>,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConfigApprovalRecord {
    pub id: String,
    pub config_id: String,
    pub version: String,
    pub target_env: Option<String>,
    pub approved_by: String,
    pub approved_at_ms: i64,
    pub actor_role: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConfigApprovalQueueEntry {
    pub config: ConfigVersion,
    pub required_role: String,
    pub required_approvals: u32,
    pub approval_count: u32,
    pub remaining_approvals: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RunConfigSnapshotCommand {
    pub run_id: String,
    pub content: String,
    pub format: String,
    pub checksum: Option<String>,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewSystemLog {
    pub id: String,
    pub run_id: Option<String>,
    pub ts_ms: i64,
    pub level: String,
    pub target: String,
    pub message: String,
    pub fields_json: Option<String>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredSystemLog {
    pub id: String,
    pub run_id: Option<String>,
    pub ts_ms: i64,
    pub level: String,
    pub target: String,
    pub message: String,
    pub fields_json: Option<String>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SystemLogCommand {
    pub run_id: Option<String>,
    pub ts_ms: i64,
    pub level: String,
    pub target: String,
    pub message: String,
    pub fields: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SystemLogFilter {
    pub run_id: Option<String>,
    pub level: Option<String>,
    pub target: Option<String>,
    pub from_ms: Option<i64>,
    pub to_ms: Option<i64>,
    pub search: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemLogRetentionCommand {
    pub before_ms: i64,
    pub target: Option<String>,
    pub run_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemLogRetentionPolicy {
    pub retention_days: u32,
}

#[derive(Clone)]
pub struct DbSystemLogSink {
    db: Db,
}

impl DbSystemLogSink {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl LogSink for DbSystemLogSink {
    async fn write_batch(&self, logs: &[StructuredLogEntry]) -> Result<(), LogSinkError> {
        let logs = logs
            .iter()
            .cloned()
            .map(|log| NewSystemLog {
                id: log.id,
                run_id: log.run_id,
                ts_ms: log.ts_ms,
                level: log.level,
                target: log.target,
                message: log.message,
                fields_json: log.fields_json,
                created_at_ms: log.created_at_ms,
            })
            .collect::<Vec<_>>();
        self.db
            .insert_system_logs_batch(&logs)
            .await
            .map_err(|error| LogSinkError::Write(error.to_string()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewStrategyRun {
    pub id: String,
    pub name: String,
    pub mode: String,
    pub status: String,
    pub started_at_ms: i64,
    pub ended_at_ms: Option<i64>,
    pub error: Option<String>,
    pub config_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StrategyRunRecord {
    pub id: String,
    pub name: String,
    pub mode: String,
    pub status: String,
    pub started_at_ms: i64,
    pub ended_at_ms: Option<i64>,
    pub error: Option<String>,
    pub config_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewOrder {
    pub id: String,
    pub run_id: String,
    pub client_order_id: String,
    pub broker_order_id: Option<String>,
    pub account_id: String,
    pub symbol: String,
    pub side: String,
    pub order_type: String,
    pub price: Option<String>,
    pub qty: String,
    pub filled_qty: String,
    pub status: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecoveredOrderState {
    pub id: String,
    pub run_id: String,
    pub order_qty: String,
    pub filled_qty: String,
    pub status: String,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredOrder {
    pub id: String,
    pub run_id: String,
    pub client_order_id: String,
    pub broker_order_id: Option<String>,
    pub account_id: String,
    pub symbol: String,
    pub side: String,
    pub order_type: String,
    pub price: Option<String>,
    pub qty: String,
    pub filled_qty: String,
    pub status: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewFill {
    pub id: String,
    pub order_id: String,
    pub run_id: String,
    pub symbol: String,
    pub side: String,
    pub price: String,
    pub qty: String,
    pub fee: String,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredFill {
    pub id: String,
    pub order_id: String,
    pub run_id: String,
    pub symbol: String,
    pub side: String,
    pub price: String,
    pub qty: String,
    pub fee: String,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeeVolumeQuery {
    pub account_id: String,
    pub market: String,
    pub exchange: String,
    pub asset_class: String,
    pub symbol: Option<String>,
    pub from_ms: i64,
    pub to_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeeRuleEngineSeed {
    pub rules_by_symbol: BTreeMap<String, FeeRule>,
    pub volume_by_rule: BTreeMap<String, Decimal>,
    pub volume_entries_by_rule: BTreeMap<String, Vec<FeeVolumeEntry>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewPosition {
    pub run_id: String,
    pub account_id: String,
    pub symbol: String,
    pub qty: String,
    pub avg_price: String,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredPosition {
    pub run_id: String,
    pub account_id: String,
    pub symbol: String,
    pub qty: String,
    pub avg_price: String,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewAccountBalance {
    pub run_id: String,
    pub account_id: String,
    pub asset: String,
    pub total: String,
    pub available: String,
    pub frozen: String,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredAccountBalance {
    pub run_id: String,
    pub account_id: String,
    pub asset: String,
    pub total: String,
    pub available: String,
    pub frozen: String,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewPortfolioSnapshot {
    pub id: String,
    pub run_id: String,
    pub account_id: String,
    pub ts_ms: i64,
    pub cash: String,
    pub market_value: String,
    pub equity: String,
    pub realized_pnl: String,
    pub unrealized_pnl: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredPortfolioSnapshot {
    pub id: String,
    pub run_id: String,
    pub account_id: String,
    pub ts_ms: i64,
    pub cash: String,
    pub market_value: String,
    pub equity: String,
    pub realized_pnl: String,
    pub unrealized_pnl: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewEventRecord {
    pub event_id: String,
    pub ts_ms: i64,
    pub source: String,
    pub category: String,
    pub payload_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EventRecord {
    pub event_id: String,
    pub ts_ms: i64,
    pub source: String,
    pub category: String,
    pub payload_json: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MarketRuleAuditFilter {
    pub rule_type: Option<String>,
    pub rule_id: Option<String>,
    pub from_ms: Option<i64>,
    pub to_ms: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewOrderEvent {
    pub id: String,
    pub event_id: String,
    pub run_id: String,
    pub order_id: Option<String>,
    pub client_order_id: Option<String>,
    pub broker_order_id: Option<String>,
    pub account_id: Option<String>,
    pub symbol: Option<String>,
    pub status: String,
    pub event_type: String,
    pub message: Option<String>,
    pub ts_ms: i64,
    pub payload_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredOrderEvent {
    pub id: String,
    pub event_id: String,
    pub run_id: String,
    pub order_id: Option<String>,
    pub client_order_id: Option<String>,
    pub broker_order_id: Option<String>,
    pub account_id: Option<String>,
    pub symbol: Option<String>,
    pub status: String,
    pub event_type: String,
    pub message: Option<String>,
    pub ts_ms: i64,
    pub payload_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewRiskEvent {
    pub id: String,
    pub event_id: String,
    pub run_id: String,
    pub account_id: Option<String>,
    pub symbol: Option<String>,
    pub risk_type: String,
    pub decision: String,
    pub reason: Option<String>,
    pub threshold: Option<String>,
    pub observed_value: Option<String>,
    pub ts_ms: i64,
    pub payload_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredRiskEvent {
    pub id: String,
    pub event_id: String,
    pub run_id: String,
    pub account_id: Option<String>,
    pub symbol: Option<String>,
    pub risk_type: String,
    pub decision: String,
    pub reason: Option<String>,
    pub threshold: Option<String>,
    pub observed_value: Option<String>,
    pub ts_ms: i64,
    pub payload_json: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OrderEventFilter {
    pub run_id: Option<String>,
    pub order_id: Option<String>,
    pub client_order_id: Option<String>,
    pub broker_order_id: Option<String>,
    pub account_id: Option<String>,
    pub symbol: Option<String>,
    pub status: Option<String>,
    pub event_type: Option<String>,
    pub from_ms: Option<i64>,
    pub to_ms: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RiskEventFilter {
    pub run_id: Option<String>,
    pub risk_type: Option<String>,
    pub decision: Option<String>,
    pub account_id: Option<String>,
    pub symbol: Option<String>,
    pub from_ms: Option<i64>,
    pub to_ms: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewInsight {
    pub id: String,
    pub event_id: String,
    pub run_id: String,
    pub strategy: String,
    pub symbol: String,
    pub side: String,
    pub confidence: String,
    pub ts_ms: i64,
    pub payload_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredInsight {
    pub id: String,
    pub event_id: String,
    pub run_id: String,
    pub strategy: String,
    pub symbol: String,
    pub side: String,
    pub confidence: String,
    pub ts_ms: i64,
    pub payload_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewPortfolioTarget {
    pub id: String,
    pub event_id: String,
    pub run_id: String,
    pub account_id: String,
    pub symbol: String,
    pub target_qty: String,
    pub ts_ms: i64,
    pub payload_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredPortfolioTarget {
    pub id: String,
    pub event_id: String,
    pub run_id: String,
    pub account_id: String,
    pub symbol: String,
    pub target_qty: String,
    pub ts_ms: i64,
    pub payload_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredRuntimeEvent {
    pub ts_ms: i64,
    pub category: String,
    pub payload_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BacktestExecutionRecord {
    pub run_id: String,
    pub order_id: String,
    pub fill_id: String,
    pub broker_order_id: String,
    pub account_id: String,
    pub symbol: String,
    pub side: String,
    pub order_type: String,
    pub price: Option<String>,
    pub qty: String,
    pub fill_price: String,
    pub fee: String,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BacktestPositionRecord {
    pub run_id: String,
    pub account_id: String,
    pub symbol: String,
    pub qty: String,
    pub avg_price: String,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BacktestFilledExecutionCommand {
    pub run_id: String,
    pub order_id: String,
    pub fill_id: String,
    pub broker_order_id: String,
    pub order: OrderRequest,
    pub fill_price: Decimal,
    pub fee: Decimal,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BacktestPositionCommand {
    pub run_id: String,
    pub account_id: String,
    pub symbol: String,
    pub qty: Decimal,
    pub avg_price: Decimal,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeEventCommand {
    pub source: String,
    pub ts_ms: i64,
    pub category: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveRunCommand {
    pub run_id: String,
    pub started_at_ms: i64,
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrategyRunStartCommand {
    pub run_id: String,
    pub name: String,
    pub mode: String,
    pub started_at_ms: i64,
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalOrderCommand {
    pub run_id: String,
    pub order_id: String,
    pub client_order_id: String,
    pub broker_order_id: Option<String>,
    pub account_id: String,
    pub symbol: String,
    pub side: String,
    pub order_type: String,
    pub price: Option<Decimal>,
    pub qty: Decimal,
    pub filled_qty: Decimal,
    pub status: String,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalFillCommand {
    pub id: String,
    pub order_id: String,
    pub run_id: String,
    pub symbol: String,
    pub side: String,
    pub price: Decimal,
    pub qty: Decimal,
    pub fee: Decimal,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountBalanceCommand {
    pub run_id: String,
    pub account_id: String,
    pub asset: String,
    pub total: Decimal,
    pub available: Decimal,
    pub frozen: Decimal,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PositionCommand {
    pub run_id: String,
    pub account_id: String,
    pub symbol: String,
    pub qty: Decimal,
    pub avg_price: Decimal,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimePositionSnapshotCommand {
    pub run_id: String,
    pub ts_ms: i64,
    pub symbol: String,
    pub position_side: String,
    pub qty: Decimal,
    pub available_qty: Decimal,
    pub avg_price: Decimal,
    pub mark_price: Option<Decimal>,
    pub currency: String,
    pub contract_metadata_json: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrokerPositionSnapshotCommand {
    pub run_id: String,
    pub account_id: String,
    pub ts_ms: i64,
    pub exchange: String,
    pub symbol: String,
    pub position_side: String,
    pub qty: Decimal,
    pub avg_price: Decimal,
    pub mark_price: Option<Decimal>,
    pub margin_used: Decimal,
    pub unrealized_pnl: Decimal,
    pub realized_pnl: Decimal,
    pub currency: String,
    pub contract_metadata_json: Option<String>,
    pub liquidation_price: Option<Decimal>,
    pub open_interest: Option<Decimal>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CryptoPositionCommand {
    pub run_id: String,
    pub account_id: String,
    pub exchange: String,
    pub symbol: String,
    pub asset_class: String,
    pub margin_mode: String,
    pub position_side: String,
    pub leverage: Decimal,
    pub qty: Decimal,
    pub avg_price: Decimal,
    pub margin_used: Decimal,
    pub funding_fee: Decimal,
    pub realized_pnl: Decimal,
    pub unrealized_pnl: Decimal,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FundingRateCommand {
    pub id: String,
    pub exchange: String,
    pub symbol: String,
    pub funding_time_ms: i64,
    pub funding_rate: Decimal,
    pub mark_price: Option<Decimal>,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CryptoMarketMetaCommand {
    pub exchange: String,
    pub symbol: String,
    pub base_asset: String,
    pub quote_asset: String,
    pub instrument_type: String,
    pub contract_type: Option<String>,
    pub contract_size: Option<Decimal>,
    pub settlement_asset: Option<String>,
    pub min_notional: Option<Decimal>,
    pub min_qty: Option<Decimal>,
    pub max_qty: Option<Decimal>,
    pub price_precision: Option<i64>,
    pub qty_precision: Option<i64>,
    pub price_tick: Option<Decimal>,
    pub qty_step: Option<Decimal>,
    pub maker_fee_rate: Option<Decimal>,
    pub taker_fee_rate: Option<Decimal>,
    pub funding_interval_hours: Option<i64>,
    pub max_leverage: Option<Decimal>,
    pub margin_modes: Option<Vec<String>>,
    pub is_inverse: bool,
    pub is_active: bool,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorporateActionMetaCommand {
    pub market: String,
    pub exchange: String,
    pub symbol: String,
    pub action_type: String,
    pub ex_date_ms: i64,
    pub record_date_ms: Option<i64>,
    pub payable_date_ms: Option<i64>,
    pub ratio: Option<String>,
    pub cash_amount: Option<Decimal>,
    pub currency: Option<String>,
    pub source: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortfolioSnapshotCommand {
    pub id: String,
    pub run_id: String,
    pub account_id: String,
    pub ts_ms: i64,
    pub cash: Decimal,
    pub market_value: Decimal,
    pub equity: Decimal,
    pub realized_pnl: Decimal,
    pub unrealized_pnl: Decimal,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PaperOrderCommand {
    pub run_id: String,
    pub order_id: String,
    pub client_order_id: String,
    pub order: OrderRequest,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PaperFailedOrderCommand {
    pub run_id: String,
    pub order_id: String,
    pub client_order_id: String,
    pub order: OrderRequest,
    pub error: String,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PaperExecutionCommand {
    pub run_id: String,
    pub order_id: String,
    pub fill_id: String,
    pub client_order_id: String,
    pub order: OrderRequest,
    pub broker_order_id: String,
    pub status: String,
    pub price: Decimal,
    pub qty: Decimal,
    pub fee: Decimal,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaperPortfolioSnapshotCommand {
    pub run_id: String,
    pub account_id: String,
    pub ts_ms: i64,
    pub base_currency: String,
    pub cash: Decimal,
    pub market_value: Decimal,
    pub equity: Decimal,
    pub realized_pnl: Decimal,
    pub unrealized_pnl: Decimal,
    pub positions: Vec<PositionCommand>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaperFinalStateCommand {
    pub run_id: String,
    pub strategy_name: String,
    pub account_id: String,
    pub symbol: String,
    pub base_currency: String,
    pub started_at_ms: i64,
    pub ended_at_ms: i64,
    pub config_json: String,
    pub cash: Decimal,
    pub market_value: Decimal,
    pub equity: Decimal,
    pub realized_pnl: Decimal,
    pub unrealized_pnl: Decimal,
    pub position_qty: Decimal,
    pub position_avg_price: Decimal,
    pub positions: Vec<PositionCommand>,
}

struct PaperOrderRecordInput<'a> {
    run_id: &'a str,
    order_id: &'a str,
    client_order_id: &'a str,
    broker_order_id: Option<String>,
    order: &'a OrderRequest,
    filled_qty: Decimal,
    status: &'a str,
    ts_ms: i64,
}

fn paper_order_record(input: PaperOrderRecordInput<'_>) -> NewOrder {
    NewOrder {
        id: input.order_id.to_string(),
        run_id: input.run_id.to_string(),
        client_order_id: input.client_order_id.to_string(),
        broker_order_id: input.broker_order_id,
        account_id: input.order.account_id.clone(),
        symbol: input.order.symbol.clone(),
        side: order_side(input.order),
        order_type: order_type(input.order),
        price: input.order.price.map(|price| price.to_string()),
        qty: input.order.qty.to_string(),
        filled_qty: input.filled_qty.to_string(),
        status: input.status.to_string(),
        created_at_ms: input.ts_ms,
        updated_at_ms: input.ts_ms,
    }
}

#[allow(clippy::too_many_arguments)]
fn paper_order_event_payload(
    run_id: &str,
    order_id: &str,
    client_order_id: &str,
    broker_order_id: Option<&str>,
    order: &OrderRequest,
    filled_qty: Decimal,
    status: &str,
    error: Option<&str>,
) -> serde_json::Value {
    serde_json::json!({
        "run_id": run_id,
        "order_id": order_id,
        "client_order_id": client_order_id,
        "broker_order_id": broker_order_id,
        "account_id": &order.account_id,
        "symbol": &order.symbol,
        "side": order_side(order),
        "order_type": order_type(order),
        "qty": order.qty.to_string(),
        "filled_qty": filled_qty.to_string(),
        "status": status,
        "error": error
    })
}

fn string_field(payload: &serde_json::Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

fn value_field_as_string(payload: &serde_json::Value, key: &str) -> Option<String> {
    payload.get(key).and_then(|value| match value {
        serde_json::Value::String(value) => Some(value.clone()),
        serde_json::Value::Number(value) => Some(value.to_string()),
        _ => None,
    })
}

fn position_snapshot_from_command(
    position: PositionCommand,
    currency: &str,
) -> Result<NewPositionSnapshot, StorageError> {
    let (market, exchange, asset_class) = parse_symbol_parts(&position.symbol)?;
    Ok(NewPositionSnapshot {
        run_id: position.run_id,
        ts_ms: position.updated_at_ms,
        market,
        exchange,
        symbol: position.symbol,
        asset_class,
        position_side: None,
        qty: position.qty.to_string(),
        available_qty: position.qty.to_string(),
        avg_price: Some(position.avg_price.to_string()),
        entry_price: Some(position.avg_price.to_string()),
        market_price: None,
        mark_price: None,
        market_value: None,
        unrealized_pnl: None,
        realized_pnl: None,
        currency: currency.to_string(),
        contract_metadata_json: None,
        liquidation_price: None,
        open_interest: None,
        created_at_ms: position.updated_at_ms,
    })
}

fn runtime_position_snapshot_from_command(
    command: RuntimePositionSnapshotCommand,
) -> Result<NewPositionSnapshot, StorageError> {
    let (market, exchange, asset_class) = parse_symbol_parts(&command.symbol)?;
    Ok(NewPositionSnapshot {
        run_id: command.run_id,
        ts_ms: command.ts_ms,
        market,
        exchange,
        symbol: command.symbol,
        asset_class,
        position_side: Some(command.position_side),
        qty: command.qty.to_string(),
        available_qty: command.available_qty.to_string(),
        avg_price: Some(command.avg_price.to_string()),
        entry_price: Some(command.avg_price.to_string()),
        market_price: None,
        mark_price: command.mark_price.map(|price| price.to_string()),
        market_value: command
            .mark_price
            .map(|price| (command.qty * price).to_string()),
        unrealized_pnl: Some(Decimal::ZERO.to_string()),
        realized_pnl: Some(Decimal::ZERO.to_string()),
        currency: command.currency,
        contract_metadata_json: command.contract_metadata_json,
        liquidation_price: None,
        open_interest: None,
        created_at_ms: command.ts_ms,
    })
}

fn broker_position_snapshot_from_command(
    command: BrokerPositionSnapshotCommand,
) -> Result<NewPositionSnapshot, StorageError> {
    let (market, exchange, asset_class) = parse_symbol_parts(&command.symbol)?;
    Ok(NewPositionSnapshot {
        run_id: command.run_id,
        ts_ms: command.ts_ms,
        market,
        exchange,
        symbol: command.symbol,
        asset_class,
        position_side: Some(command.position_side),
        qty: command.qty.to_string(),
        available_qty: command.qty.abs().to_string(),
        avg_price: Some(command.avg_price.to_string()),
        entry_price: Some(command.avg_price.to_string()),
        market_price: command.mark_price.map(|price| price.to_string()),
        mark_price: command.mark_price.map(|price| price.to_string()),
        market_value: command
            .mark_price
            .map(|price| (command.qty * price).to_string()),
        unrealized_pnl: Some(command.unrealized_pnl.to_string()),
        realized_pnl: Some(command.realized_pnl.to_string()),
        currency: command.currency,
        contract_metadata_json: command.contract_metadata_json,
        liquidation_price: command.liquidation_price.map(|price| price.to_string()),
        open_interest: command.open_interest.map(|value| value.to_string()),
        created_at_ms: command.ts_ms,
    })
}

fn parse_symbol_parts(symbol: &str) -> StorageResult<(String, String, String)> {
    let mut parts = symbol.split(':');
    let market = parts.next();
    let exchange = parts.next();
    let _code = parts.next();
    let asset_class = parts.next();
    if parts.next().is_some() {
        return Err(StorageError::Protocol(format!(
            "unsupported symbol format {symbol}"
        )));
    }

    match (market, exchange, asset_class) {
        (Some(market), Some(exchange), Some(asset_class)) => Ok((
            market.to_string(),
            exchange.to_string(),
            asset_class.to_string(),
        )),
        _ => Err(StorageError::Protocol(format!(
            "unsupported symbol format {symbol}"
        ))),
    }
}

fn order_event_projection(
    event_id: &str,
    ts_ms: i64,
    source: &str,
    category: &str,
    payload_json: &str,
) -> Option<NewOrderEvent> {
    if !category.starts_with("broker.order.") && !category.starts_with("algorithm.oms.") {
        return None;
    }

    let payload = serde_json::from_str::<serde_json::Value>(payload_json).ok()?;
    let status = string_field(&payload, "status").unwrap_or_else(|| {
        category
            .rsplit('.')
            .next()
            .unwrap_or("unknown")
            .to_uppercase()
    });

    Some(NewOrderEvent {
        id: Uuid::new_v4().to_string(),
        event_id: event_id.to_string(),
        run_id: string_field(&payload, "run_id").unwrap_or_else(|| source.to_string()),
        order_id: string_field(&payload, "order_id"),
        client_order_id: string_field(&payload, "client_order_id"),
        broker_order_id: string_field(&payload, "broker_order_id"),
        account_id: string_field(&payload, "account_id"),
        symbol: string_field(&payload, "symbol"),
        status,
        event_type: category.to_string(),
        message: string_field(&payload, "message").or_else(|| string_field(&payload, "error")),
        ts_ms,
        payload_json: payload_json.to_string(),
    })
}

fn risk_event_projection(
    event_id: &str,
    ts_ms: i64,
    source: &str,
    category: &str,
    payload_json: &str,
) -> Option<NewRiskEvent> {
    if !category.starts_with("algorithm.risk.") {
        return None;
    }

    let payload = serde_json::from_str::<serde_json::Value>(payload_json).ok()?;
    let decision = string_field(&payload, "decision")
        .or_else(|| string_field(&payload, "status"))
        .unwrap_or_else(|| category.rsplit('.').next().unwrap_or("unknown").to_string());

    Some(NewRiskEvent {
        id: Uuid::new_v4().to_string(),
        event_id: event_id.to_string(),
        run_id: string_field(&payload, "run_id").unwrap_or_else(|| source.to_string()),
        account_id: string_field(&payload, "account_id"),
        symbol: string_field(&payload, "symbol"),
        risk_type: string_field(&payload, "risk_type").unwrap_or_else(|| "pre_trade".to_string()),
        decision,
        reason: string_field(&payload, "reason").or_else(|| string_field(&payload, "error")),
        threshold: string_field(&payload, "threshold"),
        observed_value: string_field(&payload, "observed_value"),
        ts_ms,
        payload_json: payload_json.to_string(),
    })
}

fn insight_projection(
    event_id: &str,
    ts_ms: i64,
    source: &str,
    category: &str,
    payload_json: &str,
) -> Option<NewInsight> {
    if category != "algorithm.alpha.generated" {
        return None;
    }

    let payload = serde_json::from_str::<serde_json::Value>(payload_json).ok()?;
    Some(NewInsight {
        id: Uuid::new_v4().to_string(),
        event_id: event_id.to_string(),
        run_id: string_field(&payload, "run_id").unwrap_or_else(|| source.to_string()),
        strategy: string_field(&payload, "strategy").unwrap_or_else(|| "unknown".to_string()),
        symbol: string_field(&payload, "symbol")?,
        side: string_field(&payload, "side")?,
        confidence: value_field_as_string(&payload, "confidence")?,
        ts_ms,
        payload_json: payload_json.to_string(),
    })
}

fn portfolio_target_projection(
    event_id: &str,
    ts_ms: i64,
    source: &str,
    category: &str,
    payload_json: &str,
) -> Option<NewPortfolioTarget> {
    if category != "algorithm.portfolio.target" {
        return None;
    }

    let payload = serde_json::from_str::<serde_json::Value>(payload_json).ok()?;
    Some(NewPortfolioTarget {
        id: Uuid::new_v4().to_string(),
        event_id: event_id.to_string(),
        run_id: string_field(&payload, "run_id").unwrap_or_else(|| source.to_string()),
        account_id: string_field(&payload, "account_id").unwrap_or_else(|| "unknown".to_string()),
        symbol: string_field(&payload, "symbol")?,
        target_qty: value_field_as_string(&payload, "target_qty")?,
        ts_ms,
        payload_json: payload_json.to_string(),
    })
}

fn order_side(order: &OrderRequest) -> String {
    format!("{:?}", order.side).to_uppercase()
}

fn order_type(order: &OrderRequest) -> String {
    format!("{:?}", order.order_type).to_uppercase()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BacktestCompletedRun {
    pub run_id: String,
    pub strategy_name: String,
    pub started_at_ms: i64,
    pub ended_at_ms: i64,
    pub config_json: String,
}

impl Db {
    pub async fn insert_instrument(&self, instrument: NewInstrument) -> StorageResult<()> {
        sqlx::query(
            r#"
            INSERT INTO instruments (
                symbol, market, exchange, asset_class, currency, lot_size, tick_size, tradable
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(instrument.symbol)
        .bind(instrument.market)
        .bind(instrument.exchange)
        .bind(instrument.asset_class)
        .bind(instrument.currency)
        .bind(instrument.lot_size)
        .bind(instrument.tick_size)
        .bind(instrument.tradable)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn get_instrument(&self, symbol: &str) -> StorageResult<Option<InstrumentRecord>> {
        let row =
            sqlx::query_as::<_, (String, String, String, String, String, String, String, i64)>(
                r#"
            SELECT symbol, market, exchange, asset_class, currency, lot_size, tick_size, tradable
            FROM instruments
            WHERE symbol = ?
            "#,
            )
            .bind(symbol)
            .fetch_optional(self.pool())
            .await?;

        Ok(row.map(
            |(symbol, market, exchange, asset_class, currency, lot_size, tick_size, tradable)| {
                InstrumentRecord {
                    symbol,
                    market,
                    exchange,
                    asset_class,
                    currency,
                    lot_size,
                    tick_size,
                    tradable: tradable != 0,
                }
            },
        ))
    }

    pub async fn insert_lot_size_rule(&self, rule: NewLotSizeRule) -> StorageResult<()> {
        let audit_rule = rule.clone();
        sqlx::query(
            r#"
            INSERT INTO lot_size_rules (
                id, market, exchange, asset_class, symbol, lot_size, min_qty,
                min_notional, effective_from_ms, effective_to_ms
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(rule.id)
        .bind(rule.market)
        .bind(rule.exchange)
        .bind(rule.asset_class)
        .bind(rule.symbol)
        .bind(rule.lot_size)
        .bind(rule.min_qty)
        .bind(rule.min_notional)
        .bind(rule.effective_from_ms)
        .bind(rule.effective_to_ms)
        .execute(self.pool())
        .await?;
        self.record_market_rule_audit(
            "lot_size",
            &audit_rule.id,
            "inserted",
            audit_rule.effective_from_ms,
            serde_json::to_value(&audit_rule)
                .map_err(|err| StorageError::Protocol(err.to_string()))?,
        )
        .await?;
        Ok(())
    }

    pub async fn configure_lot_size_rule(&self, command: LotSizeRuleCommand) -> StorageResult<()> {
        self.insert_lot_size_rule(NewLotSizeRule {
            id: command.id,
            market: command.market,
            exchange: command.exchange,
            asset_class: command.asset_class,
            symbol: command.symbol,
            lot_size: command.lot_size,
            min_qty: command.min_qty,
            min_notional: command.min_notional,
            effective_from_ms: command.effective_from_ms,
            effective_to_ms: command.effective_to_ms,
        })
        .await
    }

    pub async fn update_lot_size_rule_effective_to(
        &self,
        id: &str,
        effective_to_ms: Option<i64>,
        ts_ms: i64,
    ) -> StorageResult<()> {
        let result = sqlx::query(
            r#"
            UPDATE lot_size_rules
            SET effective_to_ms = ?
            WHERE id = ?
            "#,
        )
        .bind(effective_to_ms)
        .bind(id)
        .execute(self.pool())
        .await?;
        if result.rows_affected() == 0 {
            return Err(StorageError::Protocol(format!(
                "lot_size_rule not found: {id}"
            )));
        }
        self.record_market_rule_audit(
            "lot_size",
            id,
            "updated",
            ts_ms,
            serde_json::json!({
                "id": id,
                "effective_to_ms": effective_to_ms,
            }),
        )
        .await?;
        Ok(())
    }

    pub async fn find_lot_size_rule(
        &self,
        market: &str,
        exchange: &str,
        asset_class: &str,
        symbol: &str,
        at_ms: i64,
    ) -> StorageResult<Option<StoredLotSizeRule>> {
        type LotSizeRuleRow = (
            String,
            String,
            String,
            String,
            Option<String>,
            String,
            String,
            String,
            i64,
            Option<i64>,
        );

        let row = sqlx::query_as::<_, LotSizeRuleRow>(
            r#"
            SELECT id, market, exchange, asset_class, symbol, lot_size, min_qty,
                   min_notional, effective_from_ms, effective_to_ms
            FROM lot_size_rules
            WHERE market = ?
              AND exchange = ?
              AND asset_class = ?
              AND (symbol = ? OR symbol IS NULL)
              AND effective_from_ms <= ?
              AND (effective_to_ms IS NULL OR effective_to_ms > ?)
            ORDER BY (symbol = ?) DESC, effective_from_ms DESC, id
            LIMIT 1
            "#,
        )
        .bind(market)
        .bind(exchange)
        .bind(asset_class)
        .bind(symbol)
        .bind(at_ms)
        .bind(at_ms)
        .bind(symbol)
        .fetch_optional(self.pool())
        .await?;

        Ok(row.map(
            |(
                id,
                market,
                exchange,
                asset_class,
                symbol,
                lot_size,
                min_qty,
                min_notional,
                effective_from_ms,
                effective_to_ms,
            )| StoredLotSizeRule {
                id,
                market,
                exchange,
                asset_class,
                symbol,
                lot_size,
                min_qty,
                min_notional,
                effective_from_ms,
                effective_to_ms,
            },
        ))
    }

    pub async fn insert_price_limit_rule(&self, rule: NewPriceLimitRule) -> StorageResult<()> {
        let audit_rule = rule.clone();
        sqlx::query(
            r#"
            INSERT INTO price_limit_rules (
                id, market, exchange, asset_class, symbol, tick_size,
                limit_up_bps, limit_down_bps, effective_from_ms, effective_to_ms
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(rule.id)
        .bind(rule.market)
        .bind(rule.exchange)
        .bind(rule.asset_class)
        .bind(rule.symbol)
        .bind(rule.tick_size)
        .bind(rule.limit_up_bps)
        .bind(rule.limit_down_bps)
        .bind(rule.effective_from_ms)
        .bind(rule.effective_to_ms)
        .execute(self.pool())
        .await?;
        self.record_market_rule_audit(
            "price_limit",
            &audit_rule.id,
            "inserted",
            audit_rule.effective_from_ms,
            serde_json::to_value(&audit_rule)
                .map_err(|err| StorageError::Protocol(err.to_string()))?,
        )
        .await?;
        Ok(())
    }

    pub async fn configure_price_limit_rule(
        &self,
        command: PriceLimitRuleCommand,
    ) -> StorageResult<()> {
        self.insert_price_limit_rule(NewPriceLimitRule {
            id: command.id,
            market: command.market,
            exchange: command.exchange,
            asset_class: command.asset_class,
            symbol: command.symbol,
            tick_size: command.tick_size,
            limit_up_bps: command.limit_up_bps,
            limit_down_bps: command.limit_down_bps,
            effective_from_ms: command.effective_from_ms,
            effective_to_ms: command.effective_to_ms,
        })
        .await
    }

    pub async fn update_price_limit_rule_effective_to(
        &self,
        id: &str,
        effective_to_ms: Option<i64>,
        ts_ms: i64,
    ) -> StorageResult<()> {
        let result = sqlx::query(
            r#"
            UPDATE price_limit_rules
            SET effective_to_ms = ?
            WHERE id = ?
            "#,
        )
        .bind(effective_to_ms)
        .bind(id)
        .execute(self.pool())
        .await?;
        if result.rows_affected() == 0 {
            return Err(StorageError::Protocol(format!(
                "price_limit_rule not found: {id}"
            )));
        }
        self.record_market_rule_audit(
            "price_limit",
            id,
            "updated",
            ts_ms,
            serde_json::json!({
                "id": id,
                "effective_to_ms": effective_to_ms,
            }),
        )
        .await?;
        Ok(())
    }

    pub async fn find_price_limit_rule(
        &self,
        market: &str,
        exchange: &str,
        asset_class: &str,
        symbol: &str,
        at_ms: i64,
    ) -> StorageResult<Option<StoredPriceLimitRule>> {
        type PriceLimitRuleRow = (
            String,
            String,
            String,
            String,
            Option<String>,
            String,
            Option<String>,
            Option<String>,
            i64,
            Option<i64>,
        );

        let row = sqlx::query_as::<_, PriceLimitRuleRow>(
            r#"
            SELECT id, market, exchange, asset_class, symbol, tick_size,
                   limit_up_bps, limit_down_bps, effective_from_ms, effective_to_ms
            FROM price_limit_rules
            WHERE market = ?
              AND exchange = ?
              AND asset_class = ?
              AND (symbol = ? OR symbol IS NULL)
              AND effective_from_ms <= ?
              AND (effective_to_ms IS NULL OR effective_to_ms > ?)
            ORDER BY (symbol = ?) DESC, effective_from_ms DESC, id
            LIMIT 1
            "#,
        )
        .bind(market)
        .bind(exchange)
        .bind(asset_class)
        .bind(symbol)
        .bind(at_ms)
        .bind(at_ms)
        .bind(symbol)
        .fetch_optional(self.pool())
        .await?;

        Ok(row.map(
            |(
                id,
                market,
                exchange,
                asset_class,
                symbol,
                tick_size,
                limit_up_bps,
                limit_down_bps,
                effective_from_ms,
                effective_to_ms,
            )| StoredPriceLimitRule {
                id,
                market,
                exchange,
                asset_class,
                symbol,
                tick_size,
                limit_up_bps,
                limit_down_bps,
                effective_from_ms,
                effective_to_ms,
            },
        ))
    }

    pub async fn insert_fee_rule(&self, rule: NewFeeRule) -> StorageResult<()> {
        parse_volume_window(&rule.volume_window)?;
        let audit_rule = rule.clone();
        sqlx::query(
            r#"
            INSERT INTO fee_rules (
                id, market, exchange, asset_class, symbol, volume_window, maker_bps, taker_bps, minimum_fee,
                tax_bps, exchange_fee_bps,
                effective_from_ms, effective_to_ms
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(rule.id)
        .bind(rule.market)
        .bind(rule.exchange)
        .bind(rule.asset_class)
        .bind(rule.symbol)
        .bind(rule.volume_window)
        .bind(rule.maker_bps)
        .bind(rule.taker_bps)
        .bind(rule.minimum_fee)
        .bind(rule.tax_bps)
        .bind(rule.exchange_fee_bps)
        .bind(rule.effective_from_ms)
        .bind(rule.effective_to_ms)
        .execute(self.pool())
        .await?;
        self.record_market_rule_audit(
            "fee",
            &audit_rule.id,
            "inserted",
            audit_rule.effective_from_ms,
            serde_json::to_value(&audit_rule)
                .map_err(|err| StorageError::Protocol(err.to_string()))?,
        )
        .await?;
        Ok(())
    }

    pub async fn update_fee_rule_effective_to(
        &self,
        id: &str,
        effective_to_ms: Option<i64>,
        ts_ms: i64,
    ) -> StorageResult<()> {
        let result = sqlx::query(
            r#"
            UPDATE fee_rules
            SET effective_to_ms = ?
            WHERE id = ?
            "#,
        )
        .bind(effective_to_ms)
        .bind(id)
        .execute(self.pool())
        .await?;
        if result.rows_affected() == 0 {
            return Err(StorageError::Protocol(format!("fee_rule not found: {id}")));
        }
        self.record_market_rule_audit(
            "fee",
            id,
            "updated",
            ts_ms,
            serde_json::json!({
                "id": id,
                "effective_to_ms": effective_to_ms,
            }),
        )
        .await?;
        Ok(())
    }

    pub async fn insert_fee_rule_tier(&self, tier: NewFeeRuleTier) -> StorageResult<()> {
        sqlx::query(
            r#"
            INSERT INTO fee_rule_tiers (
                id, fee_rule_id, volume_from, volume_to, maker_bps, taker_bps
            )
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(tier.id)
        .bind(tier.fee_rule_id)
        .bind(tier.volume_from)
        .bind(tier.volume_to)
        .bind(tier.maker_bps)
        .bind(tier.taker_bps)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn create_fee_rule_with_tiers(
        &self,
        command: NewFeeRuleWithTiers,
    ) -> StorageResult<StoredFeeRuleWithTiers> {
        let rule = command.rule;
        let tiers = command.tiers;
        parse_volume_window(&rule.volume_window)?;
        for tier in &tiers {
            if tier.fee_rule_id != rule.id {
                return Err(StorageError::Protocol(format!(
                    "fee_rule_tier {} references {}, expected {}",
                    tier.id, tier.fee_rule_id, rule.id
                )));
            }
        }

        let stored_rule = StoredFeeRule {
            id: rule.id.clone(),
            market: rule.market.clone(),
            exchange: rule.exchange.clone(),
            asset_class: rule.asset_class.clone(),
            symbol: rule.symbol.clone(),
            volume_window: rule.volume_window.clone(),
            maker_bps: rule.maker_bps.clone(),
            taker_bps: rule.taker_bps.clone(),
            minimum_fee: rule.minimum_fee.clone(),
            tax_bps: rule.tax_bps.clone(),
            exchange_fee_bps: rule.exchange_fee_bps.clone(),
            effective_from_ms: rule.effective_from_ms,
            effective_to_ms: rule.effective_to_ms,
        };

        let mut tx = self.pool().begin().await?;
        sqlx::query(
            r#"
            INSERT INTO fee_rules (
                id, market, exchange, asset_class, symbol, volume_window, maker_bps, taker_bps, minimum_fee,
                tax_bps, exchange_fee_bps,
                effective_from_ms, effective_to_ms
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&rule.id)
        .bind(&rule.market)
        .bind(&rule.exchange)
        .bind(&rule.asset_class)
        .bind(&rule.symbol)
        .bind(&rule.volume_window)
        .bind(&rule.maker_bps)
        .bind(&rule.taker_bps)
        .bind(&rule.minimum_fee)
        .bind(&rule.tax_bps)
        .bind(&rule.exchange_fee_bps)
        .bind(rule.effective_from_ms)
        .bind(rule.effective_to_ms)
        .execute(&mut *tx)
        .await?;

        for tier in &tiers {
            sqlx::query(
                r#"
                INSERT INTO fee_rule_tiers (
                    id, fee_rule_id, volume_from, volume_to, maker_bps, taker_bps
                )
                VALUES (?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&tier.id)
            .bind(&tier.fee_rule_id)
            .bind(&tier.volume_from)
            .bind(&tier.volume_to)
            .bind(&tier.maker_bps)
            .bind(&tier.taker_bps)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;

        let stored = StoredFeeRuleWithTiers {
            rule: stored_rule,
            tiers: self.list_fee_rule_tiers(&rule.id).await?,
        };
        self.record_market_rule_audit(
            "fee",
            &rule.id,
            "inserted",
            rule.effective_from_ms,
            serde_json::to_value(&stored).map_err(|err| StorageError::Protocol(err.to_string()))?,
        )
        .await?;
        Ok(stored)
    }

    pub async fn list_fee_rule_tiers(
        &self,
        fee_rule_id: &str,
    ) -> StorageResult<Vec<StoredFeeRuleTier>> {
        type FeeRuleTierRow = (String, String, String, Option<String>, String, String);

        let rows = sqlx::query_as::<_, FeeRuleTierRow>(
            r#"
            SELECT id, fee_rule_id, volume_from, volume_to, maker_bps, taker_bps
            FROM fee_rule_tiers
            WHERE fee_rule_id = ?
            ORDER BY id
            "#,
        )
        .bind(fee_rule_id)
        .fetch_all(self.pool())
        .await?;

        let mut tiers = rows
            .into_iter()
            .map(
                |(id, fee_rule_id, volume_from, volume_to, maker_bps, taker_bps)| {
                    let sort_key = Decimal::from_str(&volume_from).map_err(|error| {
                        StorageError::Protocol(format!(
                            "invalid fee_rule_tier volume_from {volume_from}: {error}"
                        ))
                    })?;
                    Ok((
                        sort_key,
                        StoredFeeRuleTier {
                            id,
                            fee_rule_id,
                            volume_from,
                            volume_to,
                            maker_bps,
                            taker_bps,
                        },
                    ))
                },
            )
            .collect::<StorageResult<Vec<_>>>()?;
        tiers.sort_by(|(left, left_tier), (right, right_tier)| {
            left.cmp(right)
                .then_with(|| left_tier.id.cmp(&right_tier.id))
        });
        Ok(tiers.into_iter().map(|(_, tier)| tier).collect())
    }

    pub async fn find_fee_rule(
        &self,
        market: &str,
        exchange: &str,
        asset_class: &str,
        symbol: Option<&str>,
        at_ms: i64,
    ) -> StorageResult<Option<StoredFeeRule>> {
        type FeeRuleRow = (
            String,
            String,
            String,
            String,
            Option<String>,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            i64,
            Option<i64>,
        );

        let row = sqlx::query_as::<_, FeeRuleRow>(
            r#"
            SELECT id, market, exchange, asset_class, symbol, volume_window, maker_bps, taker_bps, minimum_fee,
                   tax_bps, exchange_fee_bps,
                   effective_from_ms, effective_to_ms
            FROM fee_rules
            WHERE market = ?
              AND exchange = ?
              AND (
                  symbol = ?
                  OR (symbol IS NULL AND asset_class = ?)
                  OR (symbol IS NULL AND asset_class = '*')
              )
              AND effective_from_ms <= ?
              AND (effective_to_ms IS NULL OR effective_to_ms > ?)
            ORDER BY
              CASE
                WHEN symbol = ? THEN 0
                WHEN symbol IS NULL AND asset_class = ? THEN 1
                ELSE 2
              END,
              effective_from_ms DESC,
              id
            LIMIT 1
            "#,
        )
        .bind(market)
        .bind(exchange)
        .bind(symbol)
        .bind(asset_class)
        .bind(at_ms)
        .bind(at_ms)
        .bind(symbol)
        .bind(asset_class)
        .fetch_optional(self.pool())
        .await?;

        Ok(row.map(
            |(
                id,
                market,
                exchange,
                asset_class,
                symbol,
                volume_window,
                maker_bps,
                taker_bps,
                minimum_fee,
                tax_bps,
                exchange_fee_bps,
                effective_from_ms,
                effective_to_ms,
            )| StoredFeeRule {
                id,
                market,
                exchange,
                asset_class,
                symbol,
                volume_window,
                maker_bps,
                taker_bps,
                minimum_fee,
                tax_bps,
                exchange_fee_bps,
                effective_from_ms,
                effective_to_ms,
            },
        ))
    }

    pub async fn find_fee_rule_with_tiers(
        &self,
        market: &str,
        exchange: &str,
        asset_class: &str,
        symbol: Option<&str>,
        at_ms: i64,
    ) -> StorageResult<Option<StoredFeeRuleWithTiers>> {
        let Some(rule) = self
            .find_fee_rule(market, exchange, asset_class, symbol, at_ms)
            .await?
        else {
            return Ok(None);
        };
        let tiers = self.list_fee_rule_tiers(&rule.id).await?;
        Ok(Some(StoredFeeRuleWithTiers { rule, tiers }))
    }

    pub async fn load_market_fee_rules(
        &self,
        symbols: &[String],
        as_of_ms: i64,
    ) -> StorageResult<BTreeMap<String, FeeRule>> {
        let mut fee_rules_by_symbol = BTreeMap::new();
        for symbol in symbols {
            let Some((market, exchange, asset_class)) = market_rule_symbol_parts(symbol) else {
                continue;
            };
            let Some(rule_with_tiers) = self
                .find_fee_rule_with_tiers(&market, &exchange, &asset_class, Some(symbol), as_of_ms)
                .await?
            else {
                continue;
            };
            fee_rules_by_symbol.insert(symbol.clone(), fee_rule_with_tiers(rule_with_tiers)?);
        }
        Ok(fee_rules_by_symbol)
    }

    pub async fn load_market_fee_rules_with_account_volume(
        &self,
        symbols: &[String],
        account_id: &str,
        as_of_ms: i64,
    ) -> StorageResult<FeeRuleEngineSeed> {
        let mut rules_by_symbol = BTreeMap::new();
        let mut volume_by_rule = BTreeMap::new();
        let mut volume_entries_by_rule = BTreeMap::new();
        let mut logged_run_rules = BTreeSet::new();
        for symbol in symbols {
            let Some((market, exchange, asset_class)) = market_rule_symbol_parts(symbol) else {
                continue;
            };
            let Some(rule_with_tiers) = self
                .find_fee_rule_with_tiers(&market, &exchange, &asset_class, Some(symbol), as_of_ms)
                .await?
            else {
                continue;
            };
            let rule_scope = rule_with_tiers.rule.clone();
            let fee_rule = fee_rule_with_tiers(rule_with_tiers)?;
            let rule_id = fee_rule.id.clone();
            let window = fee_rule.volume_window;
            match fee_volume_seed_window(window, as_of_ms)? {
                Some((from_ms, to_ms)) if !volume_by_rule.contains_key(&rule_id) => {
                    let entries = self
                        .list_fee_volume_entries(FeeVolumeQuery {
                            account_id: account_id.to_string(),
                            market: rule_scope.market,
                            exchange: rule_scope.exchange,
                            asset_class: rule_scope.asset_class,
                            symbol: rule_scope.symbol,
                            from_ms,
                            to_ms,
                        })
                        .await?;
                    let volume = entries
                        .iter()
                        .fold(Decimal::ZERO, |total, entry| total + entry.notional);
                    tracing::debug!(
                        rule_id = %rule_id,
                        window = %window.as_str(),
                        from_ms,
                        to_ms,
                        seed_volume = %volume,
                        "fee rule volume seed loaded"
                    );
                    volume_entries_by_rule.insert(rule_id.clone(), entries);
                    volume_by_rule.insert(rule_id.clone(), volume);
                }
                None if logged_run_rules.insert(rule_id.clone()) => {
                    tracing::debug!(
                        rule_id = %rule_id,
                        window = %window.as_str(),
                        from_ms = ?Option::<i64>::None,
                        to_ms = ?Option::<i64>::None,
                        seed_volume = %Decimal::ZERO,
                        "fee rule volume seed loaded"
                    );
                }
                _ => {}
            }
            rules_by_symbol.insert(symbol.clone(), fee_rule);
        }
        Ok(FeeRuleEngineSeed {
            rules_by_symbol,
            volume_by_rule,
            volume_entries_by_rule,
        })
    }

    pub async fn upsert_market_calendar(&self, calendar: NewMarketCalendar) -> StorageResult<()> {
        sqlx::query(
            r#"
            INSERT INTO market_calendars (
                id, market, trading_day, is_open, session_template
            )
            VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(market, trading_day) DO UPDATE SET
                id = excluded.id,
                is_open = excluded.is_open,
                session_template = excluded.session_template
            "#,
        )
        .bind(calendar.id)
        .bind(calendar.market)
        .bind(calendar.trading_day)
        .bind(if calendar.is_open { 1 } else { 0 })
        .bind(calendar.session_template)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn find_market_calendar(
        &self,
        market: &str,
        trading_day: &str,
    ) -> StorageResult<Option<StoredMarketCalendar>> {
        type MarketCalendarRow = (String, String, String, i64, Option<String>);

        let row = sqlx::query_as::<_, MarketCalendarRow>(
            r#"
            SELECT id, market, trading_day, is_open, session_template
            FROM market_calendars
            WHERE market = ?
              AND trading_day = ?
            LIMIT 1
            "#,
        )
        .bind(market)
        .bind(trading_day)
        .fetch_optional(self.pool())
        .await?;

        Ok(row.map(
            |(id, market, trading_day, is_open, session_template)| StoredMarketCalendar {
                id,
                market,
                trading_day,
                is_open: is_open != 0,
                session_template,
            },
        ))
    }

    pub async fn insert_trading_session_rule(
        &self,
        session: NewTradingSessionRule,
    ) -> StorageResult<()> {
        sqlx::query(
            r#"
            INSERT INTO trading_sessions (
                id, market, trading_day, session_name, open_time, close_time, timezone
            )
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(session.id)
        .bind(session.market)
        .bind(session.trading_day)
        .bind(session.session_name)
        .bind(session.open_time)
        .bind(session.close_time)
        .bind(session.timezone)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn list_trading_session_rules(
        &self,
        market: &str,
        trading_day: &str,
    ) -> StorageResult<Vec<StoredTradingSessionRule>> {
        type TradingSessionRow = (String, String, String, String, String, String, String);

        let rows = sqlx::query_as::<_, TradingSessionRow>(
            r#"
            SELECT id, market, trading_day, session_name, open_time, close_time, timezone
            FROM trading_sessions
            WHERE market = ?
              AND trading_day = ?
            ORDER BY open_time, id
            "#,
        )
        .bind(market)
        .bind(trading_day)
        .fetch_all(self.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(id, market, trading_day, session_name, open_time, close_time, timezone)| {
                    StoredTradingSessionRule {
                        id,
                        market,
                        trading_day,
                        session_name,
                        open_time,
                        close_time,
                        timezone,
                    }
                },
            )
            .collect())
    }

    pub async fn upsert_crypto_position(&self, position: NewCryptoPosition) -> StorageResult<()> {
        sqlx::query(
            r#"
            INSERT INTO crypto_positions (
                run_id, account_id, exchange, symbol, asset_class, margin_mode, position_side,
                leverage, qty, avg_price, margin_used, funding_fee, realized_pnl,
                unrealized_pnl, updated_at_ms
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(run_id, account_id, exchange, symbol, position_side) DO UPDATE SET
                asset_class = excluded.asset_class,
                margin_mode = excluded.margin_mode,
                leverage = excluded.leverage,
                qty = excluded.qty,
                avg_price = excluded.avg_price,
                margin_used = excluded.margin_used,
                funding_fee = excluded.funding_fee,
                realized_pnl = excluded.realized_pnl,
                unrealized_pnl = excluded.unrealized_pnl,
                updated_at_ms = excluded.updated_at_ms
            "#,
        )
        .bind(position.run_id)
        .bind(position.account_id)
        .bind(position.exchange)
        .bind(position.symbol)
        .bind(position.asset_class)
        .bind(position.margin_mode)
        .bind(position.position_side)
        .bind(position.leverage)
        .bind(position.qty)
        .bind(position.avg_price)
        .bind(position.margin_used)
        .bind(position.funding_fee)
        .bind(position.realized_pnl)
        .bind(position.unrealized_pnl)
        .bind(position.updated_at_ms)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn list_crypto_positions(
        &self,
        run_id: &str,
    ) -> StorageResult<Vec<StoredCryptoPosition>> {
        type CryptoPositionRow = (
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            i64,
        );

        let rows = sqlx::query_as::<_, CryptoPositionRow>(
            r#"
            SELECT run_id, account_id, exchange, symbol, asset_class, margin_mode,
                   position_side, leverage, qty, avg_price, margin_used, funding_fee,
                   realized_pnl, unrealized_pnl, updated_at_ms
            FROM crypto_positions
            WHERE run_id = ?
            ORDER BY account_id, exchange, symbol, position_side
            "#,
        )
        .bind(run_id)
        .fetch_all(self.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(
                    run_id,
                    account_id,
                    exchange,
                    symbol,
                    asset_class,
                    margin_mode,
                    position_side,
                    leverage,
                    qty,
                    avg_price,
                    margin_used,
                    funding_fee,
                    realized_pnl,
                    unrealized_pnl,
                    updated_at_ms,
                )| StoredCryptoPosition {
                    run_id,
                    account_id,
                    exchange,
                    symbol,
                    asset_class,
                    margin_mode,
                    position_side,
                    leverage,
                    qty,
                    avg_price,
                    margin_used,
                    funding_fee,
                    realized_pnl,
                    unrealized_pnl,
                    updated_at_ms,
                },
            )
            .collect())
    }

    pub async fn get_crypto_position(
        &self,
        run_id: &str,
        account_id: &str,
        exchange: &str,
        symbol: &str,
        position_side: &str,
    ) -> StorageResult<Option<StoredCryptoPosition>> {
        type CryptoPositionRow = (
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            i64,
        );

        let row = sqlx::query_as::<_, CryptoPositionRow>(
            r#"
            SELECT run_id, account_id, exchange, symbol, asset_class, margin_mode,
                   position_side, leverage, qty, avg_price, margin_used, funding_fee,
                   realized_pnl, unrealized_pnl, updated_at_ms
            FROM crypto_positions
            WHERE run_id = ?
              AND account_id = ?
              AND exchange = ?
              AND symbol = ?
              AND position_side = ?
            "#,
        )
        .bind(run_id)
        .bind(account_id)
        .bind(exchange)
        .bind(symbol)
        .bind(position_side)
        .fetch_optional(self.pool())
        .await?;

        Ok(row.map(
            |(
                run_id,
                account_id,
                exchange,
                symbol,
                asset_class,
                margin_mode,
                position_side,
                leverage,
                qty,
                avg_price,
                margin_used,
                funding_fee,
                realized_pnl,
                unrealized_pnl,
                updated_at_ms,
            )| StoredCryptoPosition {
                run_id,
                account_id,
                exchange,
                symbol,
                asset_class,
                margin_mode,
                position_side,
                leverage,
                qty,
                avg_price,
                margin_used,
                funding_fee,
                realized_pnl,
                unrealized_pnl,
                updated_at_ms,
            },
        ))
    }

    pub async fn upsert_funding_rate(&self, rate: NewFundingRate) -> StorageResult<()> {
        sqlx::query(
            r#"
            INSERT INTO funding_rates (
                id, exchange, symbol, funding_time_ms, funding_rate, mark_price, source
            )
            VALUES (?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(exchange, symbol, funding_time_ms) DO UPDATE SET
                id = excluded.id,
                funding_rate = excluded.funding_rate,
                mark_price = excluded.mark_price,
                source = excluded.source
            "#,
        )
        .bind(rate.id)
        .bind(rate.exchange)
        .bind(rate.symbol)
        .bind(rate.funding_time_ms)
        .bind(rate.funding_rate)
        .bind(rate.mark_price)
        .bind(rate.source)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn list_funding_rates(
        &self,
        exchange: &str,
        symbol: Option<&str>,
        start_ms: Option<i64>,
        end_ms: Option<i64>,
    ) -> StorageResult<Vec<StoredFundingRate>> {
        let mut query = QueryBuilder::<Sqlite>::new(
            r#"
            SELECT id, exchange, symbol, funding_time_ms, funding_rate, mark_price, source
            FROM funding_rates
            "#,
        );
        query.push(" WHERE exchange = ");
        query.push_bind(exchange);

        if let Some(symbol) = symbol {
            query.push(" AND symbol = ");
            query.push_bind(symbol);
        }
        if let Some(start_ms) = start_ms {
            query.push(" AND funding_time_ms >= ");
            query.push_bind(start_ms);
        }
        if let Some(end_ms) = end_ms {
            query.push(" AND funding_time_ms < ");
            query.push_bind(end_ms);
        }

        query.push(" ORDER BY funding_time_ms, id");
        let rows = query
            .build_query_as::<(String, String, String, i64, String, Option<String>, String)>()
            .fetch_all(self.pool())
            .await?;

        Ok(rows
            .into_iter()
            .map(
                |(id, exchange, symbol, funding_time_ms, funding_rate, mark_price, source)| {
                    StoredFundingRate {
                        id,
                        exchange,
                        symbol,
                        funding_time_ms,
                        funding_rate,
                        mark_price,
                        source,
                    }
                },
            )
            .collect())
    }

    pub async fn get_latest_funding_rate(
        &self,
        exchange: &str,
        symbol: &str,
    ) -> StorageResult<Option<StoredFundingRate>> {
        let row =
            sqlx::query_as::<_, (String, String, String, i64, String, Option<String>, String)>(
                r#"
            SELECT id, exchange, symbol, funding_time_ms, funding_rate, mark_price, source
            FROM funding_rates
            WHERE exchange = ?
              AND symbol = ?
            ORDER BY funding_time_ms DESC, id DESC
            LIMIT 1
            "#,
            )
            .bind(exchange)
            .bind(symbol)
            .fetch_optional(self.pool())
            .await?;

        Ok(row.map(
            |(id, exchange, symbol, funding_time_ms, funding_rate, mark_price, source)| {
                StoredFundingRate {
                    id,
                    exchange,
                    symbol,
                    funding_time_ms,
                    funding_rate,
                    mark_price,
                    source,
                }
            },
        ))
    }

    pub async fn upsert_crypto_market_meta(&self, meta: NewCryptoMarketMeta) -> StorageResult<()> {
        sqlx::query(
            r#"
            INSERT INTO crypto_market_meta (
                exchange, symbol, base_asset, quote_asset, instrument_type,
                contract_type, contract_size, settlement_asset, min_notional,
                min_qty, max_qty, price_precision, qty_precision, price_tick,
                qty_step, maker_fee_rate, taker_fee_rate, funding_interval_hours,
                max_leverage, margin_modes, is_inverse, is_active, created_at, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(exchange, symbol) DO UPDATE SET
                base_asset = excluded.base_asset,
                quote_asset = excluded.quote_asset,
                instrument_type = excluded.instrument_type,
                contract_type = excluded.contract_type,
                contract_size = excluded.contract_size,
                settlement_asset = excluded.settlement_asset,
                min_notional = excluded.min_notional,
                min_qty = excluded.min_qty,
                max_qty = excluded.max_qty,
                price_precision = excluded.price_precision,
                qty_precision = excluded.qty_precision,
                price_tick = excluded.price_tick,
                qty_step = excluded.qty_step,
                maker_fee_rate = excluded.maker_fee_rate,
                taker_fee_rate = excluded.taker_fee_rate,
                funding_interval_hours = excluded.funding_interval_hours,
                max_leverage = excluded.max_leverage,
                margin_modes = excluded.margin_modes,
                is_inverse = excluded.is_inverse,
                is_active = excluded.is_active,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(meta.exchange)
        .bind(meta.symbol)
        .bind(meta.base_asset)
        .bind(meta.quote_asset)
        .bind(meta.instrument_type)
        .bind(meta.contract_type)
        .bind(meta.contract_size)
        .bind(meta.settlement_asset)
        .bind(meta.min_notional)
        .bind(meta.min_qty)
        .bind(meta.max_qty)
        .bind(meta.price_precision)
        .bind(meta.qty_precision)
        .bind(meta.price_tick)
        .bind(meta.qty_step)
        .bind(meta.maker_fee_rate)
        .bind(meta.taker_fee_rate)
        .bind(meta.funding_interval_hours)
        .bind(meta.max_leverage)
        .bind(meta.margin_modes)
        .bind(meta.is_inverse)
        .bind(meta.is_active)
        .bind(meta.created_at_ms)
        .bind(meta.updated_at_ms)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn find_crypto_market_meta(
        &self,
        exchange: &str,
        symbol: &str,
    ) -> StorageResult<Option<StoredCryptoMarketMeta>> {
        let row = sqlx::query(
            r#"
            SELECT id, exchange, symbol, base_asset, quote_asset, instrument_type,
                   contract_type, contract_size, settlement_asset, min_notional,
                   min_qty, max_qty, price_precision, qty_precision, price_tick,
                   qty_step, maker_fee_rate, taker_fee_rate, funding_interval_hours,
                   max_leverage, margin_modes, is_inverse, is_active,
                   created_at AS created_at_ms, updated_at AS updated_at_ms
            FROM crypto_market_meta
            WHERE exchange = ? AND symbol = ?
            "#,
        )
        .bind(exchange)
        .bind(symbol)
        .fetch_optional(self.pool())
        .await?;

        let Some(row) = row else {
            return Ok(None);
        };
        let is_inverse: i64 = row.try_get("is_inverse")?;
        let is_active: i64 = row.try_get("is_active")?;

        Ok(Some(StoredCryptoMarketMeta {
            id: row.try_get("id")?,
            exchange: row.try_get("exchange")?,
            symbol: row.try_get("symbol")?,
            base_asset: row.try_get("base_asset")?,
            quote_asset: row.try_get("quote_asset")?,
            instrument_type: row.try_get("instrument_type")?,
            contract_type: row.try_get("contract_type")?,
            contract_size: row.try_get("contract_size")?,
            settlement_asset: row.try_get("settlement_asset")?,
            min_notional: row.try_get("min_notional")?,
            min_qty: row.try_get("min_qty")?,
            max_qty: row.try_get("max_qty")?,
            price_precision: row.try_get("price_precision")?,
            qty_precision: row.try_get("qty_precision")?,
            price_tick: row.try_get("price_tick")?,
            qty_step: row.try_get("qty_step")?,
            maker_fee_rate: row.try_get("maker_fee_rate")?,
            taker_fee_rate: row.try_get("taker_fee_rate")?,
            funding_interval_hours: row.try_get("funding_interval_hours")?,
            max_leverage: row.try_get("max_leverage")?,
            margin_modes: row.try_get("margin_modes")?,
            is_inverse: is_inverse != 0,
            is_active: is_active != 0,
            created_at_ms: row.try_get("created_at_ms")?,
            updated_at_ms: row.try_get("updated_at_ms")?,
        }))
    }

    pub async fn insert_corporate_action_meta(
        &self,
        action: NewCorporateActionMeta,
    ) -> StorageResult<()> {
        sqlx::query(
            r#"
            INSERT INTO corporate_actions_meta (
                market, exchange, symbol, action_type, ex_date, record_date,
                payable_date, ratio, cash_amount, currency, source, created_at, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(action.market)
        .bind(action.exchange)
        .bind(action.symbol)
        .bind(action.action_type)
        .bind(action.ex_date_ms)
        .bind(action.record_date_ms)
        .bind(action.payable_date_ms)
        .bind(action.ratio)
        .bind(action.cash_amount)
        .bind(action.currency)
        .bind(action.source)
        .bind(action.created_at_ms)
        .bind(action.updated_at_ms)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn upsert_corporate_action_meta(
        &self,
        action: NewCorporateActionMeta,
    ) -> StorageResult<()> {
        sqlx::query(
            r#"
            INSERT INTO corporate_actions_meta (
                market, exchange, symbol, action_type, ex_date, record_date,
                payable_date, ratio, cash_amount, currency, source, created_at, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(market, exchange, symbol, action_type, ex_date) DO UPDATE SET
                record_date = excluded.record_date,
                payable_date = excluded.payable_date,
                ratio = excluded.ratio,
                cash_amount = excluded.cash_amount,
                currency = excluded.currency,
                source = excluded.source,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(action.market)
        .bind(action.exchange)
        .bind(action.symbol)
        .bind(action.action_type)
        .bind(action.ex_date_ms)
        .bind(action.record_date_ms)
        .bind(action.payable_date_ms)
        .bind(action.ratio)
        .bind(action.cash_amount)
        .bind(action.currency)
        .bind(action.source)
        .bind(action.created_at_ms)
        .bind(action.updated_at_ms)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn list_corporate_actions(
        &self,
        market: &str,
        symbol: &str,
        start_ms: i64,
        end_ms: i64,
    ) -> StorageResult<Vec<StoredCorporateActionMeta>> {
        type CorporateActionRow = (
            i64,
            String,
            String,
            String,
            String,
            i64,
            Option<i64>,
            Option<i64>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            i64,
            i64,
        );

        let rows = sqlx::query_as::<_, CorporateActionRow>(
            r#"
            SELECT id, market, exchange, symbol, action_type, ex_date, record_date,
                   payable_date, ratio, cash_amount, currency, source, created_at, updated_at
            FROM corporate_actions_meta
            WHERE market = ?
              AND symbol = ?
              AND ex_date >= ?
              AND ex_date < ?
            ORDER BY ex_date, id
            "#,
        )
        .bind(market)
        .bind(symbol)
        .bind(start_ms)
        .bind(end_ms)
        .fetch_all(self.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(
                    id,
                    market,
                    exchange,
                    symbol,
                    action_type,
                    ex_date_ms,
                    record_date_ms,
                    payable_date_ms,
                    ratio,
                    cash_amount,
                    currency,
                    source,
                    created_at_ms,
                    updated_at_ms,
                )| StoredCorporateActionMeta {
                    id,
                    market,
                    exchange,
                    symbol,
                    action_type,
                    ex_date_ms,
                    record_date_ms,
                    payable_date_ms,
                    ratio,
                    cash_amount,
                    currency,
                    source,
                    created_at_ms,
                    updated_at_ms,
                },
            )
            .collect())
    }

    pub async fn insert_cash_snapshot(&self, snapshot: NewCashSnapshot) -> StorageResult<()> {
        sqlx::query(
            r#"
            INSERT INTO cash_snapshots (
                run_id, ts, currency, cash, available_cash, frozen_cash, created_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(snapshot.run_id)
        .bind(snapshot.ts_ms)
        .bind(snapshot.currency)
        .bind(snapshot.cash)
        .bind(snapshot.available_cash)
        .bind(snapshot.frozen_cash)
        .bind(snapshot.created_at_ms)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn record_broker_account_balance(
        &self,
        command: BrokerAccountBalanceCommand,
    ) -> StorageResult<()> {
        sqlx::query(
            r#"
            INSERT INTO broker_account_balances (
                run_id, account_id, broker_kind, ts, currency, cash, available_cash,
                frozen_cash, equity, buying_power, margin_used, source_ts, created_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(command.run_id)
        .bind(command.account_id)
        .bind(command.broker_kind)
        .bind(command.ts_ms)
        .bind(command.currency)
        .bind(command.cash.to_string())
        .bind(command.available_cash.to_string())
        .bind(command.frozen_cash.to_string())
        .bind(command.equity.map(|value| value.to_string()))
        .bind(command.buying_power.map(|value| value.to_string()))
        .bind(command.margin_used.map(|value| value.to_string()))
        .bind(command.source_ts_ms)
        .bind(command.ts_ms)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn list_broker_account_balances(
        &self,
        run_id: &str,
    ) -> StorageResult<Vec<StoredBrokerAccountBalance>> {
        let rows = sqlx::query(
            r#"
            SELECT id, run_id, account_id, broker_kind, ts AS ts_ms, currency, cash,
                   available_cash, frozen_cash, equity, buying_power, margin_used,
                   source_ts AS source_ts_ms, created_at AS created_at_ms
            FROM broker_account_balances
            WHERE run_id = ?
            ORDER BY ts, id
            "#,
        )
        .bind(run_id)
        .fetch_all(self.pool())
        .await?;

        rows.into_iter()
            .map(|row| {
                Ok(StoredBrokerAccountBalance {
                    id: row.try_get("id")?,
                    run_id: row.try_get("run_id")?,
                    account_id: row.try_get("account_id")?,
                    broker_kind: row.try_get("broker_kind")?,
                    ts_ms: row.try_get("ts_ms")?,
                    currency: row.try_get("currency")?,
                    cash: row.try_get("cash")?,
                    available_cash: row.try_get("available_cash")?,
                    frozen_cash: row.try_get("frozen_cash")?,
                    equity: row.try_get("equity")?,
                    buying_power: row.try_get("buying_power")?,
                    margin_used: row.try_get("margin_used")?,
                    source_ts_ms: row.try_get("source_ts_ms")?,
                    created_at_ms: row.try_get("created_at_ms")?,
                })
            })
            .collect()
    }

    pub async fn list_cash_snapshots(
        &self,
        run_id: &str,
    ) -> StorageResult<Vec<StoredCashSnapshot>> {
        self.list_cash_snapshots_filtered(run_id, None, None, None)
            .await
    }

    pub async fn list_cash_snapshots_filtered(
        &self,
        run_id: &str,
        currency: Option<&str>,
        from_ms: Option<i64>,
        to_ms: Option<i64>,
    ) -> StorageResult<Vec<StoredCashSnapshot>> {
        let mut query_builder = QueryBuilder::<Sqlite>::new(
            "SELECT id, run_id, ts, currency, cash, available_cash, frozen_cash, created_at \
             FROM cash_snapshots WHERE run_id = ",
        );
        query_builder.push_bind(run_id);
        if let Some(currency) = currency {
            query_builder.push(" AND currency = ");
            query_builder.push_bind(currency);
        }
        if let Some(from_ms) = from_ms {
            query_builder.push(" AND ts >= ");
            query_builder.push_bind(from_ms);
        }
        if let Some(to_ms) = to_ms {
            query_builder.push(" AND ts <= ");
            query_builder.push_bind(to_ms);
        }
        query_builder.push(" ORDER BY ts, id");

        let rows = query_builder
            .build_query_as::<(i64, String, i64, String, String, String, String, i64)>()
            .fetch_all(self.pool())
            .await?;

        Ok(rows
            .into_iter()
            .map(
                |(
                    id,
                    run_id,
                    ts_ms,
                    currency,
                    cash,
                    available_cash,
                    frozen_cash,
                    created_at_ms,
                )| {
                    StoredCashSnapshot {
                        id,
                        run_id,
                        ts_ms,
                        currency,
                        cash,
                        available_cash,
                        frozen_cash,
                        created_at_ms,
                    }
                },
            )
            .collect())
    }

    pub async fn get_latest_cash_snapshot(
        &self,
        run_id: &str,
        currency: Option<&str>,
    ) -> StorageResult<Option<StoredCashSnapshot>> {
        let mut query_builder = QueryBuilder::<Sqlite>::new(
            "SELECT id, run_id, ts, currency, cash, available_cash, frozen_cash, created_at \
             FROM cash_snapshots WHERE run_id = ",
        );
        query_builder.push_bind(run_id);
        if let Some(currency) = currency {
            query_builder.push(" AND currency = ");
            query_builder.push_bind(currency);
        }
        query_builder.push(" ORDER BY ts DESC, id DESC LIMIT 1");

        let row = query_builder
            .build_query_as::<(i64, String, i64, String, String, String, String, i64)>()
            .fetch_optional(self.pool())
            .await?;

        Ok(row.map(
            |(id, run_id, ts_ms, currency, cash, available_cash, frozen_cash, created_at_ms)| {
                StoredCashSnapshot {
                    id,
                    run_id,
                    ts_ms,
                    currency,
                    cash,
                    available_cash,
                    frozen_cash,
                    created_at_ms,
                }
            },
        ))
    }

    pub async fn insert_position_snapshot(
        &self,
        snapshot: NewPositionSnapshot,
    ) -> StorageResult<()> {
        sqlx::query(
            r#"
            INSERT INTO position_snapshots (
                run_id, ts, market, exchange, symbol, asset_class, position_side,
                qty, available_qty, avg_price, entry_price, market_price, mark_price,
                market_value, unrealized_pnl, realized_pnl, currency, contract_metadata_json,
                liquidation_price, open_interest, created_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(snapshot.run_id)
        .bind(snapshot.ts_ms)
        .bind(snapshot.market)
        .bind(snapshot.exchange)
        .bind(snapshot.symbol)
        .bind(snapshot.asset_class)
        .bind(snapshot.position_side)
        .bind(snapshot.qty)
        .bind(snapshot.available_qty)
        .bind(snapshot.avg_price)
        .bind(snapshot.entry_price)
        .bind(snapshot.market_price)
        .bind(snapshot.mark_price)
        .bind(snapshot.market_value)
        .bind(snapshot.unrealized_pnl)
        .bind(snapshot.realized_pnl)
        .bind(snapshot.currency)
        .bind(snapshot.contract_metadata_json)
        .bind(snapshot.liquidation_price)
        .bind(snapshot.open_interest)
        .bind(snapshot.created_at_ms)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn list_position_snapshots(
        &self,
        run_id: &str,
    ) -> StorageResult<Vec<StoredPositionSnapshot>> {
        self.list_position_snapshots_filtered(run_id, None, None, None, None)
            .await
    }

    pub async fn list_position_snapshots_filtered(
        &self,
        run_id: &str,
        symbol: Option<&str>,
        position_side: Option<&str>,
        from_ms: Option<i64>,
        to_ms: Option<i64>,
    ) -> StorageResult<Vec<StoredPositionSnapshot>> {
        let mut query_builder = QueryBuilder::<Sqlite>::new(
            "SELECT id, run_id, ts AS ts_ms, market, exchange, symbol, asset_class, position_side, \
             qty, available_qty, avg_price, entry_price, market_price, mark_price, \
             market_value, unrealized_pnl, realized_pnl, currency, contract_metadata_json, \
             liquidation_price, open_interest, created_at AS created_at_ms \
             FROM position_snapshots WHERE run_id = ",
        );
        query_builder.push_bind(run_id);
        if let Some(symbol) = symbol {
            query_builder.push(" AND symbol = ");
            query_builder.push_bind(symbol);
        }
        if let Some(position_side) = position_side {
            query_builder.push(" AND position_side = ");
            query_builder.push_bind(position_side);
        }
        if let Some(from_ms) = from_ms {
            query_builder.push(" AND ts >= ");
            query_builder.push_bind(from_ms);
        }
        if let Some(to_ms) = to_ms {
            query_builder.push(" AND ts <= ");
            query_builder.push_bind(to_ms);
        }
        query_builder.push(" ORDER BY ts, id");

        let rows = query_builder.build().fetch_all(self.pool()).await?;
        let mut snapshots = Vec::with_capacity(rows.len());
        for row in rows {
            snapshots.push(StoredPositionSnapshot {
                id: row.try_get("id")?,
                run_id: row.try_get("run_id")?,
                ts_ms: row.try_get("ts_ms")?,
                market: row.try_get("market")?,
                exchange: row.try_get("exchange")?,
                symbol: row.try_get("symbol")?,
                asset_class: row.try_get("asset_class")?,
                position_side: row.try_get("position_side")?,
                qty: row.try_get("qty")?,
                available_qty: row.try_get("available_qty")?,
                avg_price: row.try_get("avg_price")?,
                entry_price: row.try_get("entry_price")?,
                market_price: row.try_get("market_price")?,
                mark_price: row.try_get("mark_price")?,
                market_value: row.try_get("market_value")?,
                unrealized_pnl: row.try_get("unrealized_pnl")?,
                realized_pnl: row.try_get("realized_pnl")?,
                currency: row.try_get("currency")?,
                contract_metadata_json: row.try_get("contract_metadata_json")?,
                liquidation_price: row.try_get("liquidation_price")?,
                open_interest: row.try_get("open_interest")?,
                created_at_ms: row.try_get("created_at_ms")?,
            });
        }
        Ok(snapshots)
    }

    pub async fn record_reconciliation_audit(
        &self,
        command: ReconciliationAuditCommand,
    ) -> StorageResult<()> {
        sqlx::query(
            r#"
            INSERT INTO broker_reconciliation_audits (
                id, run_id, account_id, broker_kind, ts, severity, cash_drift_count,
                position_drift_count, open_order_drift_count, execution_drift_count,
                stale_input_count, payload_json, created_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(command.id)
        .bind(command.run_id)
        .bind(command.account_id)
        .bind(command.broker_kind)
        .bind(command.ts_ms)
        .bind(command.severity)
        .bind(command.cash_drift_count)
        .bind(command.position_drift_count)
        .bind(command.open_order_drift_count)
        .bind(command.execution_drift_count)
        .bind(command.stale_input_count)
        .bind(command.payload_json)
        .bind(command.ts_ms)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn list_reconciliation_audits(
        &self,
        run_id: &str,
    ) -> StorageResult<Vec<StoredReconciliationAudit>> {
        let rows = sqlx::query(
            r#"
            SELECT id, run_id, account_id, broker_kind, ts AS ts_ms, severity,
                   cash_drift_count, position_drift_count, open_order_drift_count,
                   execution_drift_count, stale_input_count, payload_json,
                   created_at AS created_at_ms
            FROM broker_reconciliation_audits
            WHERE run_id = ?
            ORDER BY ts, id
            "#,
        )
        .bind(run_id)
        .fetch_all(self.pool())
        .await?;

        rows.into_iter()
            .map(|row| {
                Ok(StoredReconciliationAudit {
                    id: row.try_get("id")?,
                    run_id: row.try_get("run_id")?,
                    account_id: row.try_get("account_id")?,
                    broker_kind: row.try_get("broker_kind")?,
                    ts_ms: row.try_get("ts_ms")?,
                    severity: row.try_get("severity")?,
                    cash_drift_count: row.try_get("cash_drift_count")?,
                    position_drift_count: row.try_get("position_drift_count")?,
                    open_order_drift_count: row.try_get("open_order_drift_count")?,
                    execution_drift_count: row.try_get("execution_drift_count")?,
                    stale_input_count: row.try_get("stale_input_count")?,
                    payload_json: row.try_get("payload_json")?,
                    created_at_ms: row.try_get("created_at_ms")?,
                })
            })
            .collect()
    }

    pub async fn list_latest_reconciliation_audits_for_gate(
        &self,
        broker_kind: &str,
        account_id: &str,
        limit: i64,
    ) -> StorageResult<Vec<StoredReconciliationAudit>> {
        let rows = sqlx::query(
            r#"
            SELECT id, run_id, account_id, broker_kind, ts AS ts_ms, severity,
                   cash_drift_count, position_drift_count, open_order_drift_count,
                   execution_drift_count, stale_input_count, payload_json,
                   created_at AS created_at_ms
            FROM broker_reconciliation_audits
            WHERE broker_kind = ? AND account_id = ?
            ORDER BY ts DESC, id DESC
            LIMIT ?
            "#,
        )
        .bind(broker_kind)
        .bind(account_id)
        .bind(limit)
        .fetch_all(self.pool())
        .await?;

        rows.into_iter()
            .map(|row| {
                Ok(StoredReconciliationAudit {
                    id: row.try_get("id")?,
                    run_id: row.try_get("run_id")?,
                    account_id: row.try_get("account_id")?,
                    broker_kind: row.try_get("broker_kind")?,
                    ts_ms: row.try_get("ts_ms")?,
                    severity: row.try_get("severity")?,
                    cash_drift_count: row.try_get("cash_drift_count")?,
                    position_drift_count: row.try_get("position_drift_count")?,
                    open_order_drift_count: row.try_get("open_order_drift_count")?,
                    execution_drift_count: row.try_get("execution_drift_count")?,
                    stale_input_count: row.try_get("stale_input_count")?,
                    payload_json: row.try_get("payload_json")?,
                    created_at_ms: row.try_get("created_at_ms")?,
                })
            })
            .collect()
    }

    pub async fn list_reconciliation_audits_for_gate_since(
        &self,
        broker_kind: &str,
        account_id: &str,
        from_ts_ms: i64,
    ) -> StorageResult<Vec<StoredReconciliationAudit>> {
        let rows = sqlx::query(
            r#"
            SELECT id, run_id, account_id, broker_kind, ts AS ts_ms, severity,
                   cash_drift_count, position_drift_count, open_order_drift_count,
                   execution_drift_count, stale_input_count, payload_json,
                   created_at AS created_at_ms
            FROM broker_reconciliation_audits
            WHERE broker_kind = ? AND account_id = ? AND ts >= ?
            ORDER BY ts DESC, id DESC
            "#,
        )
        .bind(broker_kind)
        .bind(account_id)
        .bind(from_ts_ms)
        .fetch_all(self.pool())
        .await?;

        rows.into_iter()
            .map(|row| {
                Ok(StoredReconciliationAudit {
                    id: row.try_get("id")?,
                    run_id: row.try_get("run_id")?,
                    account_id: row.try_get("account_id")?,
                    broker_kind: row.try_get("broker_kind")?,
                    ts_ms: row.try_get("ts_ms")?,
                    severity: row.try_get("severity")?,
                    cash_drift_count: row.try_get("cash_drift_count")?,
                    position_drift_count: row.try_get("position_drift_count")?,
                    open_order_drift_count: row.try_get("open_order_drift_count")?,
                    execution_drift_count: row.try_get("execution_drift_count")?,
                    stale_input_count: row.try_get("stale_input_count")?,
                    payload_json: row.try_get("payload_json")?,
                    created_at_ms: row.try_get("created_at_ms")?,
                })
            })
            .collect()
    }

    pub async fn get_latest_position_snapshot(
        &self,
        run_id: &str,
        symbol: &str,
        position_side: Option<&str>,
    ) -> StorageResult<Option<StoredPositionSnapshot>> {
        let mut snapshots = self
            .list_position_snapshots_filtered(run_id, Some(symbol), position_side, None, None)
            .await?;
        snapshots.sort_by(|left, right| {
            right
                .ts_ms
                .cmp(&left.ts_ms)
                .then_with(|| right.id.cmp(&left.id))
        });
        Ok(snapshots.into_iter().next())
    }

    pub async fn upsert_config(&self, config: NewConfigRecord) -> StorageResult<()> {
        sqlx::query(
            r#"
            INSERT INTO configs (
                id, name, config_type, content, format, checksum, created_at, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                config_type = excluded.config_type,
                content = excluded.content,
                format = excluded.format,
                checksum = excluded.checksum,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(config.id)
        .bind(config.name)
        .bind(config.config_type)
        .bind(config.content)
        .bind(config.format)
        .bind(config.checksum)
        .bind(config.created_at_ms)
        .bind(config.updated_at_ms)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn record_config(&self, command: ConfigRecordCommand) -> StorageResult<()> {
        self.upsert_config(NewConfigRecord {
            id: command.id,
            name: command.name,
            config_type: command.config_type,
            content: command.content,
            format: command.format,
            checksum: command.checksum,
            created_at_ms: command.ts_ms,
            updated_at_ms: command.ts_ms,
        })
        .await
    }

    pub async fn record_run_config_snapshot(
        &self,
        command: RunConfigSnapshotCommand,
    ) -> StorageResult<()> {
        let config_id = format!("run:{}", command.run_id);
        let version = command
            .checksum
            .clone()
            .unwrap_or_else(|| format!("ts:{}", command.ts_ms));
        self.upsert_config(NewConfigRecord {
            id: config_id.clone(),
            name: command.run_id.clone(),
            config_type: "RUN".to_string(),
            content: command.content,
            format: command.format,
            checksum: command.checksum,
            created_at_ms: command.ts_ms,
            updated_at_ms: command.ts_ms,
        })
        .await?;
        self.record_config_release(ConfigReleaseCommand {
            config_id: config_id.clone(),
            version: version.clone(),
            status: "released".to_string(),
            released_by: Some("runtime".to_string()),
            notes: Some("run config snapshot".to_string()),
            ts_ms: command.ts_ms,
        })
        .await?;
        self.bind_run_config_version(RunConfigVersionBindingCommand {
            run_id: command.run_id,
            config_id,
            version,
            ts_ms: command.ts_ms,
        })
        .await
    }

    pub async fn get_config_by_name(
        &self,
        name: &str,
    ) -> StorageResult<Option<StoredConfigRecord>> {
        let row = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                String,
                String,
                Option<String>,
                i64,
                i64,
            ),
        >(
            r#"
            SELECT id, name, config_type, content, format, checksum, created_at, updated_at
            FROM configs
            WHERE name = ?
            ORDER BY updated_at DESC, id
            LIMIT 1
            "#,
        )
        .bind(name)
        .fetch_optional(self.pool())
        .await?;

        Ok(row.map(
            |(id, name, config_type, content, format, checksum, created_at_ms, updated_at_ms)| {
                StoredConfigRecord {
                    id,
                    name,
                    config_type,
                    content,
                    format,
                    checksum,
                    created_at_ms,
                    updated_at_ms,
                }
            },
        ))
    }

    pub async fn list_configs(&self) -> StorageResult<Vec<StoredConfigRecord>> {
        let rows = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                String,
                String,
                Option<String>,
                i64,
                i64,
            ),
        >(
            r#"
            SELECT id, name, config_type, content, format, checksum, created_at, updated_at
            FROM configs
            ORDER BY updated_at DESC, id
            "#,
        )
        .fetch_all(self.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(
                    id,
                    name,
                    config_type,
                    content,
                    format,
                    checksum,
                    created_at_ms,
                    updated_at_ms,
                )| {
                    StoredConfigRecord {
                        id,
                        name,
                        config_type,
                        content,
                        format,
                        checksum,
                        created_at_ms,
                        updated_at_ms,
                    }
                },
            )
            .collect())
    }

    pub async fn create_config_version(&self, config: NewConfigVersion) -> StorageResult<u32> {
        serde_json::from_str::<serde_json::Value>(&config.content_json).map_err(|error| {
            StorageError::Protocol(format!("invalid config JSON content: {error}"))
        })?;

        if let Some(parent_version) = config.parent_version {
            if self
                .get_config(&config.name, parent_version)
                .await?
                .is_none()
            {
                return Err(StorageError::Protocol(format!(
                    "parent config version {}:{} does not exist",
                    config.name, parent_version
                )));
            }
        }

        let (latest_version,) = sqlx::query_as::<_, (i64,)>(
            r#"
            SELECT COALESCE(MAX(lifecycle_version), 0)
            FROM configs
            WHERE name = ? AND lifecycle_version IS NOT NULL
            "#,
        )
        .bind(&config.name)
        .fetch_one(self.pool())
        .await?;
        let version = u32::try_from(latest_version + 1)
            .map_err(|error| StorageError::Protocol(format!("config version overflow: {error}")))?;
        let config_id = config_version_id(&config.name, version);

        sqlx::query(
            r#"
            INSERT INTO configs (
                id, name, config_type, content, format, checksum, created_at, updated_at,
                lifecycle_version, state, parent_version, created_by, state_changed_at,
                state_changed_by, state_change_reason, target_env, rollout
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&config_id)
        .bind(&config.name)
        .bind("MANAGED")
        .bind(&config.content_json)
        .bind("JSON")
        .bind(Option::<String>::None)
        .bind(config.ts_ms)
        .bind(config.ts_ms)
        .bind(i64::from(version))
        .bind(ConfigState::Draft.as_str())
        .bind(config.parent_version.map(i64::from))
        .bind(&config.created_by)
        .bind(config.ts_ms)
        .bind(&config.created_by)
        .bind(Option::<String>::None)
        .bind(&config.target_env)
        .bind(&config.rollout)
        .execute(self.pool())
        .await?;

        self.record_config_release(ConfigReleaseCommand {
            config_id,
            version: version.to_string(),
            status: ConfigState::Draft.as_str().to_string(),
            released_by: Some(config.created_by),
            notes: Some("created config draft".to_string()),
            ts_ms: config.ts_ms,
        })
        .await?;

        Ok(version)
    }

    pub async fn get_config(
        &self,
        name: &str,
        version: u32,
    ) -> StorageResult<Option<ConfigVersion>> {
        let row = sqlx::query_as::<_, ConfigVersionRow>(
            r#"
            SELECT
                id, name, lifecycle_version, content, state, parent_version,
                created_by, created_at, state_changed_at, state_changed_by,
                state_change_reason, target_env, rollout, approved_by, approved_at,
                published_by, published_at
            FROM configs
            WHERE name = ? AND lifecycle_version = ?
            "#,
        )
        .bind(name)
        .bind(i64::from(version))
        .fetch_optional(self.pool())
        .await?;

        row.map(config_version_from_row).transpose()
    }

    pub async fn get_latest_config(&self, name: &str) -> StorageResult<Option<ConfigVersion>> {
        let row = sqlx::query_as::<_, ConfigVersionRow>(
            r#"
            SELECT
                id, name, lifecycle_version, content, state, parent_version,
                created_by, created_at, state_changed_at, state_changed_by,
                state_change_reason, target_env, rollout, approved_by, approved_at,
                published_by, published_at
            FROM configs
            WHERE name = ? AND lifecycle_version IS NOT NULL
            ORDER BY lifecycle_version DESC
            LIMIT 1
            "#,
        )
        .bind(name)
        .fetch_optional(self.pool())
        .await?;

        row.map(config_version_from_row).transpose()
    }

    pub async fn get_published_config(&self, name: &str) -> StorageResult<Option<ConfigVersion>> {
        let row = sqlx::query_as::<_, ConfigVersionRow>(
            r#"
            SELECT
                id, name, lifecycle_version, content, state, parent_version,
                created_by, created_at, state_changed_at, state_changed_by,
                state_change_reason, target_env, rollout, approved_by, approved_at,
                published_by, published_at
            FROM configs
            WHERE name = ? AND lifecycle_version IS NOT NULL AND state = ?
            ORDER BY lifecycle_version DESC
            LIMIT 1
            "#,
        )
        .bind(name)
        .bind(ConfigState::Published.as_str())
        .fetch_optional(self.pool())
        .await?;

        row.map(config_version_from_row).transpose()
    }

    pub async fn list_config_versions(&self, name: &str) -> StorageResult<Vec<ConfigVersion>> {
        let rows = sqlx::query_as::<_, ConfigVersionRow>(
            r#"
            SELECT
                id, name, lifecycle_version, content, state, parent_version,
                created_by, created_at, state_changed_at, state_changed_by,
                state_change_reason, target_env, rollout, approved_by, approved_at,
                published_by, published_at
            FROM configs
            WHERE name = ? AND lifecycle_version IS NOT NULL
            ORDER BY lifecycle_version ASC
            "#,
        )
        .bind(name)
        .fetch_all(self.pool())
        .await?;

        rows.into_iter().map(config_version_from_row).collect()
    }

    pub async fn list_pending_config_approvals(
        &self,
        target_env: Option<&str>,
    ) -> StorageResult<Vec<ConfigVersion>> {
        let mut query_builder = QueryBuilder::<Sqlite>::new(
            r#"
            SELECT
                id, name, lifecycle_version, content, state, parent_version,
                created_by, created_at, state_changed_at, state_changed_by,
                state_change_reason, target_env, rollout, approved_by, approved_at,
                published_by, published_at
            FROM configs
            WHERE lifecycle_version IS NOT NULL AND state =
            "#,
        );
        query_builder.push_bind(ConfigState::PendingReview.as_str());
        if let Some(target_env) = target_env {
            query_builder.push(" AND target_env = ");
            query_builder.push_bind(target_env);
        }
        query_builder.push(" ORDER BY state_changed_at ASC, lifecycle_version ASC");

        let rows = query_builder
            .build_query_as::<ConfigVersionRow>()
            .fetch_all(self.pool())
            .await?;
        rows.into_iter().map(config_version_from_row).collect()
    }

    pub fn list_config_governance_policy(&self) -> ConfigGovernancePolicy {
        ConfigGovernancePolicy {
            rules: config_governance_policy_rules(),
        }
    }

    pub async fn list_config_approval_queue(
        &self,
        target_env: Option<&str>,
    ) -> StorageResult<Vec<ConfigApprovalQueueEntry>> {
        let mut query_builder = QueryBuilder::<Sqlite>::new(
            r#"
            SELECT
                id, name, lifecycle_version, content, state, parent_version,
                created_by, created_at, state_changed_at, state_changed_by,
                state_change_reason, target_env, rollout, approved_by, approved_at,
                published_by, published_at
            FROM configs
            WHERE lifecycle_version IS NOT NULL
                AND state IN (
            "#,
        );
        query_builder.push_bind(ConfigState::PendingReview.as_str());
        query_builder.push(", ");
        query_builder.push_bind(ConfigState::Approved.as_str());
        query_builder.push(")");
        if let Some(target_env) = target_env {
            query_builder.push(" AND target_env = ");
            query_builder.push_bind(target_env);
        }
        query_builder.push(" ORDER BY state_changed_at ASC, lifecycle_version ASC");

        let rows = query_builder
            .build_query_as::<ConfigVersionRow>()
            .fetch_all(self.pool())
            .await?;
        let configs = rows
            .into_iter()
            .map(config_version_from_row)
            .collect::<StorageResult<Vec<_>>>()?;
        let mut entries = Vec::new();
        for config in configs {
            let Some(target_env) = config_policy_environment(config.target_env.as_deref()) else {
                continue;
            };
            let Some(approval_rule) = config_governance_rule(target_env, ConfigState::Approved)
            else {
                continue;
            };
            let Some(publish_rule) = config_governance_rule(target_env, ConfigState::Published)
            else {
                continue;
            };
            let approval_count = self
                .count_config_approvals(&config.id, &config.version.to_string(), None)
                .await?;
            if config.state == ConfigState::Approved
                && approval_count >= publish_rule.required_approvals
            {
                continue;
            }
            entries.push(ConfigApprovalQueueEntry {
                config,
                required_role: approval_rule.required_role,
                required_approvals: publish_rule.required_approvals,
                approval_count,
                remaining_approvals: publish_rule
                    .required_approvals
                    .saturating_sub(approval_count),
            });
        }

        Ok(entries)
    }

    pub async fn update_config_state_with_policy(
        &self,
        name: &str,
        version: u32,
        new_state: ConfigState,
        changed_by: &str,
        actor_role: &str,
        reason: Option<&str>,
        ts_ms: i64,
    ) -> StorageResult<()> {
        let Some(current) = self.get_config(name, version).await? else {
            return Err(StorageError::Protocol(format!(
                "config version {name}:{version} does not exist"
            )));
        };
        let role = ConfigActorRole::from_str(actor_role)?;
        if let Some(target_env) = config_policy_environment(current.target_env.as_deref()) {
            let Some(rule) = config_governance_rule(target_env, new_state) else {
                return Err(StorageError::Protocol(format!(
                    "{target_env} config {} is not governed by policy",
                    new_state.as_str()
                )));
            };
            if !role.can_satisfy(&rule.required_role) {
                return Err(StorageError::Protocol(format!(
                    "{target_env} config {} requires role {}",
                    new_state.as_str(),
                    rule.required_role
                )));
            }
        }
        self.update_config_state_inner(
            name,
            version,
            new_state,
            changed_by,
            Some(role.as_str()),
            reason,
            ts_ms,
        )
        .await
    }

    pub async fn update_config_state(
        &self,
        name: &str,
        version: u32,
        new_state: ConfigState,
        changed_by: &str,
        reason: Option<&str>,
        ts_ms: i64,
    ) -> StorageResult<()> {
        self.update_config_state_inner(name, version, new_state, changed_by, None, reason, ts_ms)
            .await
    }

    async fn update_config_state_inner(
        &self,
        name: &str,
        version: u32,
        new_state: ConfigState,
        changed_by: &str,
        actor_role: Option<&str>,
        reason: Option<&str>,
        ts_ms: i64,
    ) -> StorageResult<()> {
        let Some(current) = self.get_config(name, version).await? else {
            return Err(StorageError::Protocol(format!(
                "config version {name}:{version} does not exist"
            )));
        };
        let is_reapproval =
            current.state == ConfigState::Approved && new_state == ConfigState::Approved;
        if current.state == new_state && !is_reapproval {
            return Ok(());
        }
        if !is_reapproval && !current.state.can_transition_to(new_state) {
            return Err(StorageError::Protocol(format!(
                "invalid config state transition {} -> {}",
                current.state.as_str(),
                new_state.as_str()
            )));
        }
        if new_state == ConfigState::Published
            && current.target_env.as_deref() == Some("production")
        {
            let publish_rule = config_governance_rule("production", ConfigState::Published)
                .ok_or_else(|| {
                    StorageError::Protocol(
                        "production config publish policy is missing".to_string(),
                    )
                })?;
            let approval_count = self
                .count_config_approvals(
                    &current.id,
                    &version.to_string(),
                    publish_rule
                        .requires_independent_actor
                        .then_some(changed_by),
                )
                .await?;
            if approval_count < publish_rule.required_approvals {
                return Err(StorageError::Protocol(format!(
                    "production config publish requires {} approvals; found {}",
                    publish_rule.required_approvals, approval_count
                )));
            }
        }

        let approved_by = (new_state == ConfigState::Approved).then(|| changed_by.to_string());
        let approved_at = (new_state == ConfigState::Approved).then_some(ts_ms);
        let published_by = (new_state == ConfigState::Published).then(|| changed_by.to_string());
        let published_at = (new_state == ConfigState::Published).then_some(ts_ms);

        sqlx::query(
            r#"
            UPDATE configs
            SET state = ?, updated_at = ?, state_changed_at = ?,
                state_changed_by = ?, state_change_reason = ?,
                approved_by = COALESCE(?, approved_by),
                approved_at = COALESCE(?, approved_at),
                published_by = COALESCE(?, published_by),
                published_at = COALESCE(?, published_at)
            WHERE name = ? AND lifecycle_version = ?
            "#,
        )
        .bind(new_state.as_str())
        .bind(ts_ms)
        .bind(ts_ms)
        .bind(changed_by)
        .bind(reason)
        .bind(approved_by.as_deref())
        .bind(approved_at)
        .bind(published_by.as_deref())
        .bind(published_at)
        .bind(name)
        .bind(i64::from(version))
        .execute(self.pool())
        .await?;

        if new_state == ConfigState::Approved {
            self.record_config_approval(
                &current,
                version,
                changed_by,
                actor_role.unwrap_or("approver"),
                reason,
                ts_ms,
            )
            .await?;
        }

        self.record_config_release(ConfigReleaseCommand {
            config_id: current.id.clone(),
            version: version.to_string(),
            status: new_state.as_str().to_string(),
            released_by: Some(changed_by.to_string()),
            notes: reason.map(str::to_string),
            ts_ms,
        })
        .await?;
        self.record_config_audit(ConfigAuditCommand {
            config_id: current.id.clone(),
            version: Some(version.to_string()),
            action: "state_changed".to_string(),
            actor: Some(changed_by.to_string()),
            reason: reason.map(str::to_string),
            ts_ms,
        })
        .await?;
        self.insert_event(NewEventRecord {
            event_id: Uuid::new_v4().to_string(),
            ts_ms,
            source: current.id,
            category: "config.state.changed".to_string(),
            payload_json: serde_json::json!({
                "name": name,
                "version": version,
                "old_state": current.state.as_str(),
                "new_state": new_state.as_str(),
                "changed_by": changed_by,
                "reason": reason,
                "target_env": current.target_env,
                "rollout": current.rollout,
                "approved_by": if new_state == ConfigState::Approved {
                    Some(changed_by)
                } else {
                    current.approved_by.as_deref()
                },
                "published_by": if new_state == ConfigState::Published {
                    Some(changed_by)
                } else {
                    current.published_by.as_deref()
                },
            })
            .to_string(),
        })
        .await
    }

    pub async fn diff_configs(
        &self,
        name: &str,
        version_a: u32,
        version_b: u32,
    ) -> StorageResult<ConfigDiff> {
        let config_a = self.get_config(name, version_a).await?.ok_or_else(|| {
            StorageError::Protocol(format!("config version {name}:{version_a} does not exist"))
        })?;
        let config_b = self.get_config(name, version_b).await?.ok_or_else(|| {
            StorageError::Protocol(format!("config version {name}:{version_b} does not exist"))
        })?;
        let value_a =
            serde_json::from_str::<serde_json::Value>(&config_a.content_json).map_err(|error| {
                StorageError::Protocol(format!(
                    "invalid config JSON for {name}:{version_a}: {error}"
                ))
            })?;
        let value_b =
            serde_json::from_str::<serde_json::Value>(&config_b.content_json).map_err(|error| {
                StorageError::Protocol(format!(
                    "invalid config JSON for {name}:{version_b}: {error}"
                ))
            })?;
        let mut flat_a = BTreeMap::new();
        let mut flat_b = BTreeMap::new();
        flatten_json("", &value_a, &mut flat_a);
        flatten_json("", &value_b, &mut flat_b);

        let added = flat_b
            .keys()
            .filter(|path| !flat_a.contains_key(*path))
            .cloned()
            .collect();
        let removed = flat_a
            .keys()
            .filter(|path| !flat_b.contains_key(*path))
            .cloned()
            .collect();
        let changed = flat_a
            .iter()
            .filter_map(|(path, before)| {
                flat_b.get(path).and_then(|after| {
                    (before != after).then(|| ConfigDiffEntry {
                        path: path.clone(),
                        before: before.clone(),
                        after: after.clone(),
                    })
                })
            })
            .collect();

        Ok(ConfigDiff {
            name: name.to_string(),
            version_a,
            version_b,
            added,
            removed,
            changed,
        })
    }

    pub async fn rollback_config_version(
        &self,
        name: &str,
        version: u32,
        actor: &str,
        reason: Option<&str>,
        ts_ms: i64,
    ) -> StorageResult<u32> {
        let source = self.get_config(name, version).await?.ok_or_else(|| {
            StorageError::Protocol(format!("config version {name}:{version} does not exist"))
        })?;
        let rollback_version = self
            .create_config_version(NewConfigVersion {
                name: name.to_string(),
                content_json: source.content_json,
                created_by: actor.to_string(),
                parent_version: Some(version),
                target_env: source.target_env,
                rollout: source.rollout,
                ts_ms,
            })
            .await?;
        let rollback = self
            .get_config(name, rollback_version)
            .await?
            .ok_or_else(|| {
                StorageError::Protocol(format!(
                    "rollback config version {name}:{rollback_version} was not created"
                ))
            })?;
        self.record_config_audit(ConfigAuditCommand {
            config_id: rollback.id,
            version: Some(rollback_version.to_string()),
            action: "rollback".to_string(),
            actor: Some(actor.to_string()),
            reason: reason.map(str::to_string),
            ts_ms,
        })
        .await?;
        Ok(rollback_version)
    }

    pub async fn record_config_release(&self, command: ConfigReleaseCommand) -> StorageResult<()> {
        sqlx::query(
            r#"
            INSERT INTO config_releases (
                id, config_id, version, status, released_by, notes, created_at, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(config_id, version) DO UPDATE SET
                status = excluded.status,
                released_by = excluded.released_by,
                notes = excluded.notes,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(format!("{}:{}", command.config_id, command.version))
        .bind(command.config_id)
        .bind(command.version)
        .bind(command.status)
        .bind(command.released_by)
        .bind(command.notes)
        .bind(command.ts_ms)
        .bind(command.ts_ms)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn get_config_release(
        &self,
        config_id: &str,
        version: &str,
    ) -> StorageResult<Option<StoredConfigRelease>> {
        let row = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                String,
                Option<String>,
                Option<String>,
                i64,
                i64,
            ),
        >(
            r#"
            SELECT id, config_id, version, status, released_by, notes, created_at, updated_at
            FROM config_releases
            WHERE config_id = ? AND version = ?
            "#,
        )
        .bind(config_id)
        .bind(version)
        .fetch_optional(self.pool())
        .await?;

        Ok(row.map(
            |(id, config_id, version, status, released_by, notes, created_at_ms, updated_at_ms)| {
                StoredConfigRelease {
                    id,
                    config_id,
                    version,
                    status,
                    released_by,
                    notes,
                    created_at_ms,
                    updated_at_ms,
                }
            },
        ))
    }

    pub async fn list_config_releases(
        &self,
        config_id: &str,
    ) -> StorageResult<Vec<StoredConfigRelease>> {
        let rows = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                String,
                Option<String>,
                Option<String>,
                i64,
                i64,
            ),
        >(
            r#"
            SELECT id, config_id, version, status, released_by, notes, created_at, updated_at
            FROM config_releases
            WHERE config_id = ?
            ORDER BY updated_at DESC, version DESC
            "#,
        )
        .bind(config_id)
        .fetch_all(self.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(
                    id,
                    config_id,
                    version,
                    status,
                    released_by,
                    notes,
                    created_at_ms,
                    updated_at_ms,
                )| StoredConfigRelease {
                    id,
                    config_id,
                    version,
                    status,
                    released_by,
                    notes,
                    created_at_ms,
                    updated_at_ms,
                },
            )
            .collect())
    }

    pub async fn bind_run_config_version(
        &self,
        command: RunConfigVersionBindingCommand,
    ) -> StorageResult<()> {
        sqlx::query(
            r#"
            INSERT INTO run_config_versions (run_id, config_id, version, bound_at)
            VALUES (?, ?, ?, ?)
            ON CONFLICT(run_id) DO UPDATE SET
                config_id = excluded.config_id,
                version = excluded.version,
                bound_at = excluded.bound_at
            "#,
        )
        .bind(command.run_id)
        .bind(command.config_id)
        .bind(command.version)
        .bind(command.ts_ms)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn get_run_config_version_binding(
        &self,
        run_id: &str,
    ) -> StorageResult<Option<StoredRunConfigVersionBinding>> {
        let row = sqlx::query_as::<_, (String, String, String, i64)>(
            r#"
            SELECT run_id, config_id, version, bound_at
            FROM run_config_versions
            WHERE run_id = ?
            "#,
        )
        .bind(run_id)
        .fetch_optional(self.pool())
        .await?;

        Ok(row.map(
            |(run_id, config_id, version, bound_at_ms)| StoredRunConfigVersionBinding {
                run_id,
                config_id,
                version,
                bound_at_ms,
            },
        ))
    }

    pub async fn record_config_audit(&self, command: ConfigAuditCommand) -> StorageResult<()> {
        sqlx::query(
            r#"
            INSERT INTO config_audits (
                id, config_id, version, action, actor, reason, ts
            )
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(Uuid::new_v4().to_string())
        .bind(command.config_id)
        .bind(command.version)
        .bind(command.action)
        .bind(command.actor)
        .bind(command.reason)
        .bind(command.ts_ms)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    async fn record_config_approval(
        &self,
        config: &ConfigVersion,
        version: u32,
        approved_by: &str,
        actor_role: &str,
        reason: Option<&str>,
        ts_ms: i64,
    ) -> StorageResult<()> {
        sqlx::query(
            r#"
            INSERT INTO config_approvals (
                id, config_id, version, target_env, approved_by,
                approved_at, actor_role, reason
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(config_id, version, approved_by) DO UPDATE SET
                target_env = excluded.target_env,
                approved_at = excluded.approved_at,
                actor_role = excluded.actor_role,
                reason = excluded.reason
            "#,
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&config.id)
        .bind(version.to_string())
        .bind(config.target_env.as_deref())
        .bind(approved_by)
        .bind(ts_ms)
        .bind(actor_role)
        .bind(reason)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    async fn count_config_approvals(
        &self,
        config_id: &str,
        version: &str,
        excluding_actor: Option<&str>,
    ) -> StorageResult<u32> {
        let count = if let Some(excluding_actor) = excluding_actor {
            sqlx::query_scalar::<_, i64>(
                r#"
                SELECT COUNT(*)
                FROM config_approvals
                WHERE config_id = ? AND version = ? AND approved_by != ?
                "#,
            )
            .bind(config_id)
            .bind(version)
            .bind(excluding_actor)
            .fetch_one(self.pool())
            .await?
        } else {
            sqlx::query_scalar::<_, i64>(
                r#"
                SELECT COUNT(*)
                FROM config_approvals
                WHERE config_id = ? AND version = ?
                "#,
            )
            .bind(config_id)
            .bind(version)
            .fetch_one(self.pool())
            .await?
        };

        u32::try_from(count)
            .map_err(|error| StorageError::Protocol(format!("invalid approval count: {error}")))
    }

    pub async fn list_config_approvals(
        &self,
        name: &str,
        version: u32,
    ) -> StorageResult<Vec<ConfigApprovalRecord>> {
        let Some(config) = self.get_config(name, version).await? else {
            return Err(StorageError::Protocol(format!(
                "config version {name}:{version} does not exist"
            )));
        };
        let rows = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                Option<String>,
                String,
                i64,
                String,
                Option<String>,
            ),
        >(
            r#"
            SELECT id, config_id, version, target_env, approved_by,
                approved_at, actor_role, reason
            FROM config_approvals
            WHERE config_id = ? AND version = ?
            ORDER BY approved_at ASC, approved_by ASC
            "#,
        )
        .bind(config.id)
        .bind(version.to_string())
        .fetch_all(self.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(
                    id,
                    config_id,
                    version,
                    target_env,
                    approved_by,
                    approved_at_ms,
                    actor_role,
                    reason,
                )| ConfigApprovalRecord {
                    id,
                    config_id,
                    version,
                    target_env,
                    approved_by,
                    approved_at_ms,
                    actor_role,
                    reason,
                },
            )
            .collect())
    }

    pub async fn list_config_audits(
        &self,
        config_id: &str,
    ) -> StorageResult<Vec<StoredConfigAudit>> {
        let rows = sqlx::query_as::<
            _,
            (
                String,
                String,
                Option<String>,
                String,
                Option<String>,
                Option<String>,
                i64,
            ),
        >(
            r#"
            SELECT id, config_id, version, action, actor, reason, ts
            FROM config_audits
            WHERE config_id = ?
            ORDER BY ts DESC, id
            "#,
        )
        .bind(config_id)
        .fetch_all(self.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(id, config_id, version, action, actor, reason, ts_ms)| StoredConfigAudit {
                    id,
                    config_id,
                    version,
                    action,
                    actor,
                    reason,
                    ts_ms,
                },
            )
            .collect())
    }

    pub async fn insert_system_log(&self, log: NewSystemLog) -> StorageResult<()> {
        sqlx::query(
            r#"
            INSERT INTO system_logs (
                id, run_id, ts, level, target, message, fields_json, created_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(log.id)
        .bind(log.run_id)
        .bind(log.ts_ms)
        .bind(log.level)
        .bind(log.target)
        .bind(log.message)
        .bind(log.fields_json)
        .bind(log.created_at_ms)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn insert_system_logs_batch(&self, logs: &[NewSystemLog]) -> StorageResult<()> {
        if logs.is_empty() {
            return Ok(());
        }

        let mut tx = self.pool().begin().await?;
        for log in logs {
            sqlx::query(
                r#"
                INSERT INTO system_logs (
                    id, run_id, ts, level, target, message, fields_json, created_at
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&log.id)
            .bind(&log.run_id)
            .bind(log.ts_ms)
            .bind(&log.level)
            .bind(&log.target)
            .bind(&log.message)
            .bind(&log.fields_json)
            .bind(log.created_at_ms)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn record_system_log(&self, command: SystemLogCommand) -> StorageResult<()> {
        self.insert_system_log(NewSystemLog {
            id: Uuid::new_v4().to_string(),
            run_id: command.run_id,
            ts_ms: command.ts_ms,
            level: command.level,
            target: command.target,
            message: command.message,
            fields_json: command.fields.map(|fields| fields.to_string()),
            created_at_ms: command.ts_ms,
        })
        .await
    }

    pub async fn list_system_logs(
        &self,
        run_id: Option<&str>,
    ) -> StorageResult<Vec<StoredSystemLog>> {
        self.list_system_logs_filtered(SystemLogFilter {
            run_id: run_id.map(str::to_string),
            ..SystemLogFilter::default()
        })
        .await
    }

    pub async fn list_system_logs_filtered(
        &self,
        filter: SystemLogFilter,
    ) -> StorageResult<Vec<StoredSystemLog>> {
        let mut query_builder = QueryBuilder::<Sqlite>::new(
            "SELECT id, run_id, ts, level, target, message, fields_json, created_at \
             FROM system_logs WHERE 1 = 1",
        );
        Self::push_system_log_filters(&mut query_builder, &filter);
        query_builder.push(" ORDER BY ts, id");
        if let Some(limit) = filter.limit {
            query_builder.push(" LIMIT ");
            query_builder.push_bind(limit);
        }
        if let Some(offset) = filter.offset {
            if filter.limit.is_none() {
                query_builder.push(" LIMIT -1");
            }
            query_builder.push(" OFFSET ");
            query_builder.push_bind(offset);
        }

        let rows = query_builder
            .build_query_as::<(
                String,
                Option<String>,
                i64,
                String,
                String,
                String,
                Option<String>,
                i64,
            )>()
            .fetch_all(self.pool())
            .await?;

        Ok(rows
            .into_iter()
            .map(
                |(id, run_id, ts_ms, level, target, message, fields_json, created_at_ms)| {
                    StoredSystemLog {
                        id,
                        run_id,
                        ts_ms,
                        level,
                        target,
                        message,
                        fields_json,
                        created_at_ms,
                    }
                },
            )
            .collect())
    }

    pub async fn count_system_logs(&self, filter: SystemLogFilter) -> StorageResult<u64> {
        let mut query_builder =
            QueryBuilder::<Sqlite>::new("SELECT COUNT(*) as count FROM system_logs WHERE 1 = 1");
        Self::push_system_log_filters(&mut query_builder, &filter);

        let count = query_builder
            .build_query_scalar::<i64>()
            .fetch_one(self.pool())
            .await?;
        Ok(count as u64)
    }

    fn push_system_log_filters(
        query_builder: &mut QueryBuilder<'_, Sqlite>,
        filter: &SystemLogFilter,
    ) {
        if let Some(run_id) = &filter.run_id {
            query_builder.push(" AND run_id = ");
            query_builder.push_bind(run_id.clone());
        }
        if let Some(level) = &filter.level {
            query_builder.push(" AND level = ");
            query_builder.push_bind(level.clone());
        }
        if let Some(target) = &filter.target {
            query_builder.push(" AND target = ");
            query_builder.push_bind(target.clone());
        }
        if let Some(from_ms) = filter.from_ms {
            query_builder.push(" AND ts >= ");
            query_builder.push_bind(from_ms);
        }
        if let Some(to_ms) = filter.to_ms {
            query_builder.push(" AND ts <= ");
            query_builder.push_bind(to_ms);
        }
        if let Some(search) = &filter.search {
            query_builder.push(" AND (message LIKE ");
            query_builder.push_bind(format!("%{search}%"));
            query_builder.push(" OR target LIKE ");
            query_builder.push_bind(format!("%{search}%"));
            query_builder.push(" OR COALESCE(fields_json, '') LIKE ");
            query_builder.push_bind(format!("%{search}%"));
            query_builder.push(")");
        }
    }

    pub async fn purge_system_logs(
        &self,
        command: SystemLogRetentionCommand,
    ) -> StorageResult<u64> {
        let mut query_builder = QueryBuilder::<Sqlite>::new("DELETE FROM system_logs WHERE ts < ");
        query_builder.push_bind(command.before_ms);
        if let Some(target) = command.target {
            query_builder.push(" AND target = ");
            query_builder.push_bind(target);
        }
        if let Some(run_id) = command.run_id {
            query_builder.push(" AND run_id = ");
            query_builder.push_bind(run_id);
        }
        let result = query_builder.build().execute(self.pool()).await?;
        Ok(result.rows_affected())
    }

    pub async fn purge_system_logs_by_retention(
        &self,
        now_ms: i64,
        policy: SystemLogRetentionPolicy,
    ) -> StorageResult<u64> {
        let retention_ms = i64::from(policy.retention_days).saturating_mul(86_400_000);
        self.purge_system_logs(SystemLogRetentionCommand {
            before_ms: now_ms.saturating_sub(retention_ms),
            target: None,
            run_id: None,
        })
        .await
    }

    pub async fn insert_strategy_run(&self, run: NewStrategyRun) -> StorageResult<()> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO strategy_runs (
                id, name, mode, status, started_at_ms, ended_at_ms, error, config_json
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(run.id)
        .bind(run.name)
        .bind(run.mode)
        .bind(run.status)
        .bind(run.started_at_ms)
        .bind(run.ended_at_ms)
        .bind(run.error)
        .bind(run.config_json)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn update_strategy_run_status(
        &self,
        run_id: &str,
        status: &str,
        ended_at_ms: Option<i64>,
        error: Option<&str>,
    ) -> StorageResult<()> {
        sqlx::query(
            r#"
            UPDATE strategy_runs
            SET status = ?, ended_at_ms = ?, error = ?
            WHERE id = ?
            "#,
        )
        .bind(status)
        .bind(ended_at_ms)
        .bind(error)
        .bind(run_id)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn get_strategy_run(&self, run_id: &str) -> StorageResult<Option<StrategyRunRecord>> {
        let row = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                String,
                i64,
                Option<i64>,
                Option<String>,
                String,
            ),
        >(
            r#"
            SELECT id, name, mode, status, started_at_ms, ended_at_ms, error, config_json
            FROM strategy_runs
            WHERE id = ?
            "#,
        )
        .bind(run_id)
        .fetch_optional(self.pool())
        .await?;

        Ok(row.map(
            |(id, name, mode, status, started_at_ms, ended_at_ms, error, config_json)| {
                StrategyRunRecord {
                    id,
                    name,
                    mode,
                    status,
                    started_at_ms,
                    ended_at_ms,
                    error,
                    config_json,
                }
            },
        ))
    }

    pub async fn list_strategy_runs(&self) -> StorageResult<Vec<StrategyRunRecord>> {
        let rows = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                String,
                i64,
                Option<i64>,
                Option<String>,
                String,
            ),
        >(
            r#"
            SELECT id, name, mode, status, started_at_ms, ended_at_ms, error, config_json
            FROM strategy_runs
            ORDER BY started_at_ms DESC, id
            "#,
        )
        .fetch_all(self.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(id, name, mode, status, started_at_ms, ended_at_ms, error, config_json)| {
                    StrategyRunRecord {
                        id,
                        name,
                        mode,
                        status,
                        started_at_ms,
                        ended_at_ms,
                        error,
                        config_json,
                    }
                },
            )
            .collect())
    }

    pub async fn insert_order(&self, order: NewOrder) -> StorageResult<()> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO orders (
                id, run_id, client_order_id, broker_order_id, account_id, symbol, side,
                order_type, price, qty, filled_qty, status, created_at_ms, updated_at_ms
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(order.id)
        .bind(order.run_id)
        .bind(order.client_order_id)
        .bind(order.broker_order_id)
        .bind(order.account_id)
        .bind(order.symbol)
        .bind(order.side)
        .bind(order.order_type)
        .bind(order.price)
        .bind(order.qty)
        .bind(order.filled_qty)
        .bind(order.status)
        .bind(order.created_at_ms)
        .bind(order.updated_at_ms)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn update_order_status_by_broker_id(
        &self,
        run_id: &str,
        broker_order_id: &str,
        status: &str,
        updated_at_ms: i64,
    ) -> StorageResult<u64> {
        let result = sqlx::query(
            r#"
            UPDATE orders
            SET status = ?, updated_at_ms = ?
            WHERE run_id = ? AND broker_order_id = ?
            "#,
        )
        .bind(status)
        .bind(updated_at_ms)
        .bind(run_id)
        .bind(broker_order_id)
        .execute(self.pool())
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn update_order_status_by_client_order_id(
        &self,
        run_id: &str,
        client_order_id: &str,
        broker_order_id: &str,
        status: &str,
        updated_at_ms: i64,
    ) -> StorageResult<u64> {
        let result = sqlx::query(
            r#"
            UPDATE orders
            SET broker_order_id = ?, status = ?, updated_at_ms = ?
            WHERE run_id = ? AND client_order_id = ?
            "#,
        )
        .bind(broker_order_id)
        .bind(status)
        .bind(updated_at_ms)
        .bind(run_id)
        .bind(client_order_id)
        .execute(self.pool())
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn update_order_execution_by_broker_id(
        &self,
        run_id: &str,
        broker_order_id: &str,
        status: &str,
        filled_qty: &str,
        updated_at_ms: i64,
    ) -> StorageResult<()> {
        sqlx::query(
            r#"
            UPDATE orders
            SET status = ?, filled_qty = ?, updated_at_ms = ?
            WHERE run_id = ? AND broker_order_id = ?
            "#,
        )
        .bind(status)
        .bind(filled_qty)
        .bind(updated_at_ms)
        .bind(run_id)
        .bind(broker_order_id)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn update_order_execution_by_client_order_id(
        &self,
        client_order_id: &str,
        broker_order_id: &str,
        status: &str,
        filled_qty: &str,
        updated_at_ms: i64,
    ) -> StorageResult<()> {
        sqlx::query(
            r#"
            UPDATE orders
            SET broker_order_id = ?, status = ?, filled_qty = ?, updated_at_ms = ?
            WHERE client_order_id = ?
            "#,
        )
        .bind(broker_order_id)
        .bind(status)
        .bind(filled_qty)
        .bind(updated_at_ms)
        .bind(client_order_id)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn insert_fill(&self, fill: NewFill) -> StorageResult<()> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO fills (
                id, order_id, run_id, symbol, side, price, qty, fee, ts_ms
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(fill.id)
        .bind(fill.order_id)
        .bind(fill.run_id)
        .bind(fill.symbol)
        .bind(fill.side)
        .bind(fill.price)
        .bind(fill.qty)
        .bind(fill.fee)
        .bind(fill.ts_ms)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn upsert_position(&self, position: NewPosition) -> StorageResult<()> {
        sqlx::query(
            r#"
            INSERT INTO positions (
                run_id, account_id, symbol, qty, avg_price, updated_at_ms
            ) VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT(run_id, account_id, symbol) DO UPDATE SET
                qty = excluded.qty,
                avg_price = excluded.avg_price,
                updated_at_ms = excluded.updated_at_ms
            "#,
        )
        .bind(position.run_id)
        .bind(position.account_id)
        .bind(position.symbol)
        .bind(position.qty)
        .bind(position.avg_price)
        .bind(position.updated_at_ms)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn upsert_account_balance(&self, balance: NewAccountBalance) -> StorageResult<()> {
        sqlx::query(
            r#"
            INSERT INTO account_balances (
                run_id, account_id, asset, total, available, frozen, updated_at_ms
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(run_id, account_id, asset) DO UPDATE SET
                total = excluded.total,
                available = excluded.available,
                frozen = excluded.frozen,
                updated_at_ms = excluded.updated_at_ms
            "#,
        )
        .bind(balance.run_id)
        .bind(balance.account_id)
        .bind(balance.asset)
        .bind(balance.total)
        .bind(balance.available)
        .bind(balance.frozen)
        .bind(balance.updated_at_ms)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn insert_portfolio_snapshot(
        &self,
        snapshot: NewPortfolioSnapshot,
    ) -> StorageResult<()> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO portfolio_snapshots (
                id, run_id, account_id, ts_ms, cash, market_value, equity,
                realized_pnl, unrealized_pnl
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(snapshot.id)
        .bind(snapshot.run_id)
        .bind(snapshot.account_id)
        .bind(snapshot.ts_ms)
        .bind(snapshot.cash)
        .bind(snapshot.market_value)
        .bind(snapshot.equity)
        .bind(snapshot.realized_pnl)
        .bind(snapshot.unrealized_pnl)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn insert_event(&self, event: NewEventRecord) -> StorageResult<()> {
        let event_id = event.event_id;
        let ts_ms = event.ts_ms;
        let source = event.source;
        let category = event.category;
        let payload_json = event.payload_json;

        sqlx::query(
            r#"
            INSERT OR REPLACE INTO event_store (
                event_id, ts_ms, source, category, payload_json
            ) VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(&event_id)
        .bind(ts_ms)
        .bind(&source)
        .bind(&category)
        .bind(&payload_json)
        .execute(self.pool())
        .await?;

        if let Some(order_event) =
            order_event_projection(&event_id, ts_ms, &source, &category, &payload_json)
        {
            self.insert_order_event(order_event).await?;
        }
        if let Some(risk_event) =
            risk_event_projection(&event_id, ts_ms, &source, &category, &payload_json)
        {
            self.insert_risk_event(risk_event).await?;
        }
        if let Some(insight) =
            insight_projection(&event_id, ts_ms, &source, &category, &payload_json)
        {
            self.insert_insight(insight).await?;
        }
        if let Some(target) =
            portfolio_target_projection(&event_id, ts_ms, &source, &category, &payload_json)
        {
            self.insert_portfolio_target(target).await?;
        }
        Ok(())
    }

    async fn record_market_rule_audit(
        &self,
        rule_type: &str,
        rule_id: &str,
        action: &str,
        ts_ms: i64,
        rule: serde_json::Value,
    ) -> StorageResult<()> {
        let payload_json = serde_json::json!({
            "action": action,
            "rule_type": rule_type,
            "rule_id": rule_id,
            "rule": rule,
        })
        .to_string();

        self.insert_event(NewEventRecord {
            event_id: Uuid::new_v4().to_string(),
            ts_ms,
            source: rule_id.to_string(),
            category: format!("market_rule.{rule_type}.changed"),
            payload_json,
        })
        .await
    }

    pub async fn insert_order_event(&self, event: NewOrderEvent) -> StorageResult<()> {
        sqlx::query(
            r#"
            INSERT INTO order_events (
                id, event_id, run_id, order_id, client_order_id, broker_order_id,
                account_id, symbol, status, event_type, message, ts_ms, payload_json
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(event.id)
        .bind(event.event_id)
        .bind(event.run_id)
        .bind(event.order_id)
        .bind(event.client_order_id)
        .bind(event.broker_order_id)
        .bind(event.account_id)
        .bind(event.symbol)
        .bind(event.status)
        .bind(event.event_type)
        .bind(event.message)
        .bind(event.ts_ms)
        .bind(event.payload_json)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn insert_risk_event(&self, event: NewRiskEvent) -> StorageResult<()> {
        sqlx::query(
            r#"
            INSERT INTO risk_events (
                id, event_id, run_id, account_id, symbol, risk_type, decision,
                reason, threshold, observed_value, ts_ms, payload_json
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(event.id)
        .bind(event.event_id)
        .bind(event.run_id)
        .bind(event.account_id)
        .bind(event.symbol)
        .bind(event.risk_type)
        .bind(event.decision)
        .bind(event.reason)
        .bind(event.threshold)
        .bind(event.observed_value)
        .bind(event.ts_ms)
        .bind(event.payload_json)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn insert_insight(&self, insight: NewInsight) -> StorageResult<()> {
        sqlx::query(
            r#"
            INSERT INTO insights (
                id, event_id, run_id, strategy, symbol, side, confidence, ts_ms, payload_json
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(insight.id)
        .bind(insight.event_id)
        .bind(insight.run_id)
        .bind(insight.strategy)
        .bind(insight.symbol)
        .bind(insight.side)
        .bind(insight.confidence)
        .bind(insight.ts_ms)
        .bind(insight.payload_json)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn insert_portfolio_target(&self, target: NewPortfolioTarget) -> StorageResult<()> {
        sqlx::query(
            r#"
            INSERT INTO portfolio_targets (
                id, event_id, run_id, account_id, symbol, target_qty, ts_ms, payload_json
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(target.id)
        .bind(target.event_id)
        .bind(target.run_id)
        .bind(target.account_id)
        .bind(target.symbol)
        .bind(target.target_qty)
        .bind(target.ts_ms)
        .bind(target.payload_json)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn record_runtime_event(&self, command: RuntimeEventCommand) -> StorageResult<()> {
        self.insert_event(NewEventRecord {
            event_id: Uuid::new_v4().to_string(),
            ts_ms: command.ts_ms,
            source: command.source,
            category: command.category,
            payload_json: command.payload.to_string(),
        })
        .await
    }

    pub async fn start_strategy_run(&self, command: StrategyRunStartCommand) -> StorageResult<()> {
        self.insert_strategy_run(NewStrategyRun {
            id: command.run_id,
            name: command.name,
            mode: command.mode,
            status: "running".to_string(),
            started_at_ms: command.started_at_ms,
            ended_at_ms: None,
            error: None,
            config_json: command.config.to_string(),
        })
        .await
    }

    pub async fn start_live_run(&self, command: LiveRunCommand) -> StorageResult<()> {
        self.insert_strategy_run(NewStrategyRun {
            id: command.run_id,
            name: "live".to_string(),
            mode: "live".to_string(),
            status: "running".to_string(),
            started_at_ms: command.started_at_ms,
            ended_at_ms: None,
            error: None,
            config_json: command.config.to_string(),
        })
        .await
    }

    pub async fn record_external_order(&self, command: ExternalOrderCommand) -> StorageResult<()> {
        self.insert_order(NewOrder {
            id: command.order_id,
            run_id: command.run_id,
            client_order_id: command.client_order_id,
            broker_order_id: command.broker_order_id,
            account_id: command.account_id,
            symbol: command.symbol,
            side: command.side,
            order_type: command.order_type,
            price: command.price.map(|price| price.to_string()),
            qty: command.qty.to_string(),
            filled_qty: command.filled_qty.to_string(),
            status: command.status,
            created_at_ms: command.ts_ms,
            updated_at_ms: command.ts_ms,
        })
        .await
    }

    pub async fn record_external_fill(&self, command: ExternalFillCommand) -> StorageResult<()> {
        self.insert_fill(NewFill {
            id: command.id,
            order_id: command.order_id,
            run_id: command.run_id,
            symbol: command.symbol,
            side: command.side,
            price: command.price.to_string(),
            qty: command.qty.to_string(),
            fee: command.fee.to_string(),
            ts_ms: command.ts_ms,
        })
        .await
    }

    pub async fn record_account_balance(
        &self,
        command: AccountBalanceCommand,
    ) -> StorageResult<()> {
        self.upsert_account_balance(NewAccountBalance {
            run_id: command.run_id,
            account_id: command.account_id,
            asset: command.asset,
            total: command.total.to_string(),
            available: command.available.to_string(),
            frozen: command.frozen.to_string(),
            updated_at_ms: command.updated_at_ms,
        })
        .await
    }

    pub async fn record_position(&self, command: PositionCommand) -> StorageResult<()> {
        self.upsert_position(NewPosition {
            run_id: command.run_id,
            account_id: command.account_id,
            symbol: command.symbol,
            qty: command.qty.to_string(),
            avg_price: command.avg_price.to_string(),
            updated_at_ms: command.updated_at_ms,
        })
        .await
    }

    pub async fn record_crypto_position(
        &self,
        command: CryptoPositionCommand,
    ) -> StorageResult<()> {
        self.upsert_crypto_position(NewCryptoPosition {
            run_id: command.run_id,
            account_id: command.account_id,
            exchange: command.exchange,
            symbol: command.symbol,
            asset_class: command.asset_class,
            margin_mode: command.margin_mode,
            position_side: command.position_side,
            leverage: command.leverage.to_string(),
            qty: command.qty.to_string(),
            avg_price: command.avg_price.to_string(),
            margin_used: command.margin_used.to_string(),
            funding_fee: command.funding_fee.to_string(),
            realized_pnl: command.realized_pnl.to_string(),
            unrealized_pnl: command.unrealized_pnl.to_string(),
            updated_at_ms: command.updated_at_ms,
        })
        .await
    }

    pub async fn record_funding_rate(&self, command: FundingRateCommand) -> StorageResult<()> {
        self.upsert_funding_rate(NewFundingRate {
            id: command.id,
            exchange: command.exchange,
            symbol: command.symbol,
            funding_time_ms: command.funding_time_ms,
            funding_rate: command.funding_rate.to_string(),
            mark_price: command.mark_price.map(|price| price.to_string()),
            source: command.source,
        })
        .await
    }

    pub async fn record_crypto_market_meta(
        &self,
        command: CryptoMarketMetaCommand,
    ) -> StorageResult<()> {
        let margin_modes = command
            .margin_modes
            .map(|modes| serde_json::to_string(&modes))
            .transpose()
            .map_err(|error| {
                StorageError::Protocol(format!("failed to encode margin modes: {error}"))
            })?;

        self.upsert_crypto_market_meta(NewCryptoMarketMeta {
            exchange: command.exchange,
            symbol: command.symbol,
            base_asset: command.base_asset,
            quote_asset: command.quote_asset,
            instrument_type: command.instrument_type,
            contract_type: command.contract_type,
            contract_size: command.contract_size.map(|value| value.to_string()),
            settlement_asset: command.settlement_asset,
            min_notional: command.min_notional.map(|value| value.to_string()),
            min_qty: command.min_qty.map(|value| value.to_string()),
            max_qty: command.max_qty.map(|value| value.to_string()),
            price_precision: command.price_precision,
            qty_precision: command.qty_precision,
            price_tick: command.price_tick.map(|value| value.to_string()),
            qty_step: command.qty_step.map(|value| value.to_string()),
            maker_fee_rate: command.maker_fee_rate.map(|value| value.to_string()),
            taker_fee_rate: command.taker_fee_rate.map(|value| value.to_string()),
            funding_interval_hours: command.funding_interval_hours,
            max_leverage: command.max_leverage.map(|value| value.to_string()),
            margin_modes,
            is_inverse: command.is_inverse,
            is_active: command.is_active,
            created_at_ms: command.created_at_ms,
            updated_at_ms: command.updated_at_ms,
        })
        .await
    }

    pub async fn record_corporate_action_meta(
        &self,
        command: CorporateActionMetaCommand,
    ) -> StorageResult<()> {
        self.upsert_corporate_action_meta(NewCorporateActionMeta {
            market: command.market,
            exchange: command.exchange,
            symbol: command.symbol,
            action_type: command.action_type,
            ex_date_ms: command.ex_date_ms,
            record_date_ms: command.record_date_ms,
            payable_date_ms: command.payable_date_ms,
            ratio: command.ratio,
            cash_amount: command.cash_amount.map(|value| value.to_string()),
            currency: command.currency,
            source: command.source,
            created_at_ms: command.created_at_ms,
            updated_at_ms: command.updated_at_ms,
        })
        .await
    }

    pub async fn record_portfolio_snapshot(
        &self,
        command: PortfolioSnapshotCommand,
    ) -> StorageResult<()> {
        self.insert_portfolio_snapshot(NewPortfolioSnapshot {
            id: command.id,
            run_id: command.run_id,
            account_id: command.account_id,
            ts_ms: command.ts_ms,
            cash: command.cash.to_string(),
            market_value: command.market_value.to_string(),
            equity: command.equity.to_string(),
            realized_pnl: command.realized_pnl.to_string(),
            unrealized_pnl: command.unrealized_pnl.to_string(),
        })
        .await
    }

    pub async fn record_paper_order_submitted(
        &self,
        command: PaperOrderCommand,
    ) -> StorageResult<()> {
        self.insert_order(paper_order_record(PaperOrderRecordInput {
            run_id: &command.run_id,
            order_id: &command.order_id,
            client_order_id: &command.client_order_id,
            broker_order_id: None,
            order: &command.order,
            filled_qty: Decimal::ZERO,
            status: "SUBMITTED",
            ts_ms: command.ts_ms,
        }))
        .await?;
        self.insert_event(NewEventRecord {
            event_id: Uuid::new_v4().to_string(),
            ts_ms: command.ts_ms,
            source: command.run_id.clone(),
            category: "broker.order.submitted".to_string(),
            payload_json: paper_order_event_payload(
                &command.run_id,
                &command.order_id,
                &command.client_order_id,
                None,
                &command.order,
                Decimal::ZERO,
                "SUBMITTED",
                None,
            )
            .to_string(),
        })
        .await
    }

    pub async fn record_paper_order_failed(
        &self,
        command: PaperFailedOrderCommand,
    ) -> StorageResult<()> {
        self.insert_order(paper_order_record(PaperOrderRecordInput {
            run_id: &command.run_id,
            order_id: &command.order_id,
            client_order_id: &command.client_order_id,
            broker_order_id: None,
            order: &command.order,
            filled_qty: Decimal::ZERO,
            status: "FAILED",
            ts_ms: command.ts_ms,
        }))
        .await?;
        self.insert_event(NewEventRecord {
            event_id: Uuid::new_v4().to_string(),
            ts_ms: command.ts_ms,
            source: command.run_id.clone(),
            category: "broker.order.failed".to_string(),
            payload_json: paper_order_event_payload(
                &command.run_id,
                &command.order_id,
                &command.client_order_id,
                None,
                &command.order,
                Decimal::ZERO,
                "FAILED",
                Some(&command.error),
            )
            .to_string(),
        })
        .await
    }

    pub async fn record_paper_execution_result(
        &self,
        command: PaperExecutionCommand,
    ) -> StorageResult<()> {
        self.insert_order(paper_order_record(PaperOrderRecordInput {
            run_id: &command.run_id,
            order_id: &command.order_id,
            client_order_id: &command.client_order_id,
            broker_order_id: Some(command.broker_order_id.clone()),
            order: &command.order,
            filled_qty: command.qty,
            status: &command.status,
            ts_ms: command.ts_ms,
        }))
        .await?;

        if command.qty > Decimal::ZERO {
            let symbol = command.order.symbol.clone();
            let side = order_side(&command.order);
            self.insert_fill(NewFill {
                id: command.fill_id,
                order_id: command.order_id,
                run_id: command.run_id,
                symbol,
                side,
                price: command.price.to_string(),
                qty: command.qty.to_string(),
                fee: command.fee.to_string(),
                ts_ms: command.ts_ms,
            })
            .await?;
        }
        Ok(())
    }

    pub async fn record_paper_portfolio_snapshot(
        &self,
        command: PaperPortfolioSnapshotCommand,
    ) -> StorageResult<()> {
        self.insert_portfolio_snapshot(NewPortfolioSnapshot {
            id: format!("{}-snapshot-{}", command.run_id, command.ts_ms),
            run_id: command.run_id.clone(),
            account_id: command.account_id.clone(),
            ts_ms: command.ts_ms,
            cash: command.cash.to_string(),
            market_value: command.market_value.to_string(),
            equity: command.equity.to_string(),
            realized_pnl: command.realized_pnl.to_string(),
            unrealized_pnl: command.unrealized_pnl.to_string(),
        })
        .await?;

        self.insert_cash_snapshot(NewCashSnapshot {
            run_id: command.run_id.clone(),
            ts_ms: command.ts_ms,
            currency: command.base_currency.clone(),
            cash: command.cash.to_string(),
            available_cash: command.cash.to_string(),
            frozen_cash: Decimal::ZERO.to_string(),
            created_at_ms: command.ts_ms,
        })
        .await?;

        for position in command.positions {
            self.insert_position_snapshot(position_snapshot_from_command(
                position,
                &command.base_currency,
            )?)
            .await?;
        }
        Ok(())
    }

    pub async fn record_broker_position_snapshot(
        &self,
        command: BrokerPositionSnapshotCommand,
    ) -> StorageResult<()> {
        self.insert_position_snapshot(broker_position_snapshot_from_command(command)?)
            .await
    }

    pub async fn record_runtime_position_snapshot(
        &self,
        command: RuntimePositionSnapshotCommand,
    ) -> StorageResult<()> {
        self.insert_position_snapshot(runtime_position_snapshot_from_command(command)?)
            .await
    }

    pub async fn complete_paper_run(&self, command: PaperFinalStateCommand) -> StorageResult<()> {
        self.insert_strategy_run(NewStrategyRun {
            id: command.run_id.clone(),
            name: command.strategy_name,
            mode: "paper".to_string(),
            status: "completed".to_string(),
            started_at_ms: command.started_at_ms,
            ended_at_ms: Some(command.ended_at_ms),
            error: None,
            config_json: command.config_json,
        })
        .await?;

        self.upsert_account_balance(NewAccountBalance {
            run_id: command.run_id.clone(),
            account_id: command.account_id.clone(),
            asset: command.base_currency.clone(),
            total: command.cash.to_string(),
            available: command.cash.to_string(),
            frozen: Decimal::ZERO.to_string(),
            updated_at_ms: command.ended_at_ms,
        })
        .await?;

        self.upsert_position(NewPosition {
            run_id: command.run_id.clone(),
            account_id: command.account_id.clone(),
            symbol: command.symbol.clone(),
            qty: command.position_qty.to_string(),
            avg_price: command.position_avg_price.to_string(),
            updated_at_ms: command.ended_at_ms,
        })
        .await?;

        for position in &command.positions {
            self.upsert_position(NewPosition {
                run_id: position.run_id.clone(),
                account_id: position.account_id.clone(),
                symbol: position.symbol.clone(),
                qty: position.qty.to_string(),
                avg_price: position.avg_price.to_string(),
                updated_at_ms: position.updated_at_ms,
            })
            .await?;
        }

        self.insert_portfolio_snapshot(NewPortfolioSnapshot {
            id: format!("{}-snapshot-final", command.run_id),
            run_id: command.run_id.clone(),
            account_id: command.account_id.clone(),
            ts_ms: command.ended_at_ms,
            cash: command.cash.to_string(),
            market_value: command.market_value.to_string(),
            equity: command.equity.to_string(),
            realized_pnl: command.realized_pnl.to_string(),
            unrealized_pnl: command.unrealized_pnl.to_string(),
        })
        .await?;

        self.insert_cash_snapshot(NewCashSnapshot {
            run_id: command.run_id.clone(),
            ts_ms: command.ended_at_ms,
            currency: command.base_currency.clone(),
            cash: command.cash.to_string(),
            available_cash: command.cash.to_string(),
            frozen_cash: Decimal::ZERO.to_string(),
            created_at_ms: command.ended_at_ms,
        })
        .await?;

        for position in command.positions {
            self.insert_position_snapshot(position_snapshot_from_command(
                position,
                &command.base_currency,
            )?)
            .await?;
        }
        Ok(())
    }

    pub async fn insert_runtime_events(
        &self,
        source: &str,
        events: &[StoredRuntimeEvent],
    ) -> StorageResult<()> {
        for event in events {
            self.insert_event(NewEventRecord {
                event_id: Uuid::new_v4().to_string(),
                ts_ms: event.ts_ms,
                source: source.to_string(),
                category: event.category.clone(),
                payload_json: event.payload_json.clone(),
            })
            .await?;
        }
        Ok(())
    }

    pub async fn insert_filled_backtest_execution(
        &self,
        execution: BacktestExecutionRecord,
    ) -> StorageResult<()> {
        self.insert_order(NewOrder {
            id: execution.order_id.clone(),
            run_id: execution.run_id.clone(),
            client_order_id: execution.order_id.clone(),
            broker_order_id: Some(execution.broker_order_id),
            account_id: execution.account_id,
            symbol: execution.symbol.clone(),
            side: execution.side.clone(),
            order_type: execution.order_type,
            price: execution.price,
            qty: execution.qty.clone(),
            filled_qty: execution.qty.clone(),
            status: "FILLED".to_string(),
            created_at_ms: execution.ts_ms,
            updated_at_ms: execution.ts_ms,
        })
        .await?;

        self.insert_fill(NewFill {
            id: execution.fill_id,
            order_id: execution.order_id,
            run_id: execution.run_id,
            symbol: execution.symbol,
            side: execution.side,
            price: execution.fill_price,
            qty: execution.qty,
            fee: execution.fee,
            ts_ms: execution.ts_ms,
        })
        .await
    }

    pub async fn record_backtest_filled_execution(
        &self,
        command: BacktestFilledExecutionCommand,
    ) -> StorageResult<()> {
        self.insert_order(paper_order_record(PaperOrderRecordInput {
            run_id: &command.run_id,
            order_id: &command.order_id,
            client_order_id: &command.order_id,
            broker_order_id: Some(command.broker_order_id),
            order: &command.order,
            filled_qty: command.order.qty,
            status: "FILLED",
            ts_ms: command.ts_ms,
        }))
        .await?;

        self.insert_fill(NewFill {
            id: command.fill_id,
            order_id: command.order_id,
            run_id: command.run_id,
            symbol: command.order.symbol.clone(),
            side: order_side(&command.order),
            price: command.fill_price.to_string(),
            qty: command.order.qty.to_string(),
            fee: command.fee.to_string(),
            ts_ms: command.ts_ms,
        })
        .await
    }

    pub async fn upsert_backtest_position(
        &self,
        position: BacktestPositionRecord,
    ) -> StorageResult<()> {
        self.upsert_position(NewPosition {
            run_id: position.run_id,
            account_id: position.account_id,
            symbol: position.symbol,
            qty: position.qty,
            avg_price: position.avg_price,
            updated_at_ms: position.updated_at_ms,
        })
        .await
    }

    pub async fn record_backtest_position(
        &self,
        command: BacktestPositionCommand,
    ) -> StorageResult<()> {
        self.upsert_position(NewPosition {
            run_id: command.run_id,
            account_id: command.account_id,
            symbol: command.symbol,
            qty: command.qty.to_string(),
            avg_price: command.avg_price.to_string(),
            updated_at_ms: command.updated_at_ms,
        })
        .await
    }

    pub async fn complete_backtest_run(&self, run: BacktestCompletedRun) -> StorageResult<()> {
        self.insert_strategy_run(NewStrategyRun {
            id: run.run_id,
            name: run.strategy_name,
            mode: "backtest".to_string(),
            status: "completed".to_string(),
            started_at_ms: run.started_at_ms,
            ended_at_ms: Some(run.ended_at_ms),
            error: None,
            config_json: run.config_json,
        })
        .await
    }

    pub async fn list_orders(&self, run_id: &str) -> StorageResult<Vec<StoredOrder>> {
        let rows = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                Option<String>,
                String,
                String,
                String,
                String,
                Option<String>,
                String,
                String,
                String,
                i64,
                i64,
            ),
        >(
            r#"
            SELECT id, run_id, client_order_id, broker_order_id, account_id, symbol, side,
                   order_type, price, qty, filled_qty, status, created_at_ms, updated_at_ms
            FROM orders
            WHERE run_id = ?
            ORDER BY created_at_ms, id
            "#,
        )
        .bind(run_id)
        .fetch_all(self.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(
                    id,
                    run_id,
                    client_order_id,
                    broker_order_id,
                    account_id,
                    symbol,
                    side,
                    order_type,
                    price,
                    qty,
                    filled_qty,
                    status,
                    created_at_ms,
                    updated_at_ms,
                )| StoredOrder {
                    id,
                    run_id,
                    client_order_id,
                    broker_order_id,
                    account_id,
                    symbol,
                    side,
                    order_type,
                    price,
                    qty,
                    filled_qty,
                    status,
                    created_at_ms,
                    updated_at_ms,
                },
            )
            .collect())
    }

    pub async fn get_order_by_client_order_id(
        &self,
        client_order_id: &str,
    ) -> StorageResult<Option<StoredOrder>> {
        let row = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                Option<String>,
                String,
                String,
                String,
                String,
                Option<String>,
                String,
                String,
                String,
                i64,
                i64,
            ),
        >(
            r#"
            SELECT id, run_id, client_order_id, broker_order_id, account_id, symbol, side,
                   order_type, price, qty, filled_qty, status, created_at_ms, updated_at_ms
            FROM orders
            WHERE client_order_id = ?
            "#,
        )
        .bind(client_order_id)
        .fetch_optional(self.pool())
        .await?;

        Ok(row.map(
            |(
                id,
                run_id,
                client_order_id,
                broker_order_id,
                account_id,
                symbol,
                side,
                order_type,
                price,
                qty,
                filled_qty,
                status,
                created_at_ms,
                updated_at_ms,
            )| StoredOrder {
                id,
                run_id,
                client_order_id,
                broker_order_id,
                account_id,
                symbol,
                side,
                order_type,
                price,
                qty,
                filled_qty,
                status,
                created_at_ms,
                updated_at_ms,
            },
        ))
    }

    pub async fn list_recoverable_orders(&self, run_id: &str) -> StorageResult<Vec<StoredOrder>> {
        let rows = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                Option<String>,
                String,
                String,
                String,
                String,
                Option<String>,
                String,
                String,
                String,
                i64,
                i64,
            ),
        >(
            r#"
            SELECT id, run_id, client_order_id, broker_order_id, account_id, symbol, side,
                   order_type, price, qty, filled_qty, status, created_at_ms, updated_at_ms
            FROM orders
            WHERE run_id = ? AND status IN ('SUBMITTED', 'NEW', 'PARTIALLY_FILLED')
            ORDER BY created_at_ms, id
            "#,
        )
        .bind(run_id)
        .fetch_all(self.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(
                    id,
                    run_id,
                    client_order_id,
                    broker_order_id,
                    account_id,
                    symbol,
                    side,
                    order_type,
                    price,
                    qty,
                    filled_qty,
                    status,
                    created_at_ms,
                    updated_at_ms,
                )| StoredOrder {
                    id,
                    run_id,
                    client_order_id,
                    broker_order_id,
                    account_id,
                    symbol,
                    side,
                    order_type,
                    price,
                    qty,
                    filled_qty,
                    status,
                    created_at_ms,
                    updated_at_ms,
                },
            )
            .collect())
    }

    pub async fn recover_order_state(
        &self,
        run_id: &str,
        order_id: &str,
    ) -> StorageResult<Option<RecoveredOrderState>> {
        let row = sqlx::query_as::<_, (String, String, String, String, String, i64)>(
            r#"
            SELECT id, run_id, qty, filled_qty, status, updated_at_ms
            FROM orders
            WHERE run_id = ? AND id = ?
            "#,
        )
        .bind(run_id)
        .bind(order_id)
        .fetch_optional(self.pool())
        .await?;

        Ok(row.map(
            |(id, run_id, order_qty, filled_qty, status, updated_at_ms)| RecoveredOrderState {
                id,
                run_id,
                order_qty,
                filled_qty,
                status,
                updated_at_ms,
            },
        ))
    }

    pub async fn list_fills(&self, run_id: &str) -> StorageResult<Vec<StoredFill>> {
        let rows = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                String,
                String,
                String,
                String,
                String,
                i64,
            ),
        >(
            r#"
            SELECT id, order_id, run_id, symbol, side, price, qty, fee, ts_ms
            FROM fills
            WHERE run_id = ?
            ORDER BY ts_ms, id
            "#,
        )
        .bind(run_id)
        .fetch_all(self.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(id, order_id, run_id, symbol, side, price, qty, fee, ts_ms)| StoredFill {
                    id,
                    order_id,
                    run_id,
                    symbol,
                    side,
                    price,
                    qty,
                    fee,
                    ts_ms,
                },
            )
            .collect())
    }

    pub async fn list_fee_volume_entries(
        &self,
        query: FeeVolumeQuery,
    ) -> StorageResult<Vec<FeeVolumeEntry>> {
        if query.to_ms <= query.from_ms {
            return Ok(Vec::new());
        }

        let mut query_builder = QueryBuilder::<Sqlite>::new(
            r#"
            SELECT f.symbol, f.price, f.qty, f.ts_ms
            FROM fills f
            INNER JOIN orders o ON o.id = f.order_id
            WHERE o.account_id =
            "#,
        );
        query_builder.push_bind(query.account_id);
        query_builder
            .push(" AND f.ts_ms >= ")
            .push_bind(query.from_ms)
            .push(" AND f.ts_ms < ")
            .push_bind(query.to_ms);

        if let Some(symbol) = query.symbol.as_deref() {
            query_builder.push(" AND f.symbol = ").push_bind(symbol);
        }

        query_builder.push(" ORDER BY f.ts_ms, f.id");

        let rows = query_builder
            .build_query_as::<(String, String, String, i64)>()
            .fetch_all(self.pool())
            .await?;

        rows.into_iter()
            .filter(|(symbol, _, _, _)| {
                query.symbol.is_some()
                    || fee_volume_symbol_matches_scope(
                        symbol,
                        &query.market,
                        &query.exchange,
                        &query.asset_class,
                    )
            })
            .map(|(_, price, qty, ts_ms)| {
                let price = parse_rule_decimal("fill price", &price)?;
                let qty = parse_rule_decimal("fill qty", &qty)?;
                Ok(FeeVolumeEntry {
                    ts_ms,
                    notional: price * qty,
                })
            })
            .collect()
    }

    pub async fn sum_fee_volume_notional(&self, query: FeeVolumeQuery) -> StorageResult<Decimal> {
        let entries = self.list_fee_volume_entries(query).await?;
        Ok(entries
            .into_iter()
            .fold(Decimal::ZERO, |total, entry| total + entry.notional))
    }

    pub async fn list_positions(&self, run_id: &str) -> StorageResult<Vec<StoredPosition>> {
        let rows = sqlx::query_as::<_, (String, String, String, String, String, i64)>(
            r#"
            SELECT run_id, account_id, symbol, qty, avg_price, updated_at_ms
            FROM positions
            WHERE run_id = ?
            ORDER BY account_id, symbol
            "#,
        )
        .bind(run_id)
        .fetch_all(self.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(run_id, account_id, symbol, qty, avg_price, updated_at_ms)| StoredPosition {
                    run_id,
                    account_id,
                    symbol,
                    qty,
                    avg_price,
                    updated_at_ms,
                },
            )
            .collect())
    }

    pub async fn list_account_balances(
        &self,
        run_id: &str,
    ) -> StorageResult<Vec<StoredAccountBalance>> {
        let rows = sqlx::query_as::<_, (String, String, String, String, String, String, i64)>(
            r#"
            SELECT run_id, account_id, asset, total, available, frozen, updated_at_ms
            FROM account_balances
            WHERE run_id = ?
            ORDER BY account_id, asset
            "#,
        )
        .bind(run_id)
        .fetch_all(self.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(run_id, account_id, asset, total, available, frozen, updated_at_ms)| {
                    StoredAccountBalance {
                        run_id,
                        account_id,
                        asset,
                        total,
                        available,
                        frozen,
                        updated_at_ms,
                    }
                },
            )
            .collect())
    }

    pub async fn list_portfolio_snapshots(
        &self,
        run_id: &str,
    ) -> StorageResult<Vec<StoredPortfolioSnapshot>> {
        let rows = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                i64,
                String,
                String,
                String,
                String,
                String,
            ),
        >(
            r#"
            SELECT id, run_id, account_id, ts_ms, cash, market_value, equity,
                   realized_pnl, unrealized_pnl
            FROM portfolio_snapshots
            WHERE run_id = ?
            ORDER BY ts_ms, id
            "#,
        )
        .bind(run_id)
        .fetch_all(self.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(
                    id,
                    run_id,
                    account_id,
                    ts_ms,
                    cash,
                    market_value,
                    equity,
                    realized_pnl,
                    unrealized_pnl,
                )| StoredPortfolioSnapshot {
                    id,
                    run_id,
                    account_id,
                    ts_ms,
                    cash,
                    market_value,
                    equity,
                    realized_pnl,
                    unrealized_pnl,
                },
            )
            .collect())
    }

    pub async fn list_events(&self) -> StorageResult<Vec<EventRecord>> {
        let rows = sqlx::query_as::<_, (String, i64, String, String, String)>(
            r#"
            SELECT event_id, ts_ms, source, category, payload_json
            FROM event_store
            ORDER BY ts_ms, event_id
            "#,
        )
        .fetch_all(self.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(event_id, ts_ms, source, category, payload_json)| EventRecord {
                    event_id,
                    ts_ms,
                    source,
                    category,
                    payload_json,
                },
            )
            .collect())
    }

    pub async fn list_events_by_source(&self, source: &str) -> StorageResult<Vec<EventRecord>> {
        let rows = sqlx::query_as::<_, (String, i64, String, String, String)>(
            r#"
            SELECT event_id, ts_ms, source, category, payload_json
            FROM event_store
            WHERE source = ?
            ORDER BY ts_ms, event_id
            "#,
        )
        .bind(source)
        .fetch_all(self.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(event_id, ts_ms, source, category, payload_json)| EventRecord {
                    event_id,
                    ts_ms,
                    source,
                    category,
                    payload_json,
                },
            )
            .collect())
    }

    pub async fn list_market_rule_audit_events(
        &self,
        filter: MarketRuleAuditFilter,
    ) -> StorageResult<Vec<EventRecord>> {
        let mut query = QueryBuilder::<Sqlite>::new(
            "SELECT event_id, ts_ms, source, category, payload_json FROM event_store WHERE category LIKE 'market_rule.%'",
        );

        if let Some(rule_type) = filter.rule_type.as_deref() {
            query.push(" AND category = ");
            query.push_bind(format!("market_rule.{rule_type}.changed"));
        }
        if let Some(rule_id) = filter.rule_id.as_deref() {
            query.push(" AND source = ");
            query.push_bind(rule_id);
        }
        if let Some(from_ms) = filter.from_ms {
            query.push(" AND ts_ms >= ");
            query.push_bind(from_ms);
        }
        if let Some(to_ms) = filter.to_ms {
            query.push(" AND ts_ms <= ");
            query.push_bind(to_ms);
        }

        query.push(" ORDER BY ts_ms, event_id");
        if let Some(limit) = filter.limit {
            query.push(" LIMIT ");
            query.push_bind(limit);
        }

        let rows = query
            .build_query_as::<(String, i64, String, String, String)>()
            .fetch_all(self.pool())
            .await?;

        Ok(rows
            .into_iter()
            .map(
                |(event_id, ts_ms, source, category, payload_json)| EventRecord {
                    event_id,
                    ts_ms,
                    source,
                    category,
                    payload_json,
                },
            )
            .collect())
    }

    pub async fn list_order_events(&self, run_id: &str) -> StorageResult<Vec<StoredOrderEvent>> {
        self.list_order_events_filtered(OrderEventFilter {
            run_id: Some(run_id.to_string()),
            ..OrderEventFilter::default()
        })
        .await
    }

    pub async fn list_order_events_filtered(
        &self,
        filter: OrderEventFilter,
    ) -> StorageResult<Vec<StoredOrderEvent>> {
        type OrderEventRow = (
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            String,
            String,
            Option<String>,
            i64,
            String,
        );

        let mut query_builder = QueryBuilder::<Sqlite>::new(
            r#"
            SELECT id, event_id, run_id, order_id, client_order_id, broker_order_id,
                   account_id, symbol, status, event_type, message, ts_ms, payload_json
            FROM order_events
            WHERE 1 = 1
            "#,
        );

        if let Some(run_id) = filter.run_id.as_deref() {
            query_builder.push(" AND run_id = ").push_bind(run_id);
        }
        if let Some(order_id) = filter.order_id.as_deref() {
            query_builder.push(" AND order_id = ").push_bind(order_id);
        }
        if let Some(client_order_id) = filter.client_order_id.as_deref() {
            query_builder
                .push(" AND client_order_id = ")
                .push_bind(client_order_id);
        }
        if let Some(broker_order_id) = filter.broker_order_id.as_deref() {
            query_builder
                .push(" AND broker_order_id = ")
                .push_bind(broker_order_id);
        }
        if let Some(account_id) = filter.account_id.as_deref() {
            query_builder
                .push(" AND account_id = ")
                .push_bind(account_id);
        }
        if let Some(symbol) = filter.symbol.as_deref() {
            query_builder.push(" AND symbol = ").push_bind(symbol);
        }
        if let Some(status) = filter.status.as_deref() {
            query_builder.push(" AND status = ").push_bind(status);
        }
        if let Some(event_type) = filter.event_type.as_deref() {
            query_builder
                .push(" AND event_type = ")
                .push_bind(event_type);
        }
        if let Some(from_ms) = filter.from_ms {
            query_builder.push(" AND ts_ms >= ").push_bind(from_ms);
        }
        if let Some(to_ms) = filter.to_ms {
            query_builder.push(" AND ts_ms <= ").push_bind(to_ms);
        }

        query_builder.push(" ORDER BY ts_ms DESC, id DESC");
        if let Some(limit) = filter.limit {
            query_builder.push(" LIMIT ").push_bind(limit);
        }

        let rows = query_builder
            .build_query_as::<OrderEventRow>()
            .fetch_all(self.pool())
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| StoredOrderEvent {
                id: row.0,
                event_id: row.1,
                run_id: row.2,
                order_id: row.3,
                client_order_id: row.4,
                broker_order_id: row.5,
                account_id: row.6,
                symbol: row.7,
                status: row.8,
                event_type: row.9,
                message: row.10,
                ts_ms: row.11,
                payload_json: row.12,
            })
            .collect())
    }

    pub async fn list_risk_events(&self, run_id: &str) -> StorageResult<Vec<StoredRiskEvent>> {
        self.list_risk_events_filtered(RiskEventFilter {
            run_id: Some(run_id.to_string()),
            ..RiskEventFilter::default()
        })
        .await
    }

    pub async fn list_risk_events_filtered(
        &self,
        filter: RiskEventFilter,
    ) -> StorageResult<Vec<StoredRiskEvent>> {
        type RiskEventRow = (
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            i64,
            String,
        );

        let mut query_builder = QueryBuilder::<Sqlite>::new(
            r#"
            SELECT id, event_id, run_id, account_id, symbol, risk_type, decision,
                   reason, threshold, observed_value, ts_ms, payload_json
            FROM risk_events
            WHERE 1 = 1
            "#,
        );

        if let Some(run_id) = filter.run_id.as_deref() {
            query_builder.push(" AND run_id = ").push_bind(run_id);
        }
        if let Some(risk_type) = filter.risk_type.as_deref() {
            query_builder.push(" AND risk_type = ").push_bind(risk_type);
        }
        if let Some(decision) = filter.decision.as_deref() {
            query_builder.push(" AND decision = ").push_bind(decision);
        }
        if let Some(account_id) = filter.account_id.as_deref() {
            query_builder
                .push(" AND account_id = ")
                .push_bind(account_id);
        }
        if let Some(symbol) = filter.symbol.as_deref() {
            query_builder.push(" AND symbol = ").push_bind(symbol);
        }
        if let Some(from_ms) = filter.from_ms {
            query_builder.push(" AND ts_ms >= ").push_bind(from_ms);
        }
        if let Some(to_ms) = filter.to_ms {
            query_builder.push(" AND ts_ms <= ").push_bind(to_ms);
        }

        query_builder.push(" ORDER BY ts_ms DESC, id DESC");
        if let Some(limit) = filter.limit {
            query_builder.push(" LIMIT ").push_bind(limit);
        }

        let rows = query_builder
            .build_query_as::<RiskEventRow>()
            .fetch_all(self.pool())
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| StoredRiskEvent {
                id: row.0,
                event_id: row.1,
                run_id: row.2,
                account_id: row.3,
                symbol: row.4,
                risk_type: row.5,
                decision: row.6,
                reason: row.7,
                threshold: row.8,
                observed_value: row.9,
                ts_ms: row.10,
                payload_json: row.11,
            })
            .collect())
    }

    pub async fn list_insights(&self, run_id: &str) -> StorageResult<Vec<StoredInsight>> {
        let rows = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                String,
                String,
                String,
                String,
                i64,
                String,
            ),
        >(
            r#"
            SELECT id, event_id, run_id, strategy, symbol, side, confidence, ts_ms, payload_json
            FROM insights
            WHERE run_id = ?
            ORDER BY ts_ms, id
            "#,
        )
        .bind(run_id)
        .fetch_all(self.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(
                    id,
                    event_id,
                    run_id,
                    strategy,
                    symbol,
                    side,
                    confidence,
                    ts_ms,
                    payload_json,
                )| {
                    StoredInsight {
                        id,
                        event_id,
                        run_id,
                        strategy,
                        symbol,
                        side,
                        confidence,
                        ts_ms,
                        payload_json,
                    }
                },
            )
            .collect())
    }

    pub async fn list_portfolio_targets(
        &self,
        run_id: &str,
    ) -> StorageResult<Vec<StoredPortfolioTarget>> {
        let rows =
            sqlx::query_as::<_, (String, String, String, String, String, String, i64, String)>(
                r#"
            SELECT id, event_id, run_id, account_id, symbol, target_qty, ts_ms, payload_json
            FROM portfolio_targets
            WHERE run_id = ?
            ORDER BY ts_ms, id
            "#,
            )
            .bind(run_id)
            .fetch_all(self.pool())
            .await?;

        Ok(rows
            .into_iter()
            .map(
                |(id, event_id, run_id, account_id, symbol, target_qty, ts_ms, payload_json)| {
                    StoredPortfolioTarget {
                        id,
                        event_id,
                        run_id,
                        account_id,
                        symbol,
                        target_qty,
                        ts_ms,
                        payload_json,
                    }
                },
            )
            .collect())
    }

    pub async fn replay_events_to_bus(&self, source: &str, bus: &EventBus) -> StorageResult<usize> {
        let events = self.list_events_by_source(source).await?;
        let envelopes = events
            .into_iter()
            .map(event_record_to_envelope)
            .collect::<Vec<_>>();
        let replayed = envelopes.len();
        bus.replay(envelopes)
            .map_err(|error| StorageError::Protocol(error.to_string()))?;
        Ok(replayed)
    }
}

#[derive(sqlx::FromRow)]
struct ConfigVersionRow {
    id: String,
    name: String,
    lifecycle_version: i64,
    content: String,
    state: String,
    parent_version: Option<i64>,
    created_by: String,
    created_at: i64,
    state_changed_at: i64,
    state_changed_by: String,
    state_change_reason: Option<String>,
    target_env: Option<String>,
    rollout: Option<String>,
    approved_by: Option<String>,
    approved_at: Option<i64>,
    published_by: Option<String>,
    published_at: Option<i64>,
}

fn config_version_id(name: &str, version: u32) -> String {
    format!("config:{name}:v{version}")
}

fn config_policy_environment(target_env: Option<&str>) -> Option<&str> {
    match target_env {
        Some("staging") => Some("staging"),
        Some("production") => Some("production"),
        _ => None,
    }
}

fn config_governance_policy_rules() -> Vec<ConfigGovernanceRule> {
    ["staging", "production"]
        .into_iter()
        .flat_map(|target_env| {
            let publish_requires_independent_actor = target_env == "production";
            [
                ConfigGovernanceRule {
                    target_env: target_env.to_string(),
                    transition_to: ConfigState::PendingReview,
                    required_role: "release_manager".to_string(),
                    required_approvals: 0,
                    requires_independent_actor: false,
                },
                ConfigGovernanceRule {
                    target_env: target_env.to_string(),
                    transition_to: ConfigState::Approved,
                    required_role: "approver".to_string(),
                    required_approvals: 1,
                    requires_independent_actor: false,
                },
                ConfigGovernanceRule {
                    target_env: target_env.to_string(),
                    transition_to: ConfigState::Published,
                    required_role: "release_manager".to_string(),
                    required_approvals: if target_env == "production" { 2 } else { 1 },
                    requires_independent_actor: publish_requires_independent_actor,
                },
                ConfigGovernanceRule {
                    target_env: target_env.to_string(),
                    transition_to: ConfigState::Archived,
                    required_role: "release_manager".to_string(),
                    required_approvals: 0,
                    requires_independent_actor: false,
                },
            ]
        })
        .collect()
}

fn config_governance_rule(
    target_env: &str,
    transition_to: ConfigState,
) -> Option<ConfigGovernanceRule> {
    config_governance_policy_rules()
        .into_iter()
        .find(|rule| rule.target_env == target_env && rule.transition_to == transition_to)
}

fn config_version_from_row(row: ConfigVersionRow) -> StorageResult<ConfigVersion> {
    let version = u32::try_from(row.lifecycle_version)
        .map_err(|error| StorageError::Protocol(format!("invalid config version: {error}")))?;
    let parent_version = row
        .parent_version
        .map(u32::try_from)
        .transpose()
        .map_err(|error| {
            StorageError::Protocol(format!("invalid parent config version: {error}"))
        })?;

    Ok(ConfigVersion {
        id: row.id,
        name: row.name,
        version,
        content_json: row.content,
        state: ConfigState::from_str(&row.state)?,
        parent_version,
        created_by: row.created_by,
        created_at_ms: row.created_at,
        state_changed_at_ms: row.state_changed_at,
        state_changed_by: row.state_changed_by,
        state_change_reason: row.state_change_reason,
        target_env: row.target_env,
        rollout: row.rollout,
        approved_by: row.approved_by,
        approved_at_ms: row.approved_at,
        published_by: row.published_by,
        published_at_ms: row.published_at,
    })
}

fn flatten_json(
    prefix: &str,
    value: &serde_json::Value,
    output: &mut BTreeMap<String, serde_json::Value>,
) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, nested) in map {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                flatten_json(&path, nested, output);
            }
        }
        _ => {
            output.insert(prefix.to_string(), value.clone());
        }
    }
}

fn event_record_to_envelope(record: EventRecord) -> AnyEventEnvelope {
    EventEnvelope {
        event_id: Uuid::parse_str(&record.event_id).unwrap_or_else(|_| Uuid::new_v4()),
        ts: Utc
            .timestamp_millis_opt(record.ts_ms)
            .single()
            .unwrap_or_else(Utc::now),
        source: record.source,
        category: EventCategory::System,
        payload: TraderEvent::Runtime(RuntimeEvent {
            category: record.category,
            payload_json: record.payload_json,
        }),
    }
}
