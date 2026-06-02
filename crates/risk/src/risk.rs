#![forbid(unsafe_code)]

use portfolio::TargetPosition;
use rust_decimal::Decimal;
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum RiskError {
    #[error("target quantity exceeds max position")]
    MaxPosition,
}

pub fn check_max_position(target: &TargetPosition, max_abs_qty: Decimal) -> Result<(), RiskError> {
    if target.target_qty.abs() > max_abs_qty {
        return Err(RiskError::MaxPosition);
    }
    Ok(())
}
