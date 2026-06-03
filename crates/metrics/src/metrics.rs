#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MetricsSummary {
    pub total_return: String,
    pub sharpe: String,
    pub sortino: String,
    pub max_drawdown: String,
    pub win_rate: String,
    pub order_count: usize,
    pub fill_count: usize,
}

pub fn total_return(start_equity: Decimal, end_equity: Decimal) -> Decimal {
    if start_equity.is_zero() {
        return Decimal::ZERO;
    }
    (end_equity - start_equity) / start_equity
}

pub fn equity_returns(equity: &[Decimal]) -> Vec<Decimal> {
    equity
        .windows(2)
        .filter_map(|window| {
            let start = window.first()?;
            let end = window.get(1)?;
            if start.is_zero() {
                return Some(Decimal::ZERO);
            }
            Some((*end - *start) / *start)
        })
        .collect()
}

pub fn max_drawdown(equity: &[Decimal]) -> Decimal {
    let Some(first_equity) = equity.first() else {
        return Decimal::ZERO;
    };
    if first_equity.is_zero() {
        return Decimal::ZERO;
    }

    let mut peak = *first_equity;
    let mut worst = Decimal::ZERO;
    for current in equity {
        if *current > peak {
            peak = *current;
        }
        if peak.is_zero() {
            continue;
        }
        let drawdown = (peak - *current) / peak;
        if drawdown > worst {
            worst = drawdown;
        }
    }
    worst
}

pub fn win_rate(returns: &[Decimal]) -> Decimal {
    if returns.is_empty() {
        return Decimal::ZERO;
    }
    let wins = returns
        .iter()
        .filter(|value| **value > Decimal::ZERO)
        .count();
    Decimal::from(wins) / Decimal::from(returns.len())
}

pub fn sharpe_ratio(returns: &[Decimal]) -> Decimal {
    let mean = mean(returns);
    let deviation = population_deviation(returns, mean);
    if deviation.is_zero() {
        return Decimal::ZERO;
    }
    mean / deviation
}

pub fn sortino_ratio(returns: &[Decimal]) -> Decimal {
    let mean = mean(returns);
    let downside: Vec<Decimal> = returns
        .iter()
        .copied()
        .filter(|value| *value < Decimal::ZERO)
        .collect();
    let deviation = population_deviation(&downside, Decimal::ZERO);
    if deviation.is_zero() {
        return Decimal::ZERO;
    }
    mean / deviation
}

pub fn paper_summary(
    order_count: usize,
    fill_count: usize,
    equity: &[Decimal],
    returns: &[Decimal],
) -> MetricsSummary {
    let total_return_value = match (equity.first(), equity.last()) {
        (Some(start), Some(end)) => total_return(*start, *end),
        _ => Decimal::ZERO,
    };

    MetricsSummary {
        total_return: format_decimal(total_return_value),
        sharpe: format_decimal(sharpe_ratio(returns)),
        sortino: format_decimal(sortino_ratio(returns)),
        max_drawdown: format_decimal(max_drawdown(equity)),
        win_rate: format_decimal(win_rate(returns)),
        order_count,
        fill_count,
    }
}

fn format_decimal(value: Decimal) -> String {
    value.normalize().to_string()
}

fn mean(values: &[Decimal]) -> Decimal {
    if values.is_empty() {
        return Decimal::ZERO;
    }
    values.iter().sum::<Decimal>() / Decimal::from(values.len())
}

fn population_deviation(values: &[Decimal], center: Decimal) -> Decimal {
    if values.is_empty() {
        return Decimal::ZERO;
    }
    let variance = values
        .iter()
        .map(|value| {
            let distance = *value - center;
            distance * distance
        })
        .sum::<Decimal>()
        / Decimal::from(values.len());
    decimal_sqrt(variance)
}

fn decimal_sqrt(value: Decimal) -> Decimal {
    if value <= Decimal::ZERO {
        return Decimal::ZERO;
    }

    let two = Decimal::from(2);
    let mut estimate = value;
    for _ in 0..32 {
        estimate = (estimate + value / estimate) / two;
    }
    estimate.normalize()
}
