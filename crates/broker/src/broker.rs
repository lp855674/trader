#![forbid(unsafe_code)]

pub mod binance;
pub mod ibkr;
pub mod reconciliation_gate;

pub use binance::{
    BinanceAssetBalance, BinanceHttpClient, BinanceKlineBar, BinanceLimitOrderRequest,
    BinanceOpenOrder, BinanceOrderAck, BinanceOrderSide, BinanceSignedRequest,
    BinanceSpotTestnetAdapter, BinanceSpotTestnetSettings, BinanceTrade, ReqwestBinanceHttpClient,
};

pub use ibkr::{
    IbapiIbkrGatewayClient, IbkrExecution, IbkrGatewayClient, IbkrLimitOrderRequest,
    IbkrMarketDataSnapshot, IbkrOpenOrder, IbkrOrderAck, IbkrOrderDiagnosticEvent,
    IbkrOrderDiagnosticReport, IbkrOrderSide, IbkrOrderStatus, IbkrPaperGatewayAdapter,
    IbkrPaperGatewaySettings, IbkrServerVersion, IbkrTrade,
};

pub use reconciliation_gate::{
    ReconciliationGateAudit, ReconciliationGateDecision, ReconciliationGateFailure,
    ReconciliationGateInput, ReconciliationGateRequirement, ReconciliationGateStatus,
    evaluate_reconciliation_gate,
};

use async_trait::async_trait;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};
use thiserror::Error;
use tokio::sync::Mutex;
use trader_core::{OrderRequest, OrderSide, OrderType};
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum BrokerError {
    #[error("broker rejected order: {0}")]
    Rejected(String),
    #[error("broker order not found: {0}")]
    OrderNotFound(String),
    #[error("broker configuration error: {0}")]
    Config(String),
    #[error("broker connection error: {0}")]
    Connection(String),
    #[error("broker http error: {0}")]
    Http(#[from] reqwest::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceOrderResponse {
    pub broker_order_id: String,
    pub accepted: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BrokerOrderStatus {
    Accepted,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BrokerOrder {
    pub broker_order_id: String,
    pub account_id: String,
    pub symbol: String,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub qty: Decimal,
    pub price: Option<Decimal>,
    pub status: BrokerOrderStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BrokerOpenOrder {
    pub broker_order_id: String,
    pub client_order_id: String,
    pub account_id: String,
    pub symbol: String,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub price: Option<Decimal>,
    pub qty: Decimal,
    pub filled_qty: Decimal,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BrokerExecution {
    pub trade_id: String,
    pub broker_order_id: String,
    pub client_order_id: Option<String>,
    pub account_id: String,
    pub symbol: String,
    pub side: OrderSide,
    pub price: Decimal,
    pub qty: Decimal,
    pub fee: Decimal,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CancelledOpenOrder {
    pub open_order: BrokerOpenOrder,
    pub cancelled_order: BrokerOrder,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BrokerAccountSnapshot {
    pub account_id: String,
    pub cash: Decimal,
    pub equity: Decimal,
    pub buying_power: Decimal,
    pub margin_used: Decimal,
    pub cash_balances: Vec<BrokerCashBalance>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BrokerSnapshotBundle {
    pub account: BrokerAccountSnapshot,
    pub positions: Vec<BrokerPositionSnapshot>,
    pub open_orders: Vec<BrokerOpenOrder>,
    pub executions: Vec<BrokerExecution>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecoveryOrderKey {
    pub account_id: String,
    pub client_order_id: String,
    pub broker_order_id: Option<String>,
}

pub fn broker_open_order_matches_recovery_order(
    open_order: &BrokerOpenOrder,
    recovery_order: &RecoveryOrderKey,
) -> bool {
    open_order.account_id == recovery_order.account_id
        && (non_empty_id_eq(&open_order.client_order_id, &recovery_order.client_order_id)
            || recovery_order
                .broker_order_id
                .as_deref()
                .is_some_and(|broker_order_id| {
                    non_empty_id_eq(broker_order_id, &open_order.broker_order_id)
                }))
}

pub fn broker_execution_matches_recovery_order(
    execution: &BrokerExecution,
    recovery_order: &RecoveryOrderKey,
) -> bool {
    execution.account_id == recovery_order.account_id
        && (execution
            .client_order_id
            .as_deref()
            .is_some_and(|client_order_id| {
                non_empty_id_eq(client_order_id, &recovery_order.client_order_id)
            })
            || recovery_order
                .broker_order_id
                .as_deref()
                .is_some_and(|broker_order_id| {
                    non_empty_id_eq(broker_order_id, &execution.broker_order_id)
                }))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BrokerCashBalance {
    pub account_id: String,
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
pub struct RuntimeCashBalance {
    pub account_id: String,
    pub currency: String,
    pub cash: Decimal,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct BrokerContractMetadata {
    pub conid: Option<i64>,
    pub sec_type: Option<String>,
    pub currency: Option<String>,
    pub exchange: Option<String>,
    pub primary_exchange: Option<String>,
    pub multiplier: Option<Decimal>,
    pub expiry: Option<String>,
    pub right: Option<String>,
    pub strike: Option<Decimal>,
    pub local_symbol: Option<String>,
    pub trading_class: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BrokerPositionSide {
    Long,
    Short,
}

impl BrokerPositionSide {
    pub fn from_signed_qty(qty: Decimal) -> Option<Self> {
        if qty > Decimal::ZERO {
            Some(Self::Long)
        } else if qty < Decimal::ZERO {
            Some(Self::Short)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimePositionSnapshot {
    pub account_id: String,
    pub exchange: String,
    pub symbol: String,
    pub position_side: BrokerPositionSide,
    pub qty: Decimal,
    pub avg_price: Decimal,
    pub margin_used: Decimal,
    pub contract: Option<BrokerContractMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeOpenOrder {
    pub account_id: String,
    pub symbol: String,
    pub order_id: String,
    pub client_order_id: String,
    pub broker_order_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeExecution {
    pub fill_id: String,
    pub order_id: String,
    pub account_id: Option<String>,
    pub symbol: Option<String>,
    pub client_order_id: Option<String>,
    pub broker_order_id: Option<String>,
    pub side: Option<OrderSide>,
    pub price: Option<Decimal>,
    pub qty: Option<Decimal>,
    pub fee: Option<Decimal>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BrokerPositionSnapshot {
    pub account_id: String,
    pub exchange: String,
    pub symbol: String,
    pub position_side: BrokerPositionSide,
    pub qty: Decimal,
    pub avg_price: Decimal,
    pub mark_price: Option<Decimal>,
    pub margin_used: Decimal,
    pub unrealized_pnl: Decimal,
    pub ts_ms: i64,
    pub contract: Option<BrokerContractMetadata>,
    pub liquidation_price: Option<Decimal>,
    pub open_interest: Option<Decimal>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PositionReconciliationDrift {
    pub account_id: String,
    pub exchange: String,
    pub symbol: String,
    pub position_side: BrokerPositionSide,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct PositionReconciliationReport {
    pub drifts: Vec<PositionReconciliationDrift>,
}

impl PositionReconciliationReport {
    pub fn drift_count(&self) -> usize {
        self.drifts.len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BrokerReconciliationSeverity {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BrokerReconciliationThresholds {
    pub cash_abs: Decimal,
    pub position_qty_abs: Decimal,
    pub stale_after_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BrokerReconciliationDrift {
    pub account_id: String,
    pub reason: String,
    pub symbol: Option<String>,
    pub position_side: Option<BrokerPositionSide>,
    pub currency: Option<String>,
    pub local_value: Option<String>,
    pub broker_value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BrokerReconciliationAudit {
    pub account_id: String,
    pub broker_kind: BrokerKind,
    pub ts_ms: i64,
    pub severity: BrokerReconciliationSeverity,
    pub cash_drifts: Vec<BrokerReconciliationDrift>,
    pub position_drifts: Vec<BrokerReconciliationDrift>,
    pub open_order_drifts: Vec<BrokerReconciliationDrift>,
    pub execution_drifts: Vec<BrokerReconciliationDrift>,
    pub stale_inputs: Vec<BrokerReconciliationDrift>,
}

pub struct BrokerReconciliationInput {
    pub account_id: String,
    pub broker_kind: BrokerKind,
    pub ts_ms: i64,
    pub thresholds: BrokerReconciliationThresholds,
    pub runtime_cash: Vec<RuntimeCashBalance>,
    pub broker_cash: Vec<BrokerCashBalance>,
    pub runtime_positions: Vec<RuntimePositionSnapshot>,
    pub broker_positions: Vec<BrokerPositionSnapshot>,
    pub runtime_open_orders: Vec<RuntimeOpenOrder>,
    pub broker_open_orders: Vec<BrokerOpenOrder>,
    pub runtime_executions: Vec<RuntimeExecution>,
    pub broker_executions: Vec<BrokerExecution>,
}

pub fn reconcile_broker_audit(input: BrokerReconciliationInput) -> BrokerReconciliationAudit {
    let mut audit = BrokerReconciliationAudit {
        account_id: input.account_id.clone(),
        broker_kind: input.broker_kind,
        ts_ms: input.ts_ms,
        severity: BrokerReconciliationSeverity::Info,
        cash_drifts: Vec::new(),
        position_drifts: Vec::new(),
        open_order_drifts: Vec::new(),
        execution_drifts: Vec::new(),
        stale_inputs: Vec::new(),
    };

    for broker_cash in &input.broker_cash {
        if input.ts_ms - broker_cash.source_ts_ms > input.thresholds.stale_after_ms {
            audit.stale_inputs.push(BrokerReconciliationDrift {
                account_id: broker_cash.account_id.clone(),
                reason: "broker_cash_stale".to_string(),
                symbol: None,
                position_side: None,
                currency: Some(broker_cash.currency.clone()),
                local_value: None,
                broker_value: Some(broker_cash.source_ts_ms.to_string()),
            });
        }

        match input.runtime_cash.iter().find(|runtime_cash| {
            runtime_cash.account_id == broker_cash.account_id
                && runtime_cash.currency == broker_cash.currency
        }) {
            Some(runtime_cash) => {
                let drift = (runtime_cash.cash - broker_cash.cash).abs();
                if drift > input.thresholds.cash_abs {
                    audit.cash_drifts.push(BrokerReconciliationDrift {
                        account_id: broker_cash.account_id.clone(),
                        reason: "cash_total_drift".to_string(),
                        symbol: None,
                        position_side: None,
                        currency: Some(broker_cash.currency.clone()),
                        local_value: Some(runtime_cash.cash.to_string()),
                        broker_value: Some(broker_cash.cash.to_string()),
                    });
                }
            }
            None => audit.cash_drifts.push(BrokerReconciliationDrift {
                account_id: broker_cash.account_id.clone(),
                reason: "cash_missing_runtime".to_string(),
                symbol: None,
                position_side: None,
                currency: Some(broker_cash.currency.clone()),
                local_value: None,
                broker_value: Some(broker_cash.cash.to_string()),
            }),
        }
    }

    for runtime_cash in &input.runtime_cash {
        if input.broker_cash.iter().any(|broker_cash| {
            broker_cash.account_id == runtime_cash.account_id
                && broker_cash.currency == runtime_cash.currency
        }) {
            continue;
        }
        audit.cash_drifts.push(BrokerReconciliationDrift {
            account_id: runtime_cash.account_id.clone(),
            reason: "cash_missing_broker".to_string(),
            symbol: None,
            position_side: None,
            currency: Some(runtime_cash.currency.clone()),
            local_value: Some(runtime_cash.cash.to_string()),
            broker_value: None,
        });
    }

    for position in &input.broker_positions {
        match input
            .runtime_positions
            .iter()
            .find(|runtime_position| broker_position_matches_runtime(runtime_position, position))
        {
            Some(runtime_position) => {
                let drift = (runtime_position.qty - position.qty).abs();
                if drift > input.thresholds.position_qty_abs {
                    audit.position_drifts.push(BrokerReconciliationDrift {
                        account_id: position.account_id.clone(),
                        reason: "position_qty_drift".to_string(),
                        symbol: Some(position.symbol.clone()),
                        position_side: Some(position.position_side),
                        currency: broker_position_currency(Some(runtime_position), position),
                        local_value: Some(runtime_position.qty.to_string()),
                        broker_value: Some(position.qty.to_string()),
                    });
                }
            }
            None => audit.position_drifts.push(BrokerReconciliationDrift {
                account_id: position.account_id.clone(),
                reason: "position_missing_runtime".to_string(),
                symbol: Some(position.symbol.clone()),
                position_side: Some(position.position_side),
                currency: position
                    .contract
                    .as_ref()
                    .and_then(|contract| contract.currency.clone()),
                local_value: None,
                broker_value: Some(position.qty.to_string()),
            }),
        }
    }

    for runtime_position in &input.runtime_positions {
        if input.broker_positions.iter().any(|broker_position| {
            broker_position_matches_runtime(runtime_position, broker_position)
        }) {
            continue;
        }
        audit.position_drifts.push(BrokerReconciliationDrift {
            account_id: runtime_position.account_id.clone(),
            reason: "position_missing_broker".to_string(),
            symbol: Some(runtime_position.symbol.clone()),
            position_side: Some(runtime_position.position_side),
            currency: runtime_position_currency(runtime_position),
            local_value: Some(runtime_position.qty.to_string()),
            broker_value: None,
        });
    }

    for open_order in &input.broker_open_orders {
        if input
            .runtime_open_orders
            .iter()
            .any(|runtime_order| open_order_matches_runtime(runtime_order, open_order))
        {
            continue;
        }
        audit.open_order_drifts.push(BrokerReconciliationDrift {
            account_id: open_order.account_id.clone(),
            reason: "open_order_missing_runtime".to_string(),
            symbol: Some(open_order.symbol.clone()),
            position_side: None,
            currency: None,
            local_value: None,
            broker_value: Some(open_order.broker_order_id.clone()),
        });
    }

    for runtime_order in &input.runtime_open_orders {
        if input
            .broker_open_orders
            .iter()
            .any(|broker_order| open_order_matches_runtime(runtime_order, broker_order))
        {
            continue;
        }
        audit.open_order_drifts.push(BrokerReconciliationDrift {
            account_id: runtime_order.account_id.clone(),
            reason: "open_order_missing_broker".to_string(),
            symbol: Some(runtime_order.symbol.clone()),
            position_side: None,
            currency: None,
            local_value: Some(
                runtime_order
                    .broker_order_id
                    .as_deref()
                    .unwrap_or(&runtime_order.order_id)
                    .to_string(),
            ),
            broker_value: None,
        });
    }

    for execution in &input.broker_executions {
        let runtime_execution = input.runtime_executions.iter().find(|runtime_execution| {
            execution_identity_matches_runtime(runtime_execution, execution)
        });
        match runtime_execution {
            Some(runtime_execution) => {
                if let Some(drift) = execution_field_drift(runtime_execution, execution) {
                    audit.execution_drifts.push(drift);
                }
            }
            None => audit.execution_drifts.push(BrokerReconciliationDrift {
                account_id: execution.account_id.clone(),
                reason: "execution_missing_runtime".to_string(),
                symbol: Some(execution.symbol.clone()),
                position_side: None,
                currency: None,
                local_value: None,
                broker_value: Some(execution.trade_id.clone()),
            }),
        }
    }

    audit.severity = if audit.cash_drifts.is_empty()
        && audit.position_drifts.is_empty()
        && audit.open_order_drifts.is_empty()
        && audit.execution_drifts.is_empty()
    {
        if audit.stale_inputs.is_empty() {
            BrokerReconciliationSeverity::Info
        } else {
            BrokerReconciliationSeverity::Warn
        }
    } else {
        BrokerReconciliationSeverity::Error
    };
    audit
}

fn runtime_position_currency(runtime_position: &RuntimePositionSnapshot) -> Option<String> {
    runtime_position
        .contract
        .as_ref()
        .and_then(|contract| contract.currency.clone())
}

fn broker_position_currency(
    runtime_position: Option<&RuntimePositionSnapshot>,
    broker_position: &BrokerPositionSnapshot,
) -> Option<String> {
    broker_position
        .contract
        .as_ref()
        .and_then(|contract| contract.currency.clone())
        .or_else(|| runtime_position.and_then(runtime_position_currency))
}

fn broker_position_matches_runtime(
    runtime_position: &RuntimePositionSnapshot,
    broker_position: &BrokerPositionSnapshot,
) -> bool {
    if runtime_position.account_id != broker_position.account_id
        || runtime_position.position_side != broker_position.position_side
    {
        return false;
    }

    if let Some((runtime_conid, broker_conid)) = runtime_position
        .contract
        .as_ref()
        .and_then(|contract| contract.conid)
        .zip(
            broker_position
                .contract
                .as_ref()
                .and_then(|contract| contract.conid),
        )
    {
        return runtime_conid == broker_conid;
    }

    runtime_position.account_id == broker_position.account_id
        && runtime_position.symbol == broker_position.symbol
        && runtime_position.position_side == broker_position.position_side
}

fn open_order_matches_runtime(
    runtime_order: &RuntimeOpenOrder,
    broker_order: &BrokerOpenOrder,
) -> bool {
    runtime_order.account_id == broker_order.account_id
        && (non_empty_id_eq(
            &runtime_order.client_order_id,
            &broker_order.client_order_id,
        ) || runtime_order
            .broker_order_id
            .as_deref()
            .is_some_and(|broker_order_id| {
                non_empty_id_eq(broker_order_id, &broker_order.broker_order_id)
            }))
}

fn non_empty_id_eq(left: &str, right: &str) -> bool {
    !left.trim().is_empty() && !right.trim().is_empty() && left == right
}

fn execution_identity_matches_runtime(
    runtime_execution: &RuntimeExecution,
    broker_execution: &BrokerExecution,
) -> bool {
    execution_scope_matches(runtime_execution, broker_execution)
        && (non_empty_id_eq(&runtime_execution.fill_id, &broker_execution.trade_id)
            || non_empty_id_eq(
                &runtime_execution.order_id,
                &broker_execution.broker_order_id,
            )
            || runtime_execution
                .broker_order_id
                .as_deref()
                .is_some_and(|broker_order_id| {
                    non_empty_id_eq(broker_order_id, &broker_execution.broker_order_id)
                })
            || runtime_execution
                .client_order_id
                .as_deref()
                .zip(broker_execution.client_order_id.as_deref())
                .is_some_and(|(runtime_client_order_id, broker_client_order_id)| {
                    non_empty_id_eq(runtime_client_order_id, broker_client_order_id)
                }))
}

fn execution_field_drift(
    runtime_execution: &RuntimeExecution,
    broker_execution: &BrokerExecution,
) -> Option<BrokerReconciliationDrift> {
    let mut field_drifts = Vec::new();
    push_execution_field_drift(
        &mut field_drifts,
        "side",
        runtime_execution.side.map(|side| format!("{side:?}")),
        Some(format!("{:?}", broker_execution.side)),
    );
    push_execution_field_drift(
        &mut field_drifts,
        "price",
        runtime_execution.price.map(|value| value.to_string()),
        Some(broker_execution.price.to_string()),
    );
    push_execution_field_drift(
        &mut field_drifts,
        "qty",
        runtime_execution.qty.map(|value| value.to_string()),
        Some(broker_execution.qty.to_string()),
    );
    push_execution_field_drift(
        &mut field_drifts,
        "fee",
        runtime_execution.fee.map(|value| value.to_string()),
        Some(broker_execution.fee.to_string()),
    );
    push_execution_field_drift(
        &mut field_drifts,
        "account_id",
        runtime_execution.account_id.clone(),
        Some(broker_execution.account_id.clone()),
    );
    push_execution_field_drift(
        &mut field_drifts,
        "symbol",
        runtime_execution.symbol.clone(),
        Some(broker_execution.symbol.clone()),
    );
    push_execution_field_drift(
        &mut field_drifts,
        "client_order_id",
        runtime_execution.client_order_id.clone(),
        broker_execution.client_order_id.clone(),
    );

    if field_drifts.is_empty() {
        return None;
    }

    Some(BrokerReconciliationDrift {
        account_id: broker_execution.account_id.clone(),
        reason: "execution_field_drift".to_string(),
        symbol: Some(broker_execution.symbol.clone()),
        position_side: None,
        currency: None,
        local_value: Some(field_drifts.join(",")),
        broker_value: Some(broker_execution.trade_id.clone()),
    })
}

fn push_execution_field_drift(
    field_drifts: &mut Vec<String>,
    field: &str,
    local_value: Option<String>,
    broker_value: Option<String>,
) {
    if local_value == broker_value {
        return;
    }
    field_drifts.push(format!(
        "{field}:local={}:broker={}",
        local_value.as_deref().unwrap_or("<missing>"),
        broker_value.as_deref().unwrap_or("<missing>")
    ));
}

fn execution_scope_matches(
    runtime_execution: &RuntimeExecution,
    broker_execution: &BrokerExecution,
) -> bool {
    runtime_execution
        .account_id
        .as_deref()
        .is_none_or(|account_id| account_id == broker_execution.account_id)
        && runtime_execution
            .symbol
            .as_deref()
            .is_none_or(|symbol| symbol == broker_execution.symbol)
}

pub fn reconcile_positions(
    runtime: &[RuntimePositionSnapshot],
    broker: &[BrokerPositionSnapshot],
) -> PositionReconciliationReport {
    let mut report = PositionReconciliationReport::default();

    for broker_position in broker {
        let runtime_position = runtime.iter().find(|runtime_position| {
            broker_position_matches_runtime(runtime_position, broker_position)
        });
        let Some(runtime_position) = runtime_position else {
            report.drifts.push(PositionReconciliationDrift {
                account_id: broker_position.account_id.clone(),
                exchange: broker_position.exchange.clone(),
                symbol: broker_position.symbol.clone(),
                position_side: broker_position.position_side,
                reason: "missing runtime position".to_string(),
            });
            continue;
        };

        if runtime_position.qty != broker_position.qty {
            report.drifts.push(PositionReconciliationDrift {
                account_id: broker_position.account_id.clone(),
                exchange: broker_position.exchange.clone(),
                symbol: broker_position.symbol.clone(),
                position_side: broker_position.position_side,
                reason: format!(
                    "qty mismatch runtime={} broker={}",
                    runtime_position.qty, broker_position.qty
                ),
            });
        }
        if runtime_position.avg_price != broker_position.avg_price {
            report.drifts.push(PositionReconciliationDrift {
                account_id: broker_position.account_id.clone(),
                exchange: broker_position.exchange.clone(),
                symbol: broker_position.symbol.clone(),
                position_side: broker_position.position_side,
                reason: format!(
                    "avg_price mismatch runtime={} broker={}",
                    runtime_position.avg_price, broker_position.avg_price
                ),
            });
        }
        if runtime_position.margin_used != broker_position.margin_used {
            report.drifts.push(PositionReconciliationDrift {
                account_id: broker_position.account_id.clone(),
                exchange: broker_position.exchange.clone(),
                symbol: broker_position.symbol.clone(),
                position_side: broker_position.position_side,
                reason: format!(
                    "margin mismatch runtime={} broker={}",
                    runtime_position.margin_used, broker_position.margin_used
                ),
            });
        }
    }

    for runtime_position in runtime {
        if broker.iter().any(|broker_position| {
            broker_position_matches_runtime(runtime_position, broker_position)
        }) {
            continue;
        }
        report.drifts.push(PositionReconciliationDrift {
            account_id: runtime_position.account_id.clone(),
            exchange: runtime_position.exchange.clone(),
            symbol: runtime_position.symbol.clone(),
            position_side: runtime_position.position_side,
            reason: "missing broker position".to_string(),
        });
    }

    report
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BrokerKind {
    Simulated,
    Futu,
    Binance,
    Okx,
    InteractiveBrokers,
}

impl BrokerKind {
    fn slug(self) -> &'static str {
        match self {
            Self::Simulated => "simulated",
            Self::Futu => "futu",
            Self::Binance => "binance",
            Self::Okx => "okx",
            Self::InteractiveBrokers => "ib",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BrokerCapabilities {
    pub market_data: bool,
    pub order_submit: bool,
    pub order_cancel: bool,
    pub paper_trading: bool,
    pub live_trading: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BrokerStatus {
    pub kind: BrokerKind,
    pub connected: bool,
    pub trading_enabled: bool,
    pub capabilities: BrokerCapabilities,
}

#[derive(Debug, Clone)]
pub struct SimulatedBrokerSettings {
    pub slippage_bps: Decimal,
    pub fee_bps: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulatedFill {
    pub price: Decimal,
    pub qty: Decimal,
    pub fee: Decimal,
}

#[async_trait]
pub trait Broker: Send + Sync {
    async fn place_order(&self, request: OrderRequest) -> Result<PlaceOrderResponse, BrokerError>;
    async fn cancel_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError>;
    async fn query_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError>;
    async fn account_snapshot(
        &self,
        account_id: &str,
    ) -> Result<BrokerAccountSnapshot, BrokerError>;
    async fn position_snapshots(
        &self,
        account_id: &str,
    ) -> Result<Vec<BrokerPositionSnapshot>, BrokerError>;
    async fn snapshot_bundle(
        &self,
        account_id: &str,
        execution_symbols: &[String],
    ) -> Result<BrokerSnapshotBundle, BrokerError> {
        let account = self.account_snapshot(account_id).await?;
        let positions = self.position_snapshots(account_id).await?;
        let open_orders = self.open_orders(account_id).await?;
        let mut symbols = execution_symbols.iter().cloned().collect::<BTreeSet<_>>();
        symbols.extend(positions.iter().map(|position| position.symbol.clone()));
        symbols.extend(open_orders.iter().map(|order| order.symbol.clone()));
        let mut executions = Vec::new();
        for symbol in symbols {
            executions.extend(self.executions(account_id, Some(&symbol)).await?);
        }
        Ok(BrokerSnapshotBundle {
            account,
            positions,
            open_orders,
            executions,
        })
    }
    async fn open_orders(&self, _account_id: &str) -> Result<Vec<BrokerOpenOrder>, BrokerError> {
        Ok(Vec::new())
    }
    async fn executions(
        &self,
        _account_id: &str,
        _symbol: Option<&str>,
    ) -> Result<Vec<BrokerExecution>, BrokerError> {
        Ok(Vec::new())
    }
    async fn status(&self) -> Result<BrokerStatus, BrokerError>;
}

pub async fn cancel_open_orders_for_account_symbol(
    broker: &dyn Broker,
    account_id: &str,
    symbol: Option<&str>,
) -> Result<Vec<CancelledOpenOrder>, BrokerError> {
    let open_orders = broker.open_orders(account_id).await?;
    let mut cancelled = Vec::new();
    for open_order in open_orders {
        if symbol.is_some_and(|symbol| symbol != open_order.symbol) {
            continue;
        }
        let cancelled_order = broker.cancel_order(&open_order.broker_order_id).await?;
        cancelled.push(CancelledOpenOrder {
            open_order,
            cancelled_order,
        });
    }
    Ok(cancelled)
}

#[derive(Default)]
pub struct MockBroker;

#[async_trait]
impl Broker for MockBroker {
    async fn place_order(&self, request: OrderRequest) -> Result<PlaceOrderResponse, BrokerError> {
        if request.qty <= rust_decimal::Decimal::ZERO {
            return Err(BrokerError::Rejected("qty must be positive".to_string()));
        }
        Ok(PlaceOrderResponse {
            broker_order_id: Uuid::new_v4().to_string(),
            accepted: true,
            reason: None,
        })
    }

    async fn status(&self) -> Result<BrokerStatus, BrokerError> {
        Ok(fake_status(BrokerKind::Simulated))
    }

    async fn cancel_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError> {
        Err(BrokerError::OrderNotFound(broker_order_id.to_string()))
    }

    async fn query_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError> {
        Err(BrokerError::OrderNotFound(broker_order_id.to_string()))
    }

    async fn account_snapshot(
        &self,
        account_id: &str,
    ) -> Result<BrokerAccountSnapshot, BrokerError> {
        Ok(fake_account_snapshot(account_id))
    }

    async fn position_snapshots(
        &self,
        account_id: &str,
    ) -> Result<Vec<BrokerPositionSnapshot>, BrokerError> {
        Ok(fake_position_snapshots(account_id, BrokerKind::Simulated))
    }
}

#[derive(Debug, Clone)]
pub struct FakeBrokerAdapter {
    kind: BrokerKind,
    orders: Arc<Mutex<HashMap<String, BrokerOrder>>>,
    startup_unmatched_open_order: bool,
}

impl FakeBrokerAdapter {
    pub fn new(kind: BrokerKind) -> Self {
        Self {
            kind,
            orders: Arc::new(Mutex::new(HashMap::new())),
            startup_unmatched_open_order: false,
        }
    }

    pub fn with_startup_unmatched_open_order(mut self, enabled: bool) -> Self {
        self.startup_unmatched_open_order = enabled;
        self
    }

    pub fn futu() -> Self {
        Self::new(BrokerKind::Futu)
    }

    pub fn binance() -> Self {
        Self::new(BrokerKind::Binance)
    }

    pub fn okx() -> Self {
        Self::new(BrokerKind::Okx)
    }

    pub fn interactive_brokers() -> Self {
        Self::new(BrokerKind::InteractiveBrokers)
    }
}

#[async_trait]
impl Broker for FakeBrokerAdapter {
    async fn place_order(&self, request: OrderRequest) -> Result<PlaceOrderResponse, BrokerError> {
        if request.qty <= Decimal::ZERO {
            return Err(BrokerError::Rejected("qty must be positive".to_string()));
        }
        let broker_order_id = format!("fake-{}-{}", self.kind.slug(), Uuid::new_v4());
        let order = BrokerOrder {
            broker_order_id: broker_order_id.clone(),
            account_id: request.account_id,
            symbol: request.symbol,
            side: request.side,
            order_type: request.order_type,
            qty: request.qty,
            price: request.price,
            status: BrokerOrderStatus::Accepted,
        };
        self.orders
            .lock()
            .await
            .insert(broker_order_id.clone(), order);
        Ok(PlaceOrderResponse {
            broker_order_id,
            accepted: true,
            reason: None,
        })
    }

    async fn cancel_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError> {
        let mut orders = self.orders.lock().await;
        let order = orders
            .get_mut(broker_order_id)
            .ok_or_else(|| BrokerError::OrderNotFound(broker_order_id.to_string()))?;
        order.status = BrokerOrderStatus::Cancelled;
        Ok(order.clone())
    }

    async fn query_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError> {
        self.orders
            .lock()
            .await
            .get(broker_order_id)
            .cloned()
            .ok_or_else(|| BrokerError::OrderNotFound(broker_order_id.to_string()))
    }

    async fn account_snapshot(
        &self,
        account_id: &str,
    ) -> Result<BrokerAccountSnapshot, BrokerError> {
        Ok(fake_account_snapshot(account_id))
    }

    async fn position_snapshots(
        &self,
        account_id: &str,
    ) -> Result<Vec<BrokerPositionSnapshot>, BrokerError> {
        Ok(fake_position_snapshots(account_id, self.kind))
    }

    async fn open_orders(&self, account_id: &str) -> Result<Vec<BrokerOpenOrder>, BrokerError> {
        if !self.startup_unmatched_open_order {
            return Ok(Vec::new());
        }
        Ok(vec![BrokerOpenOrder {
            broker_order_id: "fake-startup-unmatched-open-order".to_string(),
            client_order_id: "fake-startup-unmatched-client-order".to_string(),
            account_id: account_id.to_string(),
            symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            price: Some(Decimal::from(185)),
            qty: Decimal::ONE,
            filled_qty: Decimal::ZERO,
            status: "SUBMITTED".to_string(),
        }])
    }

    async fn status(&self) -> Result<BrokerStatus, BrokerError> {
        Ok(fake_status(self.kind))
    }
}

fn fake_account_snapshot(account_id: &str) -> BrokerAccountSnapshot {
    let cash = Decimal::from(100_000);
    BrokerAccountSnapshot {
        account_id: account_id.to_string(),
        cash,
        equity: cash,
        buying_power: cash,
        margin_used: Decimal::ZERO,
        cash_balances: vec![BrokerCashBalance {
            account_id: account_id.to_string(),
            currency: "USD".to_string(),
            cash,
            available_cash: cash,
            frozen_cash: Decimal::ZERO,
            equity: Some(cash),
            buying_power: Some(cash),
            margin_used: Some(Decimal::ZERO),
            source_ts_ms: 1_700_000_000_000,
        }],
    }
}

fn fake_position_snapshots(account_id: &str, kind: BrokerKind) -> Vec<BrokerPositionSnapshot> {
    let exchange = match kind {
        BrokerKind::Binance | BrokerKind::Simulated => "BINANCE",
        BrokerKind::Futu => "FUTU",
        BrokerKind::Okx => "OKX",
        BrokerKind::InteractiveBrokers => "IBKR",
    };
    vec![BrokerPositionSnapshot {
        account_id: account_id.to_string(),
        exchange: exchange.to_string(),
        symbol: format!("CRYPTO:{exchange}:BTCUSDT_PERP:CRYPTO_PERP"),
        position_side: BrokerPositionSide::Long,
        qty: Decimal::new(5, 1),
        avg_price: Decimal::from(65_000),
        mark_price: Some(Decimal::from(65_000)),
        margin_used: Decimal::from(3_250),
        unrealized_pnl: Decimal::new(125, 1),
        ts_ms: 1_700_000_000_000,
        contract: None,
        liquidation_price: None,
        open_interest: None,
    }]
}

fn fake_status(kind: BrokerKind) -> BrokerStatus {
    BrokerStatus {
        kind,
        connected: true,
        trading_enabled: true,
        capabilities: BrokerCapabilities {
            market_data: true,
            order_submit: true,
            order_cancel: true,
            paper_trading: true,
            live_trading: false,
        },
    }
}

pub fn simulate_market_fill(
    request: OrderRequest,
    mark_price: Decimal,
    settings: SimulatedBrokerSettings,
) -> Result<SimulatedFill, BrokerError> {
    if request.order_type != OrderType::Market {
        return Err(BrokerError::Rejected(
            "only market orders can be simulated".to_string(),
        ));
    }
    if request.qty <= Decimal::ZERO {
        return Err(BrokerError::Rejected("qty must be positive".to_string()));
    }
    if mark_price <= Decimal::ZERO {
        return Err(BrokerError::Rejected(
            "mark price must be positive".to_string(),
        ));
    }

    let bps_unit = Decimal::new(10_000, 0);
    let slippage = settings.slippage_bps / bps_unit;
    let fee_rate = settings.fee_bps / bps_unit;
    let price = match request.side {
        OrderSide::Buy => mark_price * (Decimal::ONE + slippage),
        OrderSide::Sell => mark_price * (Decimal::ONE - slippage),
    };
    let notional = price * request.qty;

    Ok(SimulatedFill {
        price,
        qty: request.qty,
        fee: notional * fee_rate,
    })
}

#[cfg(test)]
mod production_reconciliation_tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn account_snapshot_exposes_multi_currency_balances() {
        let snapshot = BrokerAccountSnapshot {
            account_id: "DU123".to_string(),
            cash: dec!(1000),
            equity: dec!(1500),
            buying_power: dec!(2000),
            margin_used: dec!(100),
            cash_balances: vec![
                BrokerCashBalance {
                    account_id: "DU123".to_string(),
                    currency: "USD".to_string(),
                    cash: dec!(1000),
                    available_cash: dec!(900),
                    frozen_cash: dec!(100),
                    equity: Some(dec!(1500)),
                    buying_power: Some(dec!(2000)),
                    margin_used: Some(dec!(100)),
                    source_ts_ms: 1_700_000_000_000,
                },
                BrokerCashBalance {
                    account_id: "DU123".to_string(),
                    currency: "HKD".to_string(),
                    cash: dec!(7800),
                    available_cash: dec!(7800),
                    frozen_cash: dec!(0),
                    equity: None,
                    buying_power: None,
                    margin_used: None,
                    source_ts_ms: 1_700_000_000_000,
                },
            ],
        };

        assert_eq!(snapshot.cash_balances.len(), 2);
        assert_eq!(snapshot.cash_balances[0].currency, "USD");
        assert_eq!(snapshot.cash_balances[1].cash, dec!(7800));
    }

    #[test]
    fn reconciliation_report_detects_cash_position_order_and_execution_drift() {
        let audit = reconcile_broker_audit(BrokerReconciliationInput {
            account_id: "DU123".to_string(),
            broker_kind: BrokerKind::InteractiveBrokers,
            ts_ms: 1_700_000_000_000,
            thresholds: BrokerReconciliationThresholds {
                cash_abs: dec!(1),
                position_qty_abs: dec!(0),
                stale_after_ms: 60_000,
            },
            runtime_cash: vec![RuntimeCashBalance {
                account_id: "DU123".to_string(),
                currency: "USD".to_string(),
                cash: dec!(1000),
                ts_ms: 1_700_000_000_000,
            }],
            broker_cash: vec![BrokerCashBalance {
                account_id: "DU123".to_string(),
                currency: "USD".to_string(),
                cash: dec!(998),
                available_cash: dec!(998),
                frozen_cash: dec!(0),
                equity: None,
                buying_power: None,
                margin_used: None,
                source_ts_ms: 1_700_000_000_000,
            }],
            runtime_positions: vec![RuntimePositionSnapshot {
                account_id: "DU123".to_string(),
                exchange: "IBKR".to_string(),
                symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
                position_side: BrokerPositionSide::Long,
                qty: dec!(2),
                avg_price: dec!(180),
                margin_used: dec!(0),
                contract: None,
            }],
            broker_positions: vec![BrokerPositionSnapshot {
                account_id: "DU123".to_string(),
                exchange: "IBKR".to_string(),
                symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
                position_side: BrokerPositionSide::Long,
                qty: dec!(1),
                avg_price: dec!(180),
                mark_price: None,
                margin_used: dec!(0),
                unrealized_pnl: dec!(0),
                ts_ms: 1_700_000_000_000,
                contract: Some(BrokerContractMetadata::default()),
                liquidation_price: None,
                open_interest: None,
            }],
            runtime_open_orders: Vec::new(),
            broker_open_orders: vec![BrokerOpenOrder {
                broker_order_id: "remote-order-1".to_string(),
                client_order_id: "missing-client".to_string(),
                account_id: "DU123".to_string(),
                symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
                side: trader_core::OrderSide::Buy,
                order_type: trader_core::OrderType::Limit,
                price: Some(dec!(170)),
                qty: dec!(1),
                filled_qty: dec!(0),
                status: "Submitted".to_string(),
            }],
            runtime_executions: vec![],
            broker_executions: vec![BrokerExecution {
                trade_id: "exec-1".to_string(),
                broker_order_id: "remote-order-1".to_string(),
                client_order_id: None,
                account_id: "DU123".to_string(),
                symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
                side: trader_core::OrderSide::Buy,
                price: dec!(170),
                qty: dec!(1),
                fee: dec!(1),
                ts_ms: 1_700_000_000_000,
            }],
        });

        assert_eq!(audit.cash_drifts.len(), 1);
        assert_eq!(audit.position_drifts.len(), 1);
        assert_eq!(audit.open_order_drifts.len(), 1);
        assert_eq!(audit.execution_drifts.len(), 1);
        assert_eq!(audit.severity, BrokerReconciliationSeverity::Error);
    }

    #[test]
    fn reconciliation_report_detects_runtime_cash_and_position_missing_from_broker() {
        let audit = reconcile_broker_audit(BrokerReconciliationInput {
            account_id: "DU123".to_string(),
            broker_kind: BrokerKind::InteractiveBrokers,
            ts_ms: 1_700_000_000_000,
            thresholds: BrokerReconciliationThresholds {
                cash_abs: dec!(1),
                position_qty_abs: dec!(0),
                stale_after_ms: 60_000,
            },
            runtime_cash: vec![RuntimeCashBalance {
                account_id: "DU123".to_string(),
                currency: "USD".to_string(),
                cash: dec!(1000),
                ts_ms: 1_700_000_000_000,
            }],
            broker_cash: Vec::new(),
            runtime_positions: vec![RuntimePositionSnapshot {
                account_id: "DU123".to_string(),
                exchange: "IBKR".to_string(),
                symbol: "HK:SEHK:700:EQUITY".to_string(),
                position_side: BrokerPositionSide::Long,
                qty: dec!(2),
                avg_price: dec!(300),
                margin_used: dec!(0),
                contract: Some(BrokerContractMetadata {
                    currency: Some("HKD".to_string()),
                    primary_exchange: Some("SEHK".to_string()),
                    ..BrokerContractMetadata::default()
                }),
            }],
            broker_positions: Vec::new(),
            runtime_open_orders: Vec::new(),
            broker_open_orders: Vec::new(),
            runtime_executions: Vec::new(),
            broker_executions: Vec::new(),
        });

        assert_eq!(audit.cash_drifts.len(), 1);
        assert_eq!(audit.cash_drifts[0].reason, "cash_missing_broker");
        assert_eq!(audit.cash_drifts[0].local_value.as_deref(), Some("1000"));
        assert_eq!(audit.position_drifts.len(), 1);
        assert_eq!(audit.position_drifts[0].reason, "position_missing_broker");
        assert_eq!(
            audit.position_drifts[0].position_side,
            Some(BrokerPositionSide::Long)
        );
        assert_eq!(audit.position_drifts[0].currency.as_deref(), Some("HKD"));
        assert_eq!(audit.position_drifts[0].local_value.as_deref(), Some("2"));
        assert_eq!(audit.severity, BrokerReconciliationSeverity::Error);
    }

    #[test]
    fn reconciliation_matches_positions_by_canonical_symbol_not_broker_source_exchange() {
        let audit = reconcile_broker_audit(BrokerReconciliationInput {
            account_id: "DU123".to_string(),
            broker_kind: BrokerKind::InteractiveBrokers,
            ts_ms: 1_700_000_000_000,
            thresholds: BrokerReconciliationThresholds {
                cash_abs: dec!(1),
                position_qty_abs: dec!(0),
                stale_after_ms: 60_000,
            },
            runtime_cash: Vec::new(),
            broker_cash: Vec::new(),
            runtime_positions: vec![RuntimePositionSnapshot {
                account_id: "DU123".to_string(),
                exchange: "NASDAQ".to_string(),
                symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
                position_side: BrokerPositionSide::Long,
                qty: dec!(2),
                avg_price: dec!(180),
                margin_used: dec!(0),
                contract: None,
            }],
            broker_positions: vec![BrokerPositionSnapshot {
                account_id: "DU123".to_string(),
                exchange: "IBKR".to_string(),
                symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
                position_side: BrokerPositionSide::Long,
                qty: dec!(1),
                avg_price: dec!(180),
                mark_price: None,
                margin_used: dec!(0),
                unrealized_pnl: dec!(0),
                ts_ms: 1_700_000_000_000,
                contract: Some(BrokerContractMetadata {
                    currency: Some("USD".to_string()),
                    primary_exchange: Some("NASDAQ".to_string()),
                    ..BrokerContractMetadata::default()
                }),
                liquidation_price: None,
                open_interest: None,
            }],
            runtime_open_orders: Vec::new(),
            broker_open_orders: Vec::new(),
            runtime_executions: Vec::new(),
            broker_executions: Vec::new(),
        });

        assert_eq!(audit.position_drifts.len(), 1);
        assert_eq!(audit.position_drifts[0].reason, "position_qty_drift");
        assert_eq!(audit.position_drifts[0].local_value.as_deref(), Some("2"));
        assert_eq!(audit.position_drifts[0].broker_value.as_deref(), Some("1"));
        assert_eq!(audit.position_drifts[0].currency.as_deref(), Some("USD"));
    }

    #[test]
    fn reconciliation_position_qty_drift_uses_runtime_currency_when_broker_lacks_currency() {
        let audit = reconcile_broker_audit(BrokerReconciliationInput {
            account_id: "DU123".to_string(),
            broker_kind: BrokerKind::InteractiveBrokers,
            ts_ms: 1_700_000_000_000,
            thresholds: BrokerReconciliationThresholds {
                cash_abs: dec!(1),
                position_qty_abs: dec!(0),
                stale_after_ms: 60_000,
            },
            runtime_cash: Vec::new(),
            broker_cash: Vec::new(),
            runtime_positions: vec![RuntimePositionSnapshot {
                account_id: "DU123".to_string(),
                exchange: "SEHK".to_string(),
                symbol: "HK:SEHK:700:EQUITY".to_string(),
                position_side: BrokerPositionSide::Long,
                qty: dec!(2),
                avg_price: dec!(300),
                margin_used: dec!(0),
                contract: Some(BrokerContractMetadata {
                    currency: Some("HKD".to_string()),
                    primary_exchange: Some("SEHK".to_string()),
                    ..BrokerContractMetadata::default()
                }),
            }],
            broker_positions: vec![BrokerPositionSnapshot {
                account_id: "DU123".to_string(),
                exchange: "IBKR".to_string(),
                symbol: "HK:SEHK:700:EQUITY".to_string(),
                position_side: BrokerPositionSide::Long,
                qty: dec!(1),
                avg_price: dec!(300),
                mark_price: None,
                margin_used: dec!(0),
                unrealized_pnl: dec!(0),
                ts_ms: 1_700_000_000_000,
                contract: Some(BrokerContractMetadata {
                    primary_exchange: Some("SEHK".to_string()),
                    ..BrokerContractMetadata::default()
                }),
                liquidation_price: None,
                open_interest: None,
            }],
            runtime_open_orders: Vec::new(),
            broker_open_orders: Vec::new(),
            runtime_executions: Vec::new(),
            broker_executions: Vec::new(),
        });

        assert_eq!(audit.position_drifts.len(), 1);
        assert_eq!(audit.position_drifts[0].reason, "position_qty_drift");
        assert_eq!(audit.position_drifts[0].currency.as_deref(), Some("HKD"));
    }

    #[test]
    fn reconciliation_matches_positions_by_conid_before_symbol_fallback() {
        let audit = reconcile_broker_audit(BrokerReconciliationInput {
            account_id: "DU123".to_string(),
            broker_kind: BrokerKind::InteractiveBrokers,
            ts_ms: 1_700_000_000_000,
            thresholds: BrokerReconciliationThresholds {
                cash_abs: dec!(1),
                position_qty_abs: dec!(0),
                stale_after_ms: 60_000,
            },
            runtime_cash: Vec::new(),
            broker_cash: Vec::new(),
            runtime_positions: vec![RuntimePositionSnapshot {
                account_id: "DU123".to_string(),
                exchange: "SEHK".to_string(),
                symbol: "HK:SEHK:700:EQUITY".to_string(),
                position_side: BrokerPositionSide::Long,
                qty: dec!(2),
                avg_price: dec!(300),
                margin_used: dec!(0),
                contract: Some(BrokerContractMetadata {
                    conid: Some(8068578),
                    currency: Some("HKD".to_string()),
                    primary_exchange: Some("SEHK".to_string()),
                    local_symbol: Some("700".to_string()),
                    ..BrokerContractMetadata::default()
                }),
            }],
            broker_positions: vec![BrokerPositionSnapshot {
                account_id: "DU123".to_string(),
                exchange: "IBKR".to_string(),
                symbol: "HK:SEHK:00700:EQUITY".to_string(),
                position_side: BrokerPositionSide::Long,
                qty: dec!(2),
                avg_price: dec!(300),
                mark_price: None,
                margin_used: dec!(0),
                unrealized_pnl: dec!(0),
                ts_ms: 1_700_000_000_000,
                contract: Some(BrokerContractMetadata {
                    conid: Some(8068578),
                    currency: Some("HKD".to_string()),
                    primary_exchange: Some("SEHK".to_string()),
                    local_symbol: Some("00700".to_string()),
                    ..BrokerContractMetadata::default()
                }),
                liquidation_price: None,
                open_interest: None,
            }],
            runtime_open_orders: Vec::new(),
            broker_open_orders: Vec::new(),
            runtime_executions: Vec::new(),
            broker_executions: Vec::new(),
        });

        assert!(audit.position_drifts.is_empty());
        assert_eq!(audit.severity, BrokerReconciliationSeverity::Info);
    }

    #[test]
    fn reconciliation_rejects_symbol_fallback_when_both_positions_have_different_conids() {
        let audit = reconcile_broker_audit(BrokerReconciliationInput {
            account_id: "DU123".to_string(),
            broker_kind: BrokerKind::InteractiveBrokers,
            ts_ms: 1_700_000_000_000,
            thresholds: BrokerReconciliationThresholds {
                cash_abs: dec!(1),
                position_qty_abs: dec!(0),
                stale_after_ms: 60_000,
            },
            runtime_cash: Vec::new(),
            broker_cash: Vec::new(),
            runtime_positions: vec![RuntimePositionSnapshot {
                account_id: "DU123".to_string(),
                exchange: "IBKR".to_string(),
                symbol: "US:NASDAQ:MSFT:EQUITY".to_string(),
                position_side: BrokerPositionSide::Long,
                qty: dec!(1),
                avg_price: dec!(400),
                margin_used: dec!(0),
                contract: Some(BrokerContractMetadata {
                    conid: Some(272093),
                    currency: Some("USD".to_string()),
                    primary_exchange: Some("NASDAQ".to_string()),
                    ..BrokerContractMetadata::default()
                }),
            }],
            broker_positions: vec![BrokerPositionSnapshot {
                account_id: "DU123".to_string(),
                exchange: "IBKR".to_string(),
                symbol: "US:NASDAQ:MSFT:EQUITY".to_string(),
                position_side: BrokerPositionSide::Long,
                qty: dec!(1),
                avg_price: dec!(400),
                mark_price: None,
                margin_used: dec!(0),
                unrealized_pnl: dec!(0),
                ts_ms: 1_700_000_000_000,
                contract: Some(BrokerContractMetadata {
                    conid: Some(123456789),
                    currency: Some("USD".to_string()),
                    primary_exchange: Some("NASDAQ".to_string()),
                    ..BrokerContractMetadata::default()
                }),
                liquidation_price: None,
                open_interest: None,
            }],
            runtime_open_orders: Vec::new(),
            broker_open_orders: Vec::new(),
            runtime_executions: Vec::new(),
            broker_executions: Vec::new(),
        });

        assert_eq!(audit.position_drifts.len(), 2);
        assert!(audit.position_drifts.iter().any(|drift| {
            drift.reason == "position_missing_runtime"
                && drift.broker_value.as_deref() == Some("1")
                && drift.local_value.is_none()
        }));
        assert!(audit.position_drifts.iter().any(|drift| {
            drift.reason == "position_missing_broker"
                && drift.local_value.as_deref() == Some("1")
                && drift.broker_value.is_none()
        }));
        assert_eq!(audit.severity, BrokerReconciliationSeverity::Error);
    }

    #[test]
    fn reconciliation_report_detects_runtime_open_order_missing_from_broker() {
        let audit = reconcile_broker_audit(BrokerReconciliationInput {
            account_id: "DU123".to_string(),
            broker_kind: BrokerKind::InteractiveBrokers,
            ts_ms: 1_700_000_000_000,
            thresholds: BrokerReconciliationThresholds {
                cash_abs: dec!(1),
                position_qty_abs: dec!(0),
                stale_after_ms: 60_000,
            },
            runtime_cash: Vec::new(),
            broker_cash: Vec::new(),
            runtime_positions: Vec::new(),
            broker_positions: Vec::new(),
            runtime_open_orders: vec![RuntimeOpenOrder {
                account_id: "DU123".to_string(),
                symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
                order_id: "local-order-1".to_string(),
                client_order_id: "client-order-1".to_string(),
                broker_order_id: Some("broker-order-1".to_string()),
            }],
            broker_open_orders: Vec::new(),
            runtime_executions: Vec::new(),
            broker_executions: Vec::new(),
        });

        assert_eq!(audit.open_order_drifts.len(), 1);
        assert_eq!(
            audit.open_order_drifts[0].reason,
            "open_order_missing_broker"
        );
        assert_eq!(
            audit.open_order_drifts[0].local_value.as_deref(),
            Some("broker-order-1")
        );
        assert_eq!(audit.severity, BrokerReconciliationSeverity::Error);
    }

    #[test]
    fn reconciliation_report_does_not_match_open_order_when_client_ids_are_empty() {
        let audit = reconcile_broker_audit(BrokerReconciliationInput {
            account_id: "DU123".to_string(),
            broker_kind: BrokerKind::InteractiveBrokers,
            ts_ms: 1_700_000_000_000,
            thresholds: BrokerReconciliationThresholds {
                cash_abs: dec!(1),
                position_qty_abs: dec!(0),
                stale_after_ms: 60_000,
            },
            runtime_cash: Vec::new(),
            broker_cash: Vec::new(),
            runtime_positions: Vec::new(),
            broker_positions: Vec::new(),
            runtime_open_orders: vec![RuntimeOpenOrder {
                account_id: "DU123".to_string(),
                symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
                order_id: "local-order-1".to_string(),
                client_order_id: "".to_string(),
                broker_order_id: None,
            }],
            broker_open_orders: vec![BrokerOpenOrder {
                broker_order_id: "broker-order-1".to_string(),
                client_order_id: "".to_string(),
                account_id: "DU123".to_string(),
                symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
                side: trader_core::OrderSide::Buy,
                order_type: trader_core::OrderType::Limit,
                price: Some(dec!(180)),
                qty: dec!(1),
                filled_qty: dec!(0),
                status: "Submitted".to_string(),
            }],
            runtime_executions: Vec::new(),
            broker_executions: Vec::new(),
        });

        assert_eq!(audit.open_order_drifts.len(), 2);
        assert!(audit.open_order_drifts.iter().any(|drift| {
            drift.reason == "open_order_missing_runtime"
                && drift.broker_value.as_deref() == Some("broker-order-1")
        }));
        assert!(audit.open_order_drifts.iter().any(|drift| {
            drift.reason == "open_order_missing_broker"
                && drift.local_value.as_deref() == Some("local-order-1")
        }));
        assert_eq!(audit.severity, BrokerReconciliationSeverity::Error);
    }

    #[test]
    fn reconciliation_report_does_not_match_open_order_across_accounts() {
        let audit = reconcile_broker_audit(BrokerReconciliationInput {
            account_id: "DU123".to_string(),
            broker_kind: BrokerKind::InteractiveBrokers,
            ts_ms: 1_700_000_000_000,
            thresholds: BrokerReconciliationThresholds {
                cash_abs: dec!(1),
                position_qty_abs: dec!(0),
                stale_after_ms: 60_000,
            },
            runtime_cash: Vec::new(),
            broker_cash: Vec::new(),
            runtime_positions: Vec::new(),
            broker_positions: Vec::new(),
            runtime_open_orders: vec![RuntimeOpenOrder {
                account_id: "DU123".to_string(),
                symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
                order_id: "local-order-1".to_string(),
                client_order_id: "client-order-1".to_string(),
                broker_order_id: Some("broker-order-1".to_string()),
            }],
            broker_open_orders: vec![BrokerOpenOrder {
                broker_order_id: "broker-order-1".to_string(),
                client_order_id: "client-order-1".to_string(),
                account_id: "DU999".to_string(),
                symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
                side: trader_core::OrderSide::Buy,
                order_type: trader_core::OrderType::Limit,
                price: Some(dec!(180)),
                qty: dec!(1),
                filled_qty: dec!(0),
                status: "Submitted".to_string(),
            }],
            runtime_executions: Vec::new(),
            broker_executions: Vec::new(),
        });

        assert_eq!(audit.open_order_drifts.len(), 2);
        assert!(audit.open_order_drifts.iter().any(|drift| {
            drift.account_id == "DU999" && drift.reason == "open_order_missing_runtime"
        }));
        assert!(audit.open_order_drifts.iter().any(|drift| {
            drift.account_id == "DU123" && drift.reason == "open_order_missing_broker"
        }));
        assert_eq!(audit.severity, BrokerReconciliationSeverity::Error);
    }

    #[test]
    fn reconciliation_report_matches_execution_by_runtime_order_metadata() {
        let audit = reconcile_broker_audit(BrokerReconciliationInput {
            account_id: "DU123".to_string(),
            broker_kind: BrokerKind::InteractiveBrokers,
            ts_ms: 1_700_000_000_000,
            thresholds: BrokerReconciliationThresholds {
                cash_abs: dec!(1),
                position_qty_abs: dec!(0),
                stale_after_ms: 60_000,
            },
            runtime_cash: Vec::new(),
            broker_cash: Vec::new(),
            runtime_positions: Vec::new(),
            broker_positions: Vec::new(),
            runtime_open_orders: Vec::new(),
            broker_open_orders: Vec::new(),
            runtime_executions: vec![RuntimeExecution {
                fill_id: "local-fill-1".to_string(),
                order_id: "local-order-1".to_string(),
                account_id: Some("DU123".to_string()),
                symbol: Some("US:NASDAQ:AAPL:EQUITY".to_string()),
                client_order_id: Some("client-order-1".to_string()),
                broker_order_id: Some("broker-order-1".to_string()),
                side: Some(trader_core::OrderSide::Buy),
                price: Some(dec!(180)),
                qty: Some(dec!(1)),
                fee: Some(dec!(1)),
            }],
            broker_executions: vec![BrokerExecution {
                trade_id: "broker-trade-1".to_string(),
                broker_order_id: "broker-order-1".to_string(),
                client_order_id: Some("client-order-1".to_string()),
                account_id: "DU123".to_string(),
                symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
                side: trader_core::OrderSide::Buy,
                price: dec!(180),
                qty: dec!(1),
                fee: dec!(1),
                ts_ms: 1_700_000_000_000,
            }],
        });

        assert!(audit.execution_drifts.is_empty());
        assert_eq!(audit.severity, BrokerReconciliationSeverity::Info);
    }

    #[test]
    fn reconciliation_report_does_not_match_execution_when_only_client_ids_are_absent() {
        let audit = reconcile_broker_audit(BrokerReconciliationInput {
            account_id: "DU123".to_string(),
            broker_kind: BrokerKind::InteractiveBrokers,
            ts_ms: 1_700_000_000_000,
            thresholds: BrokerReconciliationThresholds {
                cash_abs: dec!(1),
                position_qty_abs: dec!(0),
                stale_after_ms: 60_000,
            },
            runtime_cash: Vec::new(),
            broker_cash: Vec::new(),
            runtime_positions: Vec::new(),
            broker_positions: Vec::new(),
            runtime_open_orders: Vec::new(),
            broker_open_orders: Vec::new(),
            runtime_executions: vec![RuntimeExecution {
                fill_id: "local-fill-1".to_string(),
                order_id: "local-order-1".to_string(),
                account_id: Some("DU123".to_string()),
                symbol: Some("US:NASDAQ:AAPL:EQUITY".to_string()),
                client_order_id: None,
                broker_order_id: None,
                side: Some(trader_core::OrderSide::Buy),
                price: Some(dec!(180)),
                qty: Some(dec!(1)),
                fee: Some(dec!(1)),
            }],
            broker_executions: vec![BrokerExecution {
                trade_id: "broker-trade-1".to_string(),
                broker_order_id: "broker-order-1".to_string(),
                client_order_id: None,
                account_id: "DU123".to_string(),
                symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
                side: trader_core::OrderSide::Buy,
                price: dec!(180),
                qty: dec!(1),
                fee: dec!(1),
                ts_ms: 1_700_000_000_000,
            }],
        });

        assert_eq!(audit.execution_drifts.len(), 1);
        assert_eq!(
            audit.execution_drifts[0].reason,
            "execution_missing_runtime"
        );
        assert_eq!(audit.severity, BrokerReconciliationSeverity::Error);
    }

    #[test]
    fn reconciliation_report_does_not_match_execution_across_accounts() {
        let audit = reconcile_broker_audit(BrokerReconciliationInput {
            account_id: "DU123".to_string(),
            broker_kind: BrokerKind::InteractiveBrokers,
            ts_ms: 1_700_000_000_000,
            thresholds: BrokerReconciliationThresholds {
                cash_abs: dec!(1),
                position_qty_abs: dec!(0),
                stale_after_ms: 60_000,
            },
            runtime_cash: Vec::new(),
            broker_cash: Vec::new(),
            runtime_positions: Vec::new(),
            broker_positions: Vec::new(),
            runtime_open_orders: Vec::new(),
            broker_open_orders: Vec::new(),
            runtime_executions: vec![RuntimeExecution {
                fill_id: "broker-trade-1".to_string(),
                order_id: "broker-order-1".to_string(),
                account_id: Some("DU123".to_string()),
                symbol: Some("US:NASDAQ:AAPL:EQUITY".to_string()),
                client_order_id: Some("client-order-1".to_string()),
                broker_order_id: Some("broker-order-1".to_string()),
                side: Some(trader_core::OrderSide::Buy),
                price: Some(dec!(180)),
                qty: Some(dec!(1)),
                fee: Some(dec!(1)),
            }],
            broker_executions: vec![BrokerExecution {
                trade_id: "broker-trade-1".to_string(),
                broker_order_id: "broker-order-1".to_string(),
                client_order_id: Some("client-order-1".to_string()),
                account_id: "DU999".to_string(),
                symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
                side: trader_core::OrderSide::Buy,
                price: dec!(180),
                qty: dec!(1),
                fee: dec!(1),
                ts_ms: 1_700_000_000_000,
            }],
        });

        assert_eq!(audit.execution_drifts.len(), 1);
        assert_eq!(audit.execution_drifts[0].account_id, "DU999");
        assert_eq!(
            audit.execution_drifts[0].reason,
            "execution_missing_runtime"
        );
        assert_eq!(audit.severity, BrokerReconciliationSeverity::Error);
    }

    #[test]
    fn reconciliation_report_does_not_match_execution_when_client_ids_are_empty() {
        let audit = reconcile_broker_audit(BrokerReconciliationInput {
            account_id: "DU123".to_string(),
            broker_kind: BrokerKind::InteractiveBrokers,
            ts_ms: 1_700_000_000_000,
            thresholds: BrokerReconciliationThresholds {
                cash_abs: dec!(1),
                position_qty_abs: dec!(0),
                stale_after_ms: 60_000,
            },
            runtime_cash: Vec::new(),
            broker_cash: Vec::new(),
            runtime_positions: Vec::new(),
            broker_positions: Vec::new(),
            runtime_open_orders: Vec::new(),
            broker_open_orders: Vec::new(),
            runtime_executions: vec![RuntimeExecution {
                fill_id: "local-fill-1".to_string(),
                order_id: "local-order-1".to_string(),
                account_id: Some("DU123".to_string()),
                symbol: Some("US:NASDAQ:AAPL:EQUITY".to_string()),
                client_order_id: Some("".to_string()),
                broker_order_id: None,
                side: Some(trader_core::OrderSide::Buy),
                price: Some(dec!(180)),
                qty: Some(dec!(1)),
                fee: Some(dec!(1)),
            }],
            broker_executions: vec![BrokerExecution {
                trade_id: "broker-trade-1".to_string(),
                broker_order_id: "broker-order-1".to_string(),
                client_order_id: Some("".to_string()),
                account_id: "DU123".to_string(),
                symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
                side: trader_core::OrderSide::Buy,
                price: dec!(180),
                qty: dec!(1),
                fee: dec!(1),
                ts_ms: 1_700_000_000_000,
            }],
        });

        assert_eq!(audit.execution_drifts.len(), 1);
        assert_eq!(
            audit.execution_drifts[0].reason,
            "execution_missing_runtime"
        );
        assert_eq!(audit.severity, BrokerReconciliationSeverity::Error);
    }

    #[test]
    fn reconciliation_report_detects_execution_field_drift_after_identity_match() {
        let audit = reconcile_broker_audit(BrokerReconciliationInput {
            account_id: "DU123".to_string(),
            broker_kind: BrokerKind::InteractiveBrokers,
            ts_ms: 1_700_000_000_000,
            thresholds: BrokerReconciliationThresholds {
                cash_abs: dec!(1),
                position_qty_abs: dec!(0),
                stale_after_ms: 60_000,
            },
            runtime_cash: Vec::new(),
            broker_cash: Vec::new(),
            runtime_positions: Vec::new(),
            broker_positions: Vec::new(),
            runtime_open_orders: Vec::new(),
            broker_open_orders: Vec::new(),
            runtime_executions: vec![RuntimeExecution {
                fill_id: "broker-trade-1".to_string(),
                order_id: "local-order-1".to_string(),
                account_id: Some("DU123".to_string()),
                symbol: Some("US:NASDAQ:AAPL:EQUITY".to_string()),
                client_order_id: Some("client-order-1".to_string()),
                broker_order_id: Some("broker-order-1".to_string()),
                side: Some(trader_core::OrderSide::Buy),
                price: Some(dec!(179)),
                qty: Some(dec!(2)),
                fee: Some(dec!(0.5)),
            }],
            broker_executions: vec![BrokerExecution {
                trade_id: "broker-trade-1".to_string(),
                broker_order_id: "broker-order-1".to_string(),
                client_order_id: Some("client-order-1".to_string()),
                account_id: "DU123".to_string(),
                symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
                side: trader_core::OrderSide::Buy,
                price: dec!(180),
                qty: dec!(1),
                fee: dec!(1),
                ts_ms: 1_700_000_000_000,
            }],
        });

        assert_eq!(audit.execution_drifts.len(), 1);
        assert_eq!(audit.execution_drifts[0].reason, "execution_field_drift");
        let local_value = audit.execution_drifts[0].local_value.as_deref().unwrap();
        assert!(local_value.contains("price:local=179:broker=180"));
        assert!(local_value.contains("qty:local=2:broker=1"));
        assert!(local_value.contains("fee:local=0.5:broker=1"));
        assert_eq!(audit.severity, BrokerReconciliationSeverity::Error);
    }
}
