use std::collections::HashMap;

use domain::InstrumentId;
use thiserror::Error;
use super::types::OrderRequest;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum OrderState {
    Pending,
    Submitted,
    PartiallyFilled { filled_qty: f64 },
    Filled,
    Cancelled,
    Rejected { reason: String },
}

impl OrderState {
    pub fn is_terminal(&self) -> bool {
        matches!(self, OrderState::Filled | OrderState::Cancelled | OrderState::Rejected { .. })
    }

    fn name(&self) -> &'static str {
        match self {
            OrderState::Pending => "Pending",
            OrderState::Submitted => "Submitted",
            OrderState::PartiallyFilled { .. } => "PartiallyFilled",
            OrderState::Filled => "Filled",
            OrderState::Cancelled => "Cancelled",
            OrderState::Rejected { .. } => "Rejected",
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum OrderEvent {
    Submit,
    Accept,
    PartialFill { qty: f64, price: f64 },
    Fill { qty: f64, price: f64 },
    Cancel,
    Reject { reason: String },
}

impl OrderEvent {
    fn name(&self) -> &'static str {
        match self {
            OrderEvent::Submit => "Submit",
            OrderEvent::Accept => "Accept",
            OrderEvent::PartialFill { .. } => "PartialFill",
            OrderEvent::Fill { .. } => "Fill",
            OrderEvent::Cancel => "Cancel",
            OrderEvent::Reject { .. } => "Reject",
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Order {
    pub id: String,
    pub request: OrderRequest,
    pub state: OrderState,
    pub filled_qty: f64,
    pub avg_fill_price: f64,
    pub created_ts_ms: i64,
    pub updated_ts_ms: i64,
    pub idempotency_key: Option<String>,
}

impl Order {
    pub fn new(request: OrderRequest, ts_ms: i64) -> Self {
        let id = format!("{}_{}", request.client_order_id, ts_ms);
        Self {
            id,
            request,
            state: OrderState::Pending,
            filled_qty: 0.0,
            avg_fill_price: 0.0,
            created_ts_ms: ts_ms,
            updated_ts_ms: ts_ms,
            idempotency_key: None,
        }
    }

    pub fn transition(&mut self, event: OrderEvent, ts_ms: i64) -> Result<(), OrderError> {
        let new_state = match (&self.state, &event) {
            (OrderState::Pending, OrderEvent::Submit) => OrderState::Submitted,
            (OrderState::Pending, OrderEvent::Cancel) => OrderState::Cancelled,
            (OrderState::Pending, OrderEvent::Reject { reason }) => {
                OrderState::Rejected { reason: reason.clone() }
            }
            (OrderState::Submitted, OrderEvent::Accept) => OrderState::Submitted,
            (OrderState::Submitted, OrderEvent::PartialFill { qty, price }) => {
                let new_filled = self.filled_qty + qty;
                // update running average fill price
                if self.filled_qty == 0.0 {
                    self.avg_fill_price = *price;
                } else {
                    self.avg_fill_price = (self.avg_fill_price * self.filled_qty + price * qty)
                        / new_filled;
                }
                self.filled_qty = new_filled;
                OrderState::PartiallyFilled { filled_qty: new_filled }
            }
            (OrderState::Submitted, OrderEvent::Fill { qty, price }) => {
                let new_filled = self.filled_qty + qty;
                if self.filled_qty == 0.0 {
                    self.avg_fill_price = *price;
                } else {
                    self.avg_fill_price = (self.avg_fill_price * self.filled_qty + price * qty)
                        / new_filled;
                }
                self.filled_qty = new_filled;
                OrderState::Filled
            }
            (OrderState::Submitted, OrderEvent::Cancel) => OrderState::Cancelled,
            (OrderState::Submitted, OrderEvent::Reject { reason }) => {
                OrderState::Rejected { reason: reason.clone() }
            }
            (OrderState::PartiallyFilled { .. }, OrderEvent::PartialFill { qty, price }) => {
                let new_filled = self.filled_qty + qty;
                self.avg_fill_price =
                    (self.avg_fill_price * self.filled_qty + price * qty) / new_filled;
                self.filled_qty = new_filled;
                OrderState::PartiallyFilled { filled_qty: new_filled }
            }
            (OrderState::PartiallyFilled { .. }, OrderEvent::Fill { qty, price }) => {
                let new_filled = self.filled_qty + qty;
                self.avg_fill_price =
                    (self.avg_fill_price * self.filled_qty + price * qty) / new_filled;
                self.filled_qty = new_filled;
                OrderState::Filled
            }
            (OrderState::PartiallyFilled { .. }, OrderEvent::Cancel) => OrderState::Cancelled,
            _ => {
                return Err(OrderError::InvalidTransition {
                    from: self.state.name().to_string(),
                    event: event.name().to_string(),
                })
            }
        };
        self.state = new_state;
        self.updated_ts_ms = ts_ms;
        Ok(())
    }

    pub fn remaining_qty(&self) -> f64 {
        self.request.quantity - self.filled_qty
    }
}

#[derive(Debug, Error)]
pub enum OrderError {
    #[error("invalid transition from {from} on event {event}")]
    InvalidTransition { from: String, event: String },
    #[error("order not found: {0}")]
    OrderNotFound(String),
    #[error("duplicate client_order_id: {0}")]
    DuplicateClientOrderId(String),
    #[error("invalid quantity: {0}")]
    InvalidQuantity(String),
}

pub struct OrderManager {
    pub orders: HashMap<String, Order>,
}

impl OrderManager {
    pub fn new() -> Self {
        Self { orders: HashMap::new() }
    }

    pub fn submit(&mut self, request: OrderRequest, ts_ms: i64) -> Result<String, OrderError> {
        if request.quantity <= 0.0 {
            return Err(OrderError::InvalidQuantity(format!(
                "quantity must be positive, got {}",
                request.quantity
            )));
        }
        // Idempotency: if same client_order_id exists and not terminal, return existing id
        for (id, order) in &self.orders {
            if order.request.client_order_id == request.client_order_id {
                if !order.state.is_terminal() {
                    return Ok(id.clone());
                }
            }
        }
        let order = Order::new(request, ts_ms);
        let id = order.id.clone();
        self.orders.insert(id.clone(), order);
        Ok(id)
    }

    pub fn apply_event(
        &mut self,
        order_id: &str,
        event: OrderEvent,
        ts_ms: i64,
    ) -> Result<(), OrderError> {
        let order = self
            .orders
            .get_mut(order_id)
            .ok_or_else(|| OrderError::OrderNotFound(order_id.to_string()))?;
        order.transition(event, ts_ms)
    }

    pub fn cancel(&mut self, order_id: &str, ts_ms: i64) -> Result<(), OrderError> {
        self.apply_event(order_id, OrderEvent::Cancel, ts_ms)
    }

    pub fn modify_quantity(
        &mut self,
        order_id: &str,
        new_qty: f64,
        ts_ms: i64,
    ) -> Result<(), OrderError> {
        if new_qty <= 0.0 {
            return Err(OrderError::InvalidQuantity(format!(
                "quantity must be positive, got {}",
                new_qty
            )));
        }
        let order = self
            .orders
            .get_mut(order_id)
            .ok_or_else(|| OrderError::OrderNotFound(order_id.to_string()))?;
        match &order.state {
            OrderState::Pending | OrderState::Submitted => {
                order.request.quantity = new_qty;
                order.updated_ts_ms = ts_ms;
                Ok(())
            }
            _ => Err(OrderError::InvalidTransition {
                from: order.state.name().to_string(),
                event: "modify_quantity".to_string(),
            }),
        }
    }

    pub fn get(&self, order_id: &str) -> Option<&Order> {
        self.orders.get(order_id)
    }

    pub fn open_orders(&self) -> Vec<&Order> {
        self.orders.values().filter(|o| !o.state.is_terminal()).collect()
    }

    pub fn orders_for_instrument(&self, instrument: &InstrumentId) -> Vec<&Order> {
        self.orders
            .values()
            .filter(|o| &o.request.instrument == instrument)
            .collect()
    }
}

impl Default for OrderManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use domain::{InstrumentId, Side, Venue};

    use super::*;
    use crate::core::types::{OrderFlag, OrderKind, TimeInForce};

    fn make_request(client_id: &str, qty: f64) -> OrderRequest {
        OrderRequest {
            client_order_id: client_id.to_string(),
            instrument: InstrumentId::new(Venue::Crypto, "BTC-USD"),
            side: Side::Buy,
            quantity: qty,
            kind: OrderKind::Market,
            tif: TimeInForce::GTC,
            flags: vec![],
            strategy_id: "test".to_string(),
            submitted_ts_ms: 1000,
        }
    }

    #[test]
    fn fsm_rejects_invalid_transition() {
        let req = make_request("c1", 1.0);
        let mut order = Order::new(req, 1000);
        // Can't fill directly from Pending
        let result = order.transition(OrderEvent::Fill { qty: 1.0, price: 100.0 }, 1001);
        assert!(result.is_err());
        matches!(result.unwrap_err(), OrderError::InvalidTransition { .. });
    }

    #[test]
    fn fsm_valid_transitions() {
        let req = make_request("c2", 2.0);
        let mut order = Order::new(req, 1000);
        order.transition(OrderEvent::Submit, 1001).unwrap();
        assert_eq!(order.state, OrderState::Submitted);
        order.transition(OrderEvent::PartialFill { qty: 1.0, price: 100.0 }, 1002).unwrap();
        assert_eq!(order.state, OrderState::PartiallyFilled { filled_qty: 1.0 });
        assert!((order.avg_fill_price - 100.0).abs() < 1e-9);
        order.transition(OrderEvent::Fill { qty: 1.0, price: 102.0 }, 1003).unwrap();
        assert_eq!(order.state, OrderState::Filled);
        assert!((order.avg_fill_price - 101.0).abs() < 1e-9);
    }

    #[test]
    fn idempotent_submit() {
        let mut mgr = OrderManager::new();
        let req1 = make_request("same_client", 1.0);
        let req2 = make_request("same_client", 1.0);
        let id1 = mgr.submit(req1, 1000).unwrap();
        let id2 = mgr.submit(req2, 1001).unwrap();
        assert_eq!(id1, id2);
    }

    #[test]
    fn cancel_works() {
        let mut mgr = OrderManager::new();
        let req = make_request("c3", 1.0);
        let id = mgr.submit(req, 1000).unwrap();
        mgr.cancel(&id, 1001).unwrap();
        let order = mgr.get(&id).unwrap();
        assert_eq!(order.state, OrderState::Cancelled);
        assert!(order.state.is_terminal());
    }

    #[test]
    fn partial_fill_updates_avg_price() {
        let req = make_request("c4", 3.0);
        let mut order = Order::new(req, 1000);
        order.transition(OrderEvent::Submit, 1001).unwrap();
        order.transition(OrderEvent::PartialFill { qty: 1.0, price: 100.0 }, 1002).unwrap();
        order.transition(OrderEvent::PartialFill { qty: 2.0, price: 106.0 }, 1003).unwrap();
        // avg = (100*1 + 106*2) / 3 = 312/3 = 104
        assert!((order.avg_fill_price - 104.0).abs() < 1e-6);
    }
}
