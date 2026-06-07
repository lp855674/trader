#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use std::collections::VecDeque;
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum IndicatorError {
    #[error("indicator period must be greater than zero")]
    ZeroPeriod,
}

pub struct SimpleMovingAverage {
    period: usize,
    values: VecDeque<Decimal>,
    sum: Decimal,
}

impl SimpleMovingAverage {
    pub fn new(period: usize) -> Result<Self, IndicatorError> {
        if period == 0 {
            return Err(IndicatorError::ZeroPeriod);
        }
        Ok(Self {
            period,
            values: VecDeque::with_capacity(period),
            sum: Decimal::ZERO,
        })
    }

    pub fn update(&mut self, value: Decimal) -> Option<Decimal> {
        self.values.push_back(value);
        self.sum += value;
        if self.values.len() > self.period {
            if let Some(removed) = self.values.pop_front() {
                self.sum -= removed;
            }
        }
        if self.values.len() < self.period {
            return None;
        }
        Some(self.sum / Decimal::from(self.period))
    }
}

pub struct ExponentialMovingAverage {
    smoothing_factor: Decimal,
    current: Option<Decimal>,
}

impl ExponentialMovingAverage {
    pub fn new(period: usize) -> Result<Self, IndicatorError> {
        if period == 0 {
            return Err(IndicatorError::ZeroPeriod);
        }
        Ok(Self {
            smoothing_factor: Decimal::from(2) / Decimal::from(period + 1),
            current: None,
        })
    }

    pub fn update(&mut self, value: Decimal) -> Option<Decimal> {
        let next = match self.current {
            Some(current) => {
                value * self.smoothing_factor + current * (Decimal::ONE - self.smoothing_factor)
            }
            None => value,
        };
        self.current = Some(next);
        Some(next)
    }
}

pub fn indicator_sma(values: &[Decimal], period: usize) -> Result<Option<Decimal>, IndicatorError> {
    let mut average = SimpleMovingAverage::new(period)?;
    Ok(values
        .iter()
        .copied()
        .filter_map(|value| average.update(value))
        .last())
}

pub fn indicator_ema(values: &[Decimal], period: usize) -> Result<Option<Decimal>, IndicatorError> {
    let mut average = ExponentialMovingAverage::new(period)?;
    Ok(values
        .iter()
        .copied()
        .filter_map(|value| average.update(value))
        .last())
}
