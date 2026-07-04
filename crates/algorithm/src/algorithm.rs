#![forbid(unsafe_code)]

use accounting::AccountBook;
use alpha::AlphaModel;
use chrono::{Datelike, FixedOffset, Offset, TimeZone, Timelike, Utc, Weekday};
use data::{Bar, MarketSlice};
use events::{EventBus, runtime_envelope};
use execution::order_for_target_delta;
use market_rules::{ContractRiskLimits, MarketRuleSet};
use oms::OrderStateMachine;
use portfolio::equal_weight_target;
use risk::{
    DailyLossGuard, MarketDataFreshnessGuard, OrderThrottleGuard, PortfolioRiskPolicy,
    PortfolioRiskState, PriceDeviationGuard, RiskPolicy, StrategyCircuitBreaker,
    TradingSessionGuard, check_max_position,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use strategies::StrategyRuntimeMode;
use trader_core::{OrderRequest, OrderSide};
use universe::{StaticUniverseSelector, UniverseContext, UniverseSelector};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlgorithmEngineSettings {
    pub run_id: String,
    pub mode: StrategyRuntimeMode,
    pub account_id: String,
    pub symbol: String,
    pub order_qty: Decimal,
    pub max_abs_qty: Decimal,
    pub max_order_qty: Decimal,
    pub max_order_notional: Decimal,
    pub min_cash_after_order: Decimal,
    pub max_exposure: Decimal,
    pub max_drawdown: Decimal,
    pub max_leverage: Decimal,
    pub max_margin_used: Decimal,
    pub trading_halted: bool,
    pub allow_short: bool,
    pub shortable_symbols: BTreeSet<String>,
    pub initial_cash: Decimal,
    pub daily_loss_limit: Option<Decimal>,
    pub max_order_attempts_per_day: Option<u32>,
    pub max_order_failures_per_day: Option<u32>,
    pub max_price_deviation_bps: Option<Decimal>,
    pub max_market_data_age_ms: Option<u64>,
    pub max_consecutive_strategy_losses: Option<u32>,
    pub max_consecutive_strategy_errors: Option<u32>,
    pub trading_session: Option<TradingSessionWindow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TradingSessionWindow {
    pub mode: String,
    pub timezone: String,
    pub start: String,
    pub end: String,
}

impl TradingSessionWindow {
    pub fn new(
        mode: impl Into<String>,
        timezone: impl Into<String>,
        start: impl Into<String>,
        end: impl Into<String>,
    ) -> Self {
        Self {
            mode: mode.into(),
            timezone: timezone.into(),
            start: start.into(),
            end: end.into(),
        }
    }

    fn guard(&self) -> anyhow::Result<TradingSessionGuard> {
        Ok(TradingSessionGuard::new(
            parse_hhmm_to_minutes(&self.start)?,
            parse_hhmm_to_minutes(&self.end)?,
        ))
    }

    fn check(&self, ts_ms: i64) -> anyhow::Result<()> {
        if self.mode != "regular_only" {
            return Ok(());
        }
        let offset = timezone_offset_for_timestamp(&self.timezone, ts_ms)?;
        let local = offset
            .timestamp_millis_opt(ts_ms)
            .single()
            .ok_or_else(|| anyhow::anyhow!("invalid timestamp {ts_ms} for trading session"))?;
        self.guard()?
            .check(
                !matches!(local.weekday(), Weekday::Sat | Weekday::Sun),
                local.hour() * 60 + local.minute(),
            )
            .map_err(anyhow::Error::from)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineEventKind {
    UniverseSelected,
    AlphaGenerated,
    PortfolioTargetGenerated,
    MarketRuleValidated,
    RiskApproved,
    RiskRejected,
    ExecutionOrderGenerated,
    OmsSubmitted,
    OmsAccepted,
    BrokerOrderSubmitted,
    BrokerOrderFilled,
    BrokerOrderPartiallyFilled,
    BrokerOrderUnfilled,
    AccountingUpdated,
    PortfolioSnapshot,
}

impl EngineEventKind {
    pub fn category(self) -> &'static str {
        match self {
            Self::UniverseSelected => "algorithm.universe.selected",
            Self::AlphaGenerated => "algorithm.alpha.generated",
            Self::PortfolioTargetGenerated => "algorithm.portfolio.target",
            Self::MarketRuleValidated => "algorithm.market_rule.validated",
            Self::RiskApproved => "algorithm.risk.approved",
            Self::RiskRejected => "algorithm.risk.rejected",
            Self::ExecutionOrderGenerated => "algorithm.execution.order",
            Self::OmsSubmitted => "algorithm.oms.submitted",
            Self::OmsAccepted => "algorithm.oms.accepted",
            Self::BrokerOrderSubmitted => "broker.order.submitted",
            Self::BrokerOrderFilled => "broker.order.filled",
            Self::BrokerOrderPartiallyFilled => "broker.order.partially_filled",
            Self::BrokerOrderUnfilled => "broker.order.unfilled",
            Self::AccountingUpdated => "accounting.updated",
            Self::PortfolioSnapshot => "portfolio.snapshot",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineEvent {
    pub kind: EngineEventKind,
    pub category: String,
    pub ts_ms: i64,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UniverseSelectedPayload {
    pub run_id: String,
    pub mode: String,
    pub symbols: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlphaGeneratedPayload {
    pub run_id: String,
    pub symbol: String,
    pub side: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortfolioTargetGeneratedPayload {
    pub run_id: String,
    pub symbol: String,
    pub target_qty: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskRejectedPayload {
    pub run_id: String,
    pub account_id: String,
    pub symbol: String,
    pub risk_type: String,
    pub decision: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlgorithmOrderPayload {
    pub run_id: String,
    pub broker_order_id: Option<String>,
    pub account_id: String,
    pub symbol: String,
    pub side: String,
    pub order_type: String,
    pub qty: String,
    pub filled_qty: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountingUpdatedPayload {
    pub run_id: String,
    pub cash: String,
    pub realized_pnl: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AlgorithmDecision {
    pub order_number: usize,
    pub order_id: String,
    pub fill_id: String,
    pub order: Option<OrderRequest>,
    pub events: Vec<EngineEvent>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AlgorithmStep {
    pub decision: Option<AlgorithmDecision>,
    pub decisions: Vec<AlgorithmDecision>,
    pub snapshot: AccountSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountSnapshot {
    pub cash: Decimal,
    pub market_value: Decimal,
    pub equity: Decimal,
    pub realized_pnl: Decimal,
    pub unrealized_pnl: Decimal,
    pub position_qty: Decimal,
    pub position_avg_price: Decimal,
    pub positions: Vec<PositionSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PositionSnapshot {
    pub symbol: String,
    pub qty: Decimal,
    pub avg_price: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedExecution {
    pub events: Vec<EngineEvent>,
    pub snapshot: AccountSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionReport {
    pub broker_order_id: String,
    pub status: String,
    pub price: Decimal,
    pub qty: Decimal,
    pub fee: Decimal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PositionSide {
    Long,
    Short,
}

impl PositionSide {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Long => "long",
            Self::Short => "short",
        }
    }

    fn from_signed_qty(qty: Decimal) -> Option<Self> {
        if qty > Decimal::ZERO {
            Some(Self::Long)
        } else if qty < Decimal::ZERO {
            Some(Self::Short)
        } else {
            None
        }
    }

    fn opposite(self) -> Self {
        match self {
            Self::Long => Self::Short,
            Self::Short => Self::Long,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractFill {
    pub run_id: String,
    pub account_id: String,
    pub exchange: String,
    pub symbol: String,
    pub asset_class: String,
    pub margin_mode: String,
    pub side: OrderSide,
    pub qty: Decimal,
    pub price: Decimal,
    pub fee: Decimal,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FundingRateEvent {
    pub exchange: String,
    pub symbol: String,
    pub funding_time_ms: i64,
    pub funding_rate: Decimal,
    pub mark_price: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrokerPositionSnapshot {
    pub account_id: String,
    pub exchange: String,
    pub symbol: String,
    pub position_side: PositionSide,
    pub qty: Decimal,
    pub avg_price: Decimal,
    pub margin_used: Decimal,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CashSnapshot {
    pub currency: String,
    pub total: Decimal,
    pub available: Decimal,
    pub locked: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrokerCashSnapshot {
    pub currency: String,
    pub total: Decimal,
    pub available: Decimal,
    pub locked: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimePositionSnapshot {
    pub symbol: String,
    pub position_side: PositionSide,
    pub qty: Decimal,
    pub avg_price: Decimal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriftSeverity {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CashDrift {
    pub currency: String,
    pub runtime_total: Decimal,
    pub broker_total: Decimal,
    pub drift_abs: Decimal,
    pub drift_bps: Decimal,
    pub severity: DriftSeverity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PositionSnapshotDrift {
    pub symbol: String,
    pub position_side: PositionSide,
    pub runtime_qty: Decimal,
    pub broker_qty: Decimal,
    pub drift_qty: Decimal,
    pub reason: String,
    pub severity: DriftSeverity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconciliationSnapshotReport {
    pub run_id: String,
    pub account_id: String,
    pub ts_ms: i64,
    pub cash_drift: Option<CashDrift>,
    pub position_drifts: Vec<PositionSnapshotDrift>,
    pub severity: DriftSeverity,
}

impl ReconciliationSnapshotReport {
    pub fn new(
        run_id: &str,
        account_id: &str,
        ts_ms: i64,
        runtime_cash: Option<CashSnapshot>,
        broker_cash: Option<BrokerCashSnapshot>,
        runtime_positions: Vec<RuntimePositionSnapshot>,
        broker_positions: Vec<BrokerPositionSnapshot>,
        cash_threshold_bps: Decimal,
    ) -> Self {
        let cash_drift = reconcile_cash(runtime_cash, broker_cash, cash_threshold_bps);
        let position_drifts = reconcile_snapshot_positions(&runtime_positions, &broker_positions);
        let mut severity = DriftSeverity::Info;
        if let Some(cash_drift) = &cash_drift {
            severity = severity.max(cash_drift.severity);
        }
        for drift in &position_drifts {
            severity = severity.max(drift.severity);
        }

        Self {
            run_id: run_id.to_string(),
            account_id: account_id.to_string(),
            ts_ms,
            cash_drift,
            position_drifts,
            severity,
        }
    }
}

fn reconcile_cash(
    runtime_cash: Option<CashSnapshot>,
    broker_cash: Option<BrokerCashSnapshot>,
    threshold_bps: Decimal,
) -> Option<CashDrift> {
    let runtime_cash = runtime_cash?;
    let broker_cash = broker_cash?;
    if runtime_cash.currency != broker_cash.currency {
        return Some(CashDrift {
            currency: runtime_cash.currency,
            runtime_total: runtime_cash.total,
            broker_total: broker_cash.total,
            drift_abs: (runtime_cash.total - broker_cash.total).abs(),
            drift_bps: Decimal::MAX,
            severity: DriftSeverity::Error,
        });
    }

    let drift_abs = (runtime_cash.total - broker_cash.total).abs();
    let drift_bps = if broker_cash.total == Decimal::ZERO {
        if drift_abs == Decimal::ZERO {
            Decimal::ZERO
        } else {
            Decimal::MAX
        }
    } else {
        drift_abs / broker_cash.total.abs() * Decimal::from(10_000)
    };

    if drift_bps <= threshold_bps {
        return None;
    }

    Some(CashDrift {
        currency: runtime_cash.currency,
        runtime_total: runtime_cash.total,
        broker_total: broker_cash.total,
        drift_abs,
        drift_bps,
        severity: DriftSeverity::Warn,
    })
}

fn reconcile_snapshot_positions(
    runtime_positions: &[RuntimePositionSnapshot],
    broker_positions: &[BrokerPositionSnapshot],
) -> Vec<PositionSnapshotDrift> {
    let mut drifts = Vec::new();
    for runtime_position in runtime_positions {
        let broker_position = broker_positions.iter().find(|broker_position| {
            broker_position.symbol == runtime_position.symbol
                && broker_position.position_side == runtime_position.position_side
        });
        match broker_position {
            Some(broker_position)
                if broker_position.qty != runtime_position.qty
                    || broker_position.avg_price != runtime_position.avg_price =>
            {
                drifts.push(PositionSnapshotDrift {
                    symbol: runtime_position.symbol.clone(),
                    position_side: runtime_position.position_side,
                    runtime_qty: runtime_position.qty,
                    broker_qty: broker_position.qty,
                    drift_qty: runtime_position.qty - broker_position.qty,
                    reason: if broker_position.qty != runtime_position.qty {
                        "qty mismatch".to_string()
                    } else {
                        "avg_price mismatch".to_string()
                    },
                    severity: DriftSeverity::Error,
                });
            }
            Some(_) => {}
            None => drifts.push(PositionSnapshotDrift {
                symbol: runtime_position.symbol.clone(),
                position_side: runtime_position.position_side,
                runtime_qty: runtime_position.qty,
                broker_qty: Decimal::ZERO,
                drift_qty: runtime_position.qty,
                reason: "missing broker position".to_string(),
                severity: DriftSeverity::Error,
            }),
        }
    }

    for broker_position in broker_positions {
        let has_runtime_position = runtime_positions.iter().any(|runtime_position| {
            runtime_position.symbol == broker_position.symbol
                && runtime_position.position_side == broker_position.position_side
        });
        if !has_runtime_position {
            drifts.push(PositionSnapshotDrift {
                symbol: broker_position.symbol.clone(),
                position_side: broker_position.position_side,
                runtime_qty: Decimal::ZERO,
                broker_qty: broker_position.qty,
                drift_qty: -broker_position.qty,
                reason: "missing runtime position".to_string(),
                severity: DriftSeverity::Error,
            });
        }
    }

    drifts
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractPosition {
    pub run_id: String,
    pub account_id: String,
    pub exchange: String,
    pub symbol: String,
    pub asset_class: String,
    pub margin_mode: String,
    pub position_side: PositionSide,
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
pub struct ReconciliationDrift {
    pub symbol: String,
    pub position_side: PositionSide,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReconciliationResult {
    pub drifts: Vec<ReconciliationDrift>,
}

impl ReconciliationResult {
    pub fn drift_count(&self) -> usize {
        self.drifts.len()
    }
}

pub type AccountingError = anyhow::Error;

pub trait ContractAccountingBook {
    fn on_fill<'a>(
        &'a mut self,
        fill: &'a ContractFill,
    ) -> impl std::future::Future<Output = Result<(), AccountingError>> + Send + 'a;

    fn on_funding<'a>(
        &'a mut self,
        rate: &'a FundingRateEvent,
    ) -> impl std::future::Future<Output = Result<(), AccountingError>> + Send + 'a;

    fn on_reconciliation<'a>(
        &'a mut self,
        broker_state: &'a BrokerPositionSnapshot,
    ) -> impl std::future::Future<Output = Result<ReconciliationResult, AccountingError>> + Send + 'a;

    fn get_position(&self, symbol: &str, side: PositionSide) -> Option<&ContractPosition>;
}

#[derive(Debug, Clone)]
pub struct SimulatedContractAccounting {
    account_id: String,
    default_leverage: Decimal,
    positions: BTreeMap<(String, PositionSide), ContractPosition>,
}

impl SimulatedContractAccounting {
    pub fn new(account_id: String, default_leverage: Decimal) -> Self {
        Self {
            account_id,
            default_leverage,
            positions: BTreeMap::new(),
        }
    }

    pub fn positions(&self) -> impl Iterator<Item = &ContractPosition> {
        self.positions.values()
    }

    fn position_mut_for_fill(&mut self, fill: &ContractFill) -> &mut ContractPosition {
        let signed_qty = signed_fill_qty(fill);
        let fill_side = PositionSide::from_signed_qty(signed_qty).unwrap_or(PositionSide::Long);
        let opposite_side = fill_side.opposite();
        let opposite_key = (fill.symbol.clone(), opposite_side);
        let target_side = if self
            .positions
            .get(&opposite_key)
            .is_some_and(|position| position.qty != Decimal::ZERO)
        {
            opposite_side
        } else {
            fill_side
        };
        let key = (fill.symbol.clone(), target_side);
        self.positions
            .entry(key)
            .or_insert_with(|| ContractPosition {
                run_id: fill.run_id.clone(),
                account_id: self.account_id.clone(),
                exchange: fill.exchange.clone(),
                symbol: fill.symbol.clone(),
                asset_class: fill.asset_class.clone(),
                margin_mode: fill.margin_mode.clone(),
                position_side: target_side,
                leverage: self.default_leverage,
                qty: Decimal::ZERO,
                avg_price: Decimal::ZERO,
                margin_used: Decimal::ZERO,
                funding_fee: Decimal::ZERO,
                realized_pnl: Decimal::ZERO,
                unrealized_pnl: Decimal::ZERO,
                updated_at_ms: fill.ts_ms,
            })
    }

    fn recalculate_margin(position: &mut ContractPosition) {
        position.margin_used =
            if position.qty == Decimal::ZERO || position.leverage == Decimal::ZERO {
                Decimal::ZERO
            } else {
                position.qty.abs() * position.avg_price / position.leverage
            };
    }
}

impl ContractAccountingBook for SimulatedContractAccounting {
    fn on_fill<'a>(
        &'a mut self,
        fill: &'a ContractFill,
    ) -> impl std::future::Future<Output = Result<(), AccountingError>> + Send + 'a {
        async move {
            if fill.qty < Decimal::ZERO {
                anyhow::bail!("contract fill quantity must be non-negative");
            }

            let signed_qty = signed_fill_qty(fill);
            let position = self.position_mut_for_fill(fill);
            let current_qty = position.qty;
            let next_qty = current_qty + signed_qty;

            if current_qty == Decimal::ZERO || same_sign(current_qty, signed_qty) {
                let next_abs_qty = next_qty.abs();
                position.avg_price = if next_abs_qty == Decimal::ZERO {
                    Decimal::ZERO
                } else {
                    ((current_qty.abs() * position.avg_price) + (signed_qty.abs() * fill.price))
                        / next_abs_qty
                };
            } else {
                let closed_qty = current_qty.abs().min(signed_qty.abs());
                let direction = if current_qty > Decimal::ZERO {
                    Decimal::ONE
                } else {
                    -Decimal::ONE
                };
                position.realized_pnl += (fill.price - position.avg_price) * closed_qty * direction;
                if next_qty == Decimal::ZERO {
                    position.avg_price = Decimal::ZERO;
                }
            }

            position.realized_pnl -= fill.fee;
            position.qty = next_qty;
            position.updated_at_ms = fill.ts_ms;
            Self::recalculate_margin(position);
            Ok(())
        }
    }

    fn on_funding<'a>(
        &'a mut self,
        rate: &'a FundingRateEvent,
    ) -> impl std::future::Future<Output = Result<(), AccountingError>> + Send + 'a {
        async move {
            for position in self
                .positions
                .values_mut()
                .filter(|position| position.symbol == rate.symbol && position.qty != Decimal::ZERO)
            {
                let funding_fee = -(position.qty * rate.funding_rate * rate.mark_price);
                position.funding_fee += funding_fee;
                position.realized_pnl += funding_fee;
                position.updated_at_ms = rate.funding_time_ms;
            }
            Ok(())
        }
    }

    fn on_reconciliation<'a>(
        &'a mut self,
        broker_state: &'a BrokerPositionSnapshot,
    ) -> impl std::future::Future<Output = Result<ReconciliationResult, AccountingError>> + Send + 'a
    {
        async move {
            let mut result = ReconciliationResult::default();
            let Some(position) =
                self.get_position(&broker_state.symbol, broker_state.position_side)
            else {
                result.drifts.push(ReconciliationDrift {
                    symbol: broker_state.symbol.clone(),
                    position_side: broker_state.position_side,
                    reason: "missing runtime position".to_string(),
                });
                return Ok(result);
            };

            if position.qty != broker_state.qty {
                result.drifts.push(ReconciliationDrift {
                    symbol: broker_state.symbol.clone(),
                    position_side: broker_state.position_side,
                    reason: format!(
                        "qty mismatch runtime={} broker={}",
                        position.qty, broker_state.qty
                    ),
                });
            }
            if position.avg_price != broker_state.avg_price {
                result.drifts.push(ReconciliationDrift {
                    symbol: broker_state.symbol.clone(),
                    position_side: broker_state.position_side,
                    reason: format!(
                        "avg_price mismatch runtime={} broker={}",
                        position.avg_price, broker_state.avg_price
                    ),
                });
            }
            if position.margin_used != broker_state.margin_used {
                result.drifts.push(ReconciliationDrift {
                    symbol: broker_state.symbol.clone(),
                    position_side: broker_state.position_side,
                    reason: format!(
                        "margin mismatch runtime={} broker={}",
                        position.margin_used, broker_state.margin_used
                    ),
                });
            }
            Ok(result)
        }
    }

    fn get_position(&self, symbol: &str, side: PositionSide) -> Option<&ContractPosition> {
        self.positions.get(&(symbol.to_string(), side))
    }
}

fn signed_fill_qty(fill: &ContractFill) -> Decimal {
    match fill.side {
        OrderSide::Buy => fill.qty,
        OrderSide::Sell => -fill.qty,
    }
}

fn same_sign(left: Decimal, right: Decimal) -> bool {
    (left > Decimal::ZERO && right > Decimal::ZERO)
        || (left < Decimal::ZERO && right < Decimal::ZERO)
}

pub struct AlgorithmEngine {
    settings: AlgorithmEngineSettings,
    universe: Box<dyn UniverseSelector>,
    alpha: Box<dyn AlphaModel + Send + Sync>,
    account_book: AccountBook,
    portfolio_risk: PortfolioRiskPolicy,
    event_bus: Option<EventBus>,
    last_prices: BTreeMap<String, Decimal>,
    peak_equity: Decimal,
    orders: usize,
    day_start_equity: Decimal,
    order_attempts_today: u32,
    order_failures_today: u32,
    consecutive_strategy_losses: u32,
    consecutive_strategy_errors: u32,
}

impl AlgorithmEngine {
    pub fn new(
        settings: AlgorithmEngineSettings,
        alpha: Box<dyn AlphaModel + Send + Sync>,
    ) -> Self {
        let universe = Box::new(StaticUniverseSelector::new(vec![settings.symbol.clone()]));
        Self::new_with_universe(settings, universe, alpha)
    }

    pub fn new_with_universe(
        settings: AlgorithmEngineSettings,
        universe: Box<dyn UniverseSelector>,
        alpha: Box<dyn AlphaModel + Send + Sync>,
    ) -> Self {
        let account_book = AccountBook::new(settings.account_id.clone(), settings.initial_cash);
        let portfolio_risk = PortfolioRiskPolicy::new(
            settings.max_exposure,
            settings.max_drawdown,
            settings.max_leverage,
            settings.max_margin_used,
        )
        .with_shorting(settings.allow_short);
        Self {
            peak_equity: settings.initial_cash,
            day_start_equity: settings.initial_cash,
            settings,
            universe,
            alpha,
            account_book,
            portfolio_risk,
            event_bus: None,
            last_prices: BTreeMap::new(),
            orders: 0,
            order_attempts_today: 0,
            order_failures_today: 0,
            consecutive_strategy_losses: 0,
            consecutive_strategy_errors: 0,
        }
    }

    pub fn set_event_bus(&mut self, event_bus: EventBus) {
        self.event_bus = Some(event_bus);
    }

    pub fn record_order_failure(&mut self) {
        self.order_failures_today = self.order_failures_today.saturating_add(1);
    }

    pub fn on_bar(&mut self, bar: Bar) -> anyhow::Result<AlgorithmStep> {
        let symbol = self.settings.symbol.clone();
        self.on_market_slice(MarketSlice::single(symbol, bar))
    }

    pub fn on_market_slice(&mut self, market_slice: MarketSlice) -> anyhow::Result<AlgorithmStep> {
        for (symbol, bar) in market_slice.iter() {
            self.last_prices.insert(symbol.to_string(), bar.close);
        }
        let Some(primary_bar) = market_slice
            .bar(&self.settings.symbol)
            .or_else(|| market_slice.iter().next().map(|(_, bar)| bar))
        else {
            return Ok(AlgorithmStep {
                decision: None,
                decisions: Vec::new(),
                snapshot: self.snapshot_from_prices()?,
            });
        };

        let context = UniverseContext::new(self.settings.symbol.clone(), primary_bar.clone())
            .with_available_symbols(market_slice.symbols());
        let selected = self.universe.select(&context)?;
        let universe_event = self.event(
            EngineEventKind::UniverseSelected,
            market_slice.ts_ms,
            payload_value(UniverseSelectedPayload {
                run_id: self.settings.run_id.clone(),
                mode: format!("{:?}", self.settings.mode),
                symbols: selected.clone(),
            }),
        );
        let mut decisions = Vec::new();
        for symbol in selected {
            let Some(bar) = market_slice.bar(&symbol) else {
                continue;
            };
            let include_universe_event = decisions.is_empty();
            if let Some(decision) = self.decision_for_symbol(
                &symbol,
                bar,
                market_slice.ts_ms,
                include_universe_event,
                &universe_event,
            )? {
                decisions.push(decision);
            }
        }

        if decisions.is_empty() {
            self.publish_events(std::slice::from_ref(&universe_event));
            return Ok(AlgorithmStep {
                decision: None,
                decisions,
                snapshot: self.snapshot_from_prices()?,
            });
        };

        for decision in &decisions {
            self.publish_events(&decision.events);
        }
        Ok(AlgorithmStep {
            decision: decisions.first().cloned(),
            decisions,
            snapshot: self.snapshot_from_prices()?,
        })
    }

    fn decision_for_symbol(
        &mut self,
        symbol: &str,
        bar: &Bar,
        evaluation_ts_ms: i64,
        include_universe_event: bool,
        universe_event: &EngineEvent,
    ) -> anyhow::Result<Option<AlgorithmDecision>> {
        let mut events = if include_universe_event {
            vec![universe_event.clone()]
        } else {
            Vec::new()
        };

        let Some(signal) = self.alpha.on_bar_for_symbol(symbol, bar) else {
            return Ok(None);
        };
        tracing::info!(
            run_id = %self.settings.run_id,
            symbol = %signal.symbol,
            side = ?signal.side,
            confidence = signal.confidence,
            ts_ms = bar.ts_ms,
            category = "trading",
            "algorithm alpha generated"
        );
        events.push(self.event(
            EngineEventKind::AlphaGenerated,
            bar.ts_ms,
            payload_value(AlphaGeneratedPayload {
                run_id: self.settings.run_id.clone(),
                symbol: signal.symbol.clone(),
                side: format!("{:?}", signal.side),
                confidence: signal.confidence,
            }),
        ));

        let target = equal_weight_target(&signal, self.settings.order_qty);
        check_max_position(&target, self.settings.max_abs_qty)?;
        tracing::info!(
            run_id = %self.settings.run_id,
            symbol = %target.symbol,
            target_qty = %target.target_qty,
            ts_ms = bar.ts_ms,
            category = "trading",
            "algorithm portfolio target generated"
        );
        events.push(self.event(
            EngineEventKind::PortfolioTargetGenerated,
            bar.ts_ms,
            payload_value(PortfolioTargetGeneratedPayload {
                run_id: self.settings.run_id.clone(),
                symbol: target.symbol.clone(),
                target_qty: target.target_qty.to_string(),
            }),
        ));

        let current_qty = self
            .account_book
            .position(&target.symbol)
            .map_or(Decimal::ZERO, |position| position.qty);
        let order = order_for_target_delta(&target, current_qty, self.settings.account_id.clone());
        let Some(order) = order else {
            return Ok(None);
        };
        if let Some(window) = &self.settings.trading_session
            && let Err(error) = window.check(evaluation_ts_ms)
        {
            events.push(self.risk_rejected_event(
                bar.ts_ms,
                &order,
                "trading_session_closed",
                error.to_string(),
            ));
            return Ok(Some(self.rejected_decision(events)));
        }
        if let Some(max_market_data_age_ms) = self.settings.max_market_data_age_ms
            && let Err(rejection) = MarketDataFreshnessGuard::new(max_market_data_age_ms)
                .check(bar.ts_ms, evaluation_ts_ms)
        {
            events.push(self.risk_rejected_event(
                bar.ts_ms,
                &order,
                rejection.risk_type,
                rejection.reason,
            ));
            return Ok(Some(self.rejected_decision(events)));
        }
        let equity = self.account_book.equity_with_prices(&self.last_prices);
        if let Some(daily_loss_limit) = self.settings.daily_loss_limit
            && let Err(rejection) =
                DailyLossGuard::new(daily_loss_limit).check(self.day_start_equity, equity)
        {
            events.push(self.risk_rejected_event(
                bar.ts_ms,
                &order,
                rejection.risk_type,
                rejection.reason,
            ));
            return Ok(Some(self.rejected_decision(events)));
        }
        if let Err(rejection) = StrategyCircuitBreaker::new(
            self.settings.max_consecutive_strategy_losses,
            self.settings.max_consecutive_strategy_errors,
        )
        .check(
            self.consecutive_strategy_losses,
            self.consecutive_strategy_errors,
        ) {
            events.push(self.risk_rejected_event(
                bar.ts_ms,
                &order,
                rejection.risk_type,
                rejection.reason,
            ));
            return Ok(Some(self.rejected_decision(events)));
        }
        if let Err(rejection) = OrderThrottleGuard::new(
            self.settings.max_order_attempts_per_day,
            self.settings.max_order_failures_per_day,
        )
        .check_attempts(self.order_attempts_today)
        {
            events.push(self.risk_rejected_event(
                bar.ts_ms,
                &order,
                rejection.risk_type,
                rejection.reason,
            ));
            return Ok(Some(self.rejected_decision(events)));
        }
        if let Some(max_price_deviation_bps) = self.settings.max_price_deviation_bps
            && let Some(order_price) = order.price
            && let Err(rejection) =
                PriceDeviationGuard::new(max_price_deviation_bps).check(order_price, bar.close)
        {
            events.push(self.risk_rejected_event(
                bar.ts_ms,
                &order,
                rejection.risk_type,
                rejection.reason,
            ));
            return Ok(Some(self.rejected_decision(events)));
        }
        let market_rules = MarketRuleSet::for_symbol(&order.symbol)?;
        market_rules.validate_order(&order, bar.close)?;
        if let Some(contract_limits) = ContractRiskLimits::for_symbol(&order.symbol) {
            let target_notional = target.target_qty.abs() * bar.close;
            let projected_margin = position_margin(&market_rules, target.target_qty, bar.close);
            let margin_ratio = if projected_margin == Decimal::ZERO {
                Decimal::MAX
            } else {
                equity / projected_margin
            };
            contract_limits.validate(
                self.settings.max_leverage,
                target_notional,
                margin_ratio,
                contract_limits.liquidation_buffer_bps,
                Decimal::ZERO,
            )?;
        }
        events.push(self.event(
            EngineEventKind::MarketRuleValidated,
            bar.ts_ms,
            order_payload(
                &self.settings.run_id,
                &order,
                Decimal::ZERO,
                "validated",
                None,
            ),
        ));

        let gross_exposure = self
            .account_book
            .gross_exposure_with_prices(&self.last_prices);
        let current_margin_used = self.margin_used_with_prices()?;
        let current_symbol_margin = position_margin(&market_rules, current_qty, bar.close);
        let target_symbol_margin = position_margin(&market_rules, target.target_qty, bar.close);
        let projected_margin_used =
            (current_margin_used - current_symbol_margin).max(Decimal::ZERO) + target_symbol_margin;
        let portfolio_state = PortfolioRiskState::new(
            equity,
            self.peak_equity,
            gross_exposure,
            projected_margin_used,
            self.settings.trading_halted,
        );
        let symbol_risk = self.portfolio_risk.clone().with_shorting(
            self.settings.allow_short || self.settings.shortable_symbols.contains(&target.symbol),
        );
        symbol_risk.check_projected_target(&target, current_qty, bar.close, &portfolio_state)?;
        RiskPolicy::new(
            self.settings.max_order_qty,
            self.settings.max_order_notional,
            self.settings.min_cash_after_order,
        )
        .check_order(
            &order,
            bar.close,
            self.account_book.cash(),
            self.settings.trading_halted,
        )?;
        tracing::info!(
            run_id = %self.settings.run_id,
            symbol = %order.symbol,
            qty = %order.qty,
            side = ?order.side,
            price = %bar.close,
            cash = %self.account_book.cash(),
            ts_ms = bar.ts_ms,
            category = "risk",
            "algorithm risk approved"
        );
        events.push(self.event(
            EngineEventKind::RiskApproved,
            bar.ts_ms,
            order_payload(
                &self.settings.run_id,
                &order,
                Decimal::ZERO,
                "approved",
                None,
            ),
        ));

        events.push(self.event(
            EngineEventKind::ExecutionOrderGenerated,
            bar.ts_ms,
            order_payload(
                &self.settings.run_id,
                &order,
                Decimal::ZERO,
                "generated",
                None,
            ),
        ));
        tracing::info!(
            run_id = %self.settings.run_id,
            symbol = %order.symbol,
            qty = %order.qty,
            side = ?order.side,
            price = %bar.close,
            ts_ms = bar.ts_ms,
            category = "trading",
            "algorithm execution order generated"
        );
        let mut order_state = OrderStateMachine::with_order_qty(order.qty);
        order_state.submit()?;
        events.push(self.event(
            EngineEventKind::OmsSubmitted,
            bar.ts_ms,
            order_payload(
                &self.settings.run_id,
                &order,
                Decimal::ZERO,
                "submitted",
                None,
            ),
        ));
        order_state.accept()?;
        events.push(self.event(
            EngineEventKind::OmsAccepted,
            bar.ts_ms,
            order_payload(
                &self.settings.run_id,
                &order,
                Decimal::ZERO,
                "accepted",
                None,
            ),
        ));

        self.order_attempts_today += 1;
        self.orders += 1;
        let order_number = self.orders;
        let decision = AlgorithmDecision {
            order_number,
            order_id: format!("{}-order-{}", self.settings.run_id, order_number),
            fill_id: format!("{}-fill-{}", self.settings.run_id, order_number),
            order: Some(order),
            events,
        };
        Ok(Some(decision))
    }

    pub fn apply_execution(
        &mut self,
        order: &OrderRequest,
        report: &ExecutionReport,
        ts_ms: i64,
    ) -> anyhow::Result<AppliedExecution> {
        let realized_pnl_before = self.account_book.realized_pnl();
        if report.qty > Decimal::ZERO {
            match order.side {
                OrderSide::Buy => {
                    self.account_book
                        .buy(&order.symbol, report.qty, report.price, report.fee)
                }
                OrderSide::Sell => {
                    self.account_book
                        .sell(&order.symbol, report.qty, report.price, report.fee)?
                }
            }
        }
        let realized_pnl_after = self.account_book.realized_pnl();
        let realized_delta = realized_pnl_after - realized_pnl_before;
        if realized_delta < Decimal::ZERO {
            self.consecutive_strategy_losses = self.consecutive_strategy_losses.saturating_add(1);
        } else if realized_delta > Decimal::ZERO {
            self.consecutive_strategy_losses = 0;
        }
        self.last_prices.insert(order.symbol.clone(), report.price);
        let fill_kind = if report.qty > Decimal::ZERO && report.qty < order.qty {
            EngineEventKind::BrokerOrderPartiallyFilled
        } else if report.qty > Decimal::ZERO {
            EngineEventKind::BrokerOrderFilled
        } else {
            EngineEventKind::BrokerOrderUnfilled
        };
        tracing::info!(
            run_id = %self.settings.run_id,
            symbol = %order.symbol,
            qty = %report.qty,
            price = %report.price,
            fee = %report.fee,
            status = %report.status,
            broker_order_id = %report.broker_order_id,
            ts_ms = ts_ms,
            category = "trading",
            "algorithm execution applied"
        );
        let events = vec![
            self.event(
                fill_kind,
                ts_ms,
                order_payload(
                    &self.settings.run_id,
                    order,
                    report.qty,
                    &report.status,
                    Some(&report.broker_order_id),
                ),
            ),
            self.event(
                EngineEventKind::AccountingUpdated,
                ts_ms,
                payload_value(AccountingUpdatedPayload {
                    run_id: self.settings.run_id.clone(),
                    cash: self.account_book.cash().to_string(),
                    realized_pnl: self.account_book.realized_pnl().to_string(),
                }),
            ),
        ];
        self.publish_events(&events);
        Ok(AppliedExecution {
            events,
            snapshot: self.snapshot_from_prices()?,
        })
    }

    pub fn snapshot(&mut self, mark_price: Decimal) -> anyhow::Result<AccountSnapshot> {
        self.last_prices
            .insert(self.settings.symbol.clone(), mark_price);
        self.snapshot_from_prices()
    }

    pub fn snapshot_from_prices(&mut self) -> anyhow::Result<AccountSnapshot> {
        let market_value = self
            .account_book
            .market_value_with_prices(&self.last_prices);
        let gross_exposure = self
            .account_book
            .gross_exposure_with_prices(&self.last_prices);
        let equity = self.account_book.equity_with_prices(&self.last_prices);
        let margin_used = self.margin_used_with_prices()?;
        self.portfolio_risk
            .check_portfolio(&PortfolioRiskState::new(
                equity,
                self.peak_equity,
                gross_exposure,
                margin_used,
                self.settings.trading_halted,
            ))?;
        if equity > self.peak_equity {
            self.peak_equity = equity;
        }
        let position = self.account_book.position(&self.settings.symbol);
        let positions = self
            .account_book
            .positions()
            .into_iter()
            .filter(|position| position.qty != Decimal::ZERO)
            .map(|position| PositionSnapshot {
                symbol: position.symbol.clone(),
                qty: position.qty,
                avg_price: position.avg_price,
            })
            .collect::<Vec<_>>();
        Ok(AccountSnapshot {
            cash: self.account_book.cash(),
            market_value,
            equity,
            realized_pnl: self.account_book.realized_pnl(),
            unrealized_pnl: self
                .account_book
                .unrealized_pnl_with_prices(&self.last_prices),
            position_qty: position.map_or(Decimal::ZERO, |position| position.qty),
            position_avg_price: position.map_or(Decimal::ZERO, |position| position.avg_price),
            positions,
        })
    }

    fn event(&self, kind: EngineEventKind, ts_ms: i64, payload: serde_json::Value) -> EngineEvent {
        EngineEvent {
            kind,
            category: kind.category().to_string(),
            ts_ms,
            payload,
        }
    }

    fn risk_rejected_event(
        &self,
        ts_ms: i64,
        order: &OrderRequest,
        risk_type: &str,
        reason: String,
    ) -> EngineEvent {
        self.event(
            EngineEventKind::RiskRejected,
            ts_ms,
            payload_value(RiskRejectedPayload {
                run_id: self.settings.run_id.clone(),
                account_id: order.account_id.clone(),
                symbol: order.symbol.clone(),
                risk_type: risk_type.to_string(),
                decision: "rejected".to_string(),
                reason,
            }),
        )
    }

    fn rejected_decision(&self, events: Vec<EngineEvent>) -> AlgorithmDecision {
        AlgorithmDecision {
            order_number: self.orders + 1,
            order_id: format!(
                "{}-order-rejected-{}",
                self.settings.run_id,
                self.orders + 1
            ),
            fill_id: format!("{}-fill-rejected-{}", self.settings.run_id, self.orders + 1),
            order: None,
            events,
        }
    }

    fn publish_events(&self, events: &[EngineEvent]) {
        let Some(event_bus) = &self.event_bus else {
            return;
        };
        for event in events {
            let Ok(envelope) = runtime_envelope(
                self.settings.run_id.clone(),
                event.category.clone(),
                &event.payload,
            ) else {
                continue;
            };
            // best-effort: runtime observers may lag or disconnect.
            let _ = event_bus.publish(envelope);
        }
    }

    fn margin_used_with_prices(&self) -> anyhow::Result<Decimal> {
        self.account_book
            .positions()
            .into_iter()
            .filter(|position| position.qty != Decimal::ZERO)
            .try_fold(Decimal::ZERO, |total, position| {
                let price = self
                    .last_prices
                    .get(&position.symbol)
                    .copied()
                    .unwrap_or(Decimal::ZERO);
                let rules = MarketRuleSet::for_symbol(&position.symbol)?;
                Ok(total + position_margin(&rules, position.qty, price))
            })
    }
}

fn position_margin(rules: &MarketRuleSet, qty: Decimal, reference_price: Decimal) -> Decimal {
    qty.abs() * reference_price * rules.initial_margin_rate
}

fn order_payload(
    run_id: &str,
    order: &OrderRequest,
    filled_qty: Decimal,
    status: &str,
    broker_order_id: Option<&str>,
) -> serde_json::Value {
    payload_value(AlgorithmOrderPayload {
        run_id: run_id.to_string(),
        broker_order_id: broker_order_id.map(str::to_string),
        account_id: order.account_id.clone(),
        symbol: order.symbol.clone(),
        side: format!("{:?}", order.side).to_uppercase(),
        order_type: format!("{:?}", order.order_type).to_uppercase(),
        qty: order.qty.to_string(),
        filled_qty: filled_qty.to_string(),
        status: status.to_string(),
    })
}

fn payload_value(payload: impl Serialize) -> serde_json::Value {
    match serde_json::to_value(payload) {
        Ok(value) => value,
        Err(_) => serde_json::Value::Object(Default::default()),
    }
}

fn parse_hhmm_to_minutes(value: &str) -> anyhow::Result<u32> {
    let (hour, minute) = value
        .split_once(':')
        .ok_or_else(|| anyhow::anyhow!("invalid HH:MM time {value}"))?;
    let hour = hour.parse::<u32>()?;
    let minute = minute.parse::<u32>()?;
    if hour > 23 || minute > 59 {
        anyhow::bail!("invalid HH:MM time {value}");
    }
    Ok(hour * 60 + minute)
}

fn timezone_offset_for_timestamp(timezone: &str, ts_ms: i64) -> anyhow::Result<FixedOffset> {
    match timezone {
        "UTC" => Ok(Utc.fix()),
        "America/New_York" => eastern_offset_for_timestamp(ts_ms),
        other => parse_fixed_offset(other),
    }
}

fn parse_fixed_offset(value: &str) -> anyhow::Result<FixedOffset> {
    if value.len() != 6 || (value.as_bytes()[0] != b'+' && value.as_bytes()[0] != b'-') {
        anyhow::bail!("unsupported timezone {value}");
    }
    let sign = if value.starts_with('-') { -1 } else { 1 };
    let hour = value[1..3].parse::<i32>()?;
    let minute = value[4..6].parse::<i32>()?;
    let seconds = sign * (hour * 3600 + minute * 60);
    FixedOffset::east_opt(seconds)
        .ok_or_else(|| anyhow::anyhow!("invalid fixed offset timezone {value}"))
}

fn eastern_offset_for_timestamp(ts_ms: i64) -> anyhow::Result<FixedOffset> {
    let utc = Utc
        .timestamp_millis_opt(ts_ms)
        .single()
        .ok_or_else(|| anyhow::anyhow!("invalid timestamp {ts_ms}"))?;
    let year = utc.year();
    let dst_start_day = nth_weekday_of_month(year, 3, Weekday::Sun, 2);
    let dst_end_day = nth_weekday_of_month(year, 11, Weekday::Sun, 1);
    let dst_start_utc = Utc
        .with_ymd_and_hms(year, 3, dst_start_day, 7, 0, 0)
        .single()
        .ok_or_else(|| anyhow::anyhow!("invalid DST start for {year}"))?;
    let dst_end_utc = Utc
        .with_ymd_and_hms(year, 11, dst_end_day, 6, 0, 0)
        .single()
        .ok_or_else(|| anyhow::anyhow!("invalid DST end for {year}"))?;
    let offset_seconds = if utc >= dst_start_utc && utc < dst_end_utc {
        -4 * 3600
    } else {
        -5 * 3600
    };
    FixedOffset::east_opt(offset_seconds)
        .ok_or_else(|| anyhow::anyhow!("invalid eastern offset for {ts_ms}"))
}

fn nth_weekday_of_month(year: i32, month: u32, weekday: Weekday, nth: u32) -> u32 {
    let mut day = 1_u32;
    let mut seen = 0_u32;
    loop {
        let date = chrono::NaiveDate::from_ymd_opt(year, month, day)
            .expect("valid calendar day while computing nth weekday");
        if date.weekday() == weekday {
            seen += 1;
            if seen == nth {
                return day;
            }
        }
        day += 1;
    }
}
