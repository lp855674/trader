#![forbid(unsafe_code)]

use rust_decimal::Decimal;

pub fn total_return(start_equity: Decimal, end_equity: Decimal) -> Decimal {
    (end_equity - start_equity) / start_equity
}
