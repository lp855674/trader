#![forbid(unsafe_code)]

use thiserror::Error;
use trader_core::OrderStatus;

#[derive(Debug, Error, PartialEq)]
pub enum OmsError {
    #[error("invalid transition from {0:?}")]
    InvalidTransition(OrderStatus),
}

pub struct OrderStateMachine {
    status: OrderStatus,
}

impl OrderStateMachine {
    pub fn new() -> Self {
        Self {
            status: OrderStatus::New,
        }
    }

    pub fn status(&self) -> OrderStatus {
        self.status
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
