use crate::{Db, StorageError, StorageResult};
use chrono::{TimeZone, Utc};
use events::{AnyEventEnvelope, EventBus, EventCategory, EventEnvelope, RuntimeEvent, TraderEvent};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::Row;
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
    pub cash: Decimal,
    pub market_value: Decimal,
    pub equity: Decimal,
    pub realized_pnl: Decimal,
    pub unrealized_pnl: Decimal,
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
        symbol: &str,
        start_ms: i64,
        end_ms: i64,
    ) -> StorageResult<Vec<StoredFundingRate>> {
        let rows =
            sqlx::query_as::<_, (String, String, String, i64, String, Option<String>, String)>(
                r#"
            SELECT id, exchange, symbol, funding_time_ms, funding_rate, mark_price, source
            FROM funding_rates
            WHERE exchange = ?
              AND symbol = ?
              AND funding_time_ms >= ?
              AND funding_time_ms < ?
            ORDER BY funding_time_ms, id
            "#,
            )
            .bind(exchange)
            .bind(symbol)
            .bind(start_ms)
            .bind(end_ms)
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

    pub async fn list_cash_snapshots(
        &self,
        run_id: &str,
    ) -> StorageResult<Vec<StoredCashSnapshot>> {
        let rows = sqlx::query_as::<_, (i64, String, i64, String, String, String, String, i64)>(
            r#"
            SELECT id, run_id, ts, currency, cash, available_cash, frozen_cash, created_at
            FROM cash_snapshots
            WHERE run_id = ?
            ORDER BY ts, id
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

    pub async fn insert_position_snapshot(
        &self,
        snapshot: NewPositionSnapshot,
    ) -> StorageResult<()> {
        sqlx::query(
            r#"
            INSERT INTO position_snapshots (
                run_id, ts, market, exchange, symbol, asset_class, position_side,
                qty, available_qty, avg_price, entry_price, market_price, mark_price,
                market_value, unrealized_pnl, realized_pnl, currency, created_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
        .bind(snapshot.created_at_ms)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn list_position_snapshots(
        &self,
        run_id: &str,
    ) -> StorageResult<Vec<StoredPositionSnapshot>> {
        let rows = sqlx::query(
            r#"
            SELECT id, run_id, ts AS ts_ms, market, exchange, symbol, asset_class, position_side,
                   qty, available_qty, avg_price, entry_price, market_price, mark_price,
                   market_value, unrealized_pnl, realized_pnl, currency, created_at AS created_at_ms
            FROM position_snapshots
            WHERE run_id = ?
            ORDER BY ts, id
            "#,
        )
        .bind(run_id)
        .fetch_all(self.pool())
        .await?;

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
                created_at_ms: row.try_get("created_at_ms")?,
            });
        }
        Ok(snapshots)
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

    pub async fn list_system_logs(
        &self,
        run_id: Option<&str>,
    ) -> StorageResult<Vec<StoredSystemLog>> {
        let rows = sqlx::query_as::<
            _,
            (
                String,
                Option<String>,
                i64,
                String,
                String,
                String,
                Option<String>,
                i64,
            ),
        >(
            r#"
            SELECT id, run_id, ts, level, target, message, fields_json, created_at
            FROM system_logs
            WHERE (? IS NULL OR run_id = ?)
            ORDER BY ts, id
            "#,
        )
        .bind(run_id)
        .bind(run_id)
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
    ) -> StorageResult<()> {
        sqlx::query(
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
        Ok(())
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
            asset: command.base_currency,
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

        for position in command.positions {
            self.upsert_position(NewPosition {
                run_id: position.run_id,
                account_id: position.account_id,
                symbol: position.symbol,
                qty: position.qty.to_string(),
                avg_price: position.avg_price.to_string(),
                updated_at_ms: position.updated_at_ms,
            })
            .await?;
        }

        self.insert_portfolio_snapshot(NewPortfolioSnapshot {
            id: format!("{}-snapshot-final", command.run_id),
            run_id: command.run_id,
            account_id: command.account_id,
            ts_ms: command.ended_at_ms,
            cash: command.cash.to_string(),
            market_value: command.market_value.to_string(),
            equity: command.equity.to_string(),
            realized_pnl: command.realized_pnl.to_string(),
            unrealized_pnl: command.unrealized_pnl.to_string(),
        })
        .await
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

    pub async fn list_order_events(&self, run_id: &str) -> StorageResult<Vec<StoredOrderEvent>> {
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

        let rows = sqlx::query_as::<_, OrderEventRow>(
            r#"
            SELECT id, event_id, run_id, order_id, client_order_id, broker_order_id,
                   account_id, symbol, status, event_type, message, ts_ms, payload_json
            FROM order_events
            WHERE run_id = ?
            ORDER BY ts_ms, id
            "#,
        )
        .bind(run_id)
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

        let rows = sqlx::query_as::<_, RiskEventRow>(
            r#"
            SELECT id, event_id, run_id, account_id, symbol, risk_type, decision,
                   reason, threshold, observed_value, ts_ms, payload_json
            FROM risk_events
            WHERE run_id = ?
            ORDER BY ts_ms, id
            "#,
        )
        .bind(run_id)
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
