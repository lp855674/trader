#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use thiserror::Error;
use trader_core::OrderStatus;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum OmsError {
    #[error("invalid transition from {0:?}")]
    InvalidTransition(OrderStatus),
    #[error("fill quantity must be positive")]
    InvalidFillQuantity,
    #[error("fill quantity exceeds remaining quantity")]
    Overfill,
}

pub struct OrderStateMachine {
    status: OrderStatus,
    order_qty: Decimal,
    filled_qty: Decimal,
}

impl OrderStateMachine {
    pub fn new() -> Self {
        Self::with_order_qty(Decimal::ONE)
    }

    pub fn with_order_qty(order_qty: Decimal) -> Self {
        Self {
            status: OrderStatus::New,
            order_qty,
            filled_qty: Decimal::ZERO,
        }
    }

    pub fn status(&self) -> OrderStatus {
        self.status
    }

    pub fn filled_qty(&self) -> Decimal {
        self.filled_qty
    }

    pub fn remaining_qty(&self) -> Decimal {
        self.order_qty - self.filled_qty
    }

    pub fn submit(&mut self) -> Result<(), OmsError> {
        self.transition(OrderStatus::Submitted, &[OrderStatus::New])
    }

    pub fn accept(&mut self) -> Result<(), OmsError> {
        self.transition(OrderStatus::Submitted, &[OrderStatus::Submitted])
    }

    pub fn fill(&mut self) -> Result<(), OmsError> {
        self.transition(
            OrderStatus::Filled,
            &[OrderStatus::Submitted, OrderStatus::PartiallyFilled],
        )
    }

    pub fn request_cancel(&mut self) -> Result<(), OmsError> {
        self.transition(
            OrderStatus::PendingCancel,
            &[
                OrderStatus::New,
                OrderStatus::PendingSubmit,
                OrderStatus::Submitted,
            ],
        )
    }

    pub fn cancel(&mut self) -> Result<(), OmsError> {
        self.transition(OrderStatus::Canceled, &[OrderStatus::PendingCancel])
    }

    pub fn reject(&mut self) -> Result<(), OmsError> {
        self.transition(
            OrderStatus::Rejected,
            &[
                OrderStatus::New,
                OrderStatus::PendingSubmit,
                OrderStatus::Submitted,
            ],
        )
    }

    pub fn record_fill(&mut self, fill_qty: Decimal) -> Result<(), OmsError> {
        if fill_qty <= Decimal::ZERO {
            return Err(OmsError::InvalidFillQuantity);
        }
        if fill_qty > self.remaining_qty() {
            return Err(OmsError::Overfill);
        }
        if !matches!(
            self.status,
            OrderStatus::Submitted | OrderStatus::PartiallyFilled
        ) {
            return Err(OmsError::InvalidTransition(self.status));
        }

        self.filled_qty += fill_qty;
        self.status = if self.remaining_qty() == Decimal::ZERO {
            OrderStatus::Filled
        } else {
            OrderStatus::PartiallyFilled
        };
        Ok(())
    }

    fn transition(&mut self, next: OrderStatus, allowed: &[OrderStatus]) -> Result<(), OmsError> {
        if !allowed.contains(&self.status) {
            return Err(OmsError::InvalidTransition(self.status));
        }
        self.status = next;
        Ok(())
    }
}

impl Default for OrderStateMachine {
    fn default() -> Self {
        Self::new()
    }
}
