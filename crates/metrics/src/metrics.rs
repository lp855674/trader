#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MetricsSummary {
    pub total_return: String,
    pub order_count: usize,
    pub fill_count: usize,
}

pub fn total_return(start_equity: Decimal, end_equity: Decimal) -> Decimal {
    (end_equity - start_equity) / start_equity
}

pub fn paper_summary(
    order_count: usize,
    fill_count: usize,
    initial_equity: Decimal,
    final_equity: Decimal,
) -> MetricsSummary {
    MetricsSummary {
        total_return: total_return(initial_equity, final_equity).to_string(),
        order_count,
        fill_count,
    }
}
