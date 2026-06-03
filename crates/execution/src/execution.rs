#![forbid(unsafe_code)]

use portfolio::TargetPosition;
use rust_decimal::Decimal;
use thiserror::Error;
use trader_core::{OrderRequest, OrderSide, OrderType};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ExecutionError {
    #[error("execution intent must contain at least one slice")]
    EmptySlices,
    #[error("execution intent weights must sum to a positive value")]
    InvalidWeights,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TimeSlicedIntent {
    pub order: OrderRequest,
    pub slices: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WeightedIntent {
    pub order: OrderRequest,
    pub weights: Vec<Decimal>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReduceOnlyIntent {
    pub order: OrderRequest,
    pub current_qty: Decimal,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionIntent {
    Immediate(OrderRequest),
    Twap(TimeSlicedIntent),
    Vwap(WeightedIntent),
    PostOnly(OrderRequest, Decimal),
    ReduceOnly(ReduceOnlyIntent),
}

pub fn expand_execution_intent(
    intent: ExecutionIntent,
) -> Result<Vec<OrderRequest>, ExecutionError> {
    match intent {
        ExecutionIntent::Immediate(order) => Ok(vec![order]),
        ExecutionIntent::Twap(intent) => expand_twap(intent),
        ExecutionIntent::Vwap(intent) => expand_vwap(intent),
        ExecutionIntent::PostOnly(mut order, limit_price) => {
            order.order_type = OrderType::PostOnly;
            order.price = Some(limit_price);
            Ok(vec![order])
        }
        ExecutionIntent::ReduceOnly(intent) => Ok(expand_reduce_only(intent)),
    }
}

fn expand_twap(intent: TimeSlicedIntent) -> Result<Vec<OrderRequest>, ExecutionError> {
    if intent.slices == 0 {
        return Err(ExecutionError::EmptySlices);
    }

    let slice_qty = intent.order.qty / Decimal::from(intent.slices);
    let mut allocated_qty = Decimal::ZERO;
    let mut orders = Vec::with_capacity(intent.slices);
    for index in 0..intent.slices {
        let mut order = intent.order.clone();
        order.qty = if index + 1 == intent.slices {
            intent.order.qty - allocated_qty
        } else {
            slice_qty
        };
        allocated_qty += order.qty;
        orders.push(order);
    }
    Ok(orders)
}

fn expand_vwap(intent: WeightedIntent) -> Result<Vec<OrderRequest>, ExecutionError> {
    if intent.weights.is_empty() {
        return Err(ExecutionError::EmptySlices);
    }
    let total_weight = intent
        .weights
        .iter()
        .copied()
        .fold(Decimal::ZERO, |sum, weight| sum + weight);
    if total_weight <= Decimal::ZERO {
        return Err(ExecutionError::InvalidWeights);
    }

    let mut allocated_qty = Decimal::ZERO;
    let mut orders = Vec::with_capacity(intent.weights.len());
    for (index, weight) in intent.weights.iter().enumerate() {
        let mut order = intent.order.clone();
        order.qty = if index + 1 == intent.weights.len() {
            intent.order.qty - allocated_qty
        } else {
            intent.order.qty * *weight / total_weight
        };
        allocated_qty += order.qty;
        orders.push(order);
    }
    Ok(orders)
}

fn expand_reduce_only(intent: ReduceOnlyIntent) -> Vec<OrderRequest> {
    let reducible_qty = match (intent.order.side, intent.current_qty > Decimal::ZERO) {
        (OrderSide::Sell, true) => intent.current_qty,
        (OrderSide::Buy, false) if intent.current_qty < Decimal::ZERO => intent.current_qty.abs(),
        _ => Decimal::ZERO,
    };
    if reducible_qty <= Decimal::ZERO {
        return Vec::new();
    }

    let mut order = intent.order;
    order.qty = order.qty.min(reducible_qty);
    vec![order]
}

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
