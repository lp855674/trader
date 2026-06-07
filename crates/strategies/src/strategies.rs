#![forbid(unsafe_code)]

use data::Bar;
use events::{SignalEvent, SignalSide};
use rust_decimal::Decimal;
use thiserror::Error;

pub trait Strategy: Send {
    fn on_bar(&mut self, bar: &Bar) -> Option<SignalEvent>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrategyRuntimeMode {
    Backtest,
    Replay,
    Paper,
    Live,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrategyContext {
    pub strategy_id: String,
    pub symbol: String,
    pub runtime_mode: StrategyRuntimeMode,
}

impl StrategyContext {
    pub fn new(
        strategy_id: impl Into<String>,
        symbol: impl Into<String>,
        runtime_mode: StrategyRuntimeMode,
    ) -> Self {
        Self {
            strategy_id: strategy_id.into(),
            symbol: symbol.into(),
            runtime_mode,
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum StrategyRegistryError {
    #[error("unknown strategy {0}")]
    UnknownStrategy(String),
}

#[derive(Debug, Default, Clone, Copy)]
pub struct StrategyRegistry;

impl StrategyRegistry {
    pub fn create(
        &self,
        name: &str,
        context: StrategyContext,
        fast_window: usize,
        slow_window: usize,
    ) -> Result<Box<dyn Strategy + Send>, StrategyRegistryError> {
        match name {
            "moving_average_cross" => Ok(Box::new(MovingAverageCrossStrategy::from_context(
                context,
                fast_window,
                slow_window,
            ))),
            other => Err(StrategyRegistryError::UnknownStrategy(other.to_string())),
        }
    }

    pub fn create_alpha(
        &self,
        name: &str,
        context: StrategyContext,
        fast_window: usize,
        slow_window: usize,
    ) -> Result<Box<dyn alpha::AlphaModel + Send + Sync>, StrategyRegistryError> {
        match name {
            "moving_average_cross" => Ok(Box::new(MovingAverageCrossStrategy::from_context(
                context,
                fast_window,
                slow_window,
            ))),
            other => Err(StrategyRegistryError::UnknownStrategy(other.to_string())),
        }
    }
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

    pub fn from_context(context: StrategyContext, fast_window: usize, slow_window: usize) -> Self {
        Self::new(
            context.strategy_id,
            context.symbol,
            fast_window,
            slow_window,
        )
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

impl alpha::AlphaModel for MovingAverageCrossStrategy {
    fn on_bar(&mut self, bar: &Bar) -> Option<SignalEvent> {
        <Self as Strategy>::on_bar(self, bar)
    }
}
