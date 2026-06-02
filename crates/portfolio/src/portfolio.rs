#![forbid(unsafe_code)]

use events::{SignalEvent, SignalSide};
use rust_decimal::Decimal;

#[derive(Debug, Clone, PartialEq)]
pub struct TargetPosition {
    pub symbol: String,
    pub target_qty: Decimal,
}

pub fn equal_weight_target(signal: &SignalEvent, qty: Decimal) -> TargetPosition {
    let signed_qty = match signal.side {
        SignalSide::Buy | SignalSide::CloseShort => qty,
        SignalSide::Sell | SignalSide::CloseLong => Decimal::ZERO,
    };
    TargetPosition {
        symbol: signal.symbol.clone(),
        target_qty: signed_qty,
    }
}
