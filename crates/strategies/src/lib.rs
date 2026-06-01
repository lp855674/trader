#![forbid(unsafe_code)]

use data::Bar;
use events::{SignalEvent, SignalSide};
use rust_decimal::Decimal;

pub trait Strategy {
    fn on_bar(&mut self, bar: &Bar) -> Option<SignalEvent>;
}

pub struct MovingAverageCrossStrategy {
    strategy_id: String,
    symbol: String,
    fast_window: usize,
    slow_window: usize,
    closes: Vec<Decimal>,
    last_side: Option<SignalSide>,
}

impl MovingAverageCrossStrategy {
    pub fn new(
        strategy_id: impl Into<String>,
        symbol: impl Into<String>,
        fast_window: usize,
        slow_window: usize,
    ) -> Self {
        Self {
            strategy_id: strategy_id.into(),
            symbol: symbol.into(),
            fast_window,
            slow_window,
            closes: Vec::new(),
            last_side: None,
        }
    }

    fn mean(&self, window: usize) -> Option<Decimal> {
        if self.closes.len() < window {
            return None;
        }
        let sum: Decimal = self.closes[self.closes.len() - window..].iter().sum();
        Some(sum / Decimal::from(window))
    }
}

impl Strategy for MovingAverageCrossStrategy {
    fn on_bar(&mut self, bar: &Bar) -> Option<SignalEvent> {
        self.closes.push(bar.close);
        let fast = self.mean(self.fast_window)?;
        let slow = self.mean(self.slow_window)?;
        let side = if fast > slow {
            SignalSide::Buy
        } else if fast < slow {
            SignalSide::Sell
        } else {
            return None;
        };

        if self.last_side == Some(side) {
            return None;
        }
        self.last_side = Some(side);

        Some(SignalEvent {
            strategy_id: self.strategy_id.clone(),
            symbol: self.symbol.clone(),
            side,
            confidence: 0.8,
            ts: chrono::Utc::now(),
        })
    }
}
