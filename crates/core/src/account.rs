use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountSnapshot {
    pub account_id: String,
    pub cash: Decimal,
    pub equity: Decimal,
    pub buying_power: Decimal,
    pub margin_used: Decimal,
    pub unrealized_pnl: Decimal,
    pub realized_pnl: Decimal,
}
