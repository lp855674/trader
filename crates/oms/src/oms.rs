#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use std::collections::HashSet;
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
    processed_report_ids: HashSet<String>,
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
            processed_report_ids: HashSet::new(),
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

    pub fn apply_fill_report(
        &mut self,
        report_id: impl Into<String>,
        fill_qty: Decimal,
    ) -> Result<bool, OmsError> {
        let report_id = report_id.into();
        if self.has_report(&report_id) {
            return Ok(false);
        }
        self.record_fill(fill_qty)?;
        self.mark_report(report_id);
        Ok(true)
    }

    pub fn apply_cancel_report(&mut self, report_id: impl Into<String>) -> Result<bool, OmsError> {
        let report_id = report_id.into();
        if self.has_report(&report_id) {
            return Ok(false);
        }
        if self.status.is_terminal() {
            self.mark_report(report_id);
            return Ok(false);
        }
        if self.status != OrderStatus::PendingCancel {
            self.request_cancel()?;
        }
        self.cancel()?;
        self.mark_report(report_id);
        Ok(true)
    }

    pub fn apply_reject_report(&mut self, report_id: impl Into<String>) -> Result<bool, OmsError> {
        let report_id = report_id.into();
        if self.has_report(&report_id) {
            return Ok(false);
        }
        if self.status.is_terminal() {
            self.mark_report(report_id);
            return Ok(false);
        }
        self.reject()?;
        self.mark_report(report_id);
        Ok(true)
    }

    fn transition(&mut self, next: OrderStatus, allowed: &[OrderStatus]) -> Result<(), OmsError> {
        if !allowed.contains(&self.status) {
            return Err(OmsError::InvalidTransition(self.status));
        }
        self.status = next;
        Ok(())
    }

    fn has_report(&self, report_id: &str) -> bool {
        self.processed_report_ids.contains(report_id)
    }

    fn mark_report(&mut self, report_id: impl Into<String>) {
        self.processed_report_ids.insert(report_id.into());
    }
}

impl Default for OrderStateMachine {
    fn default() -> Self {
        Self::new()
    }
}
