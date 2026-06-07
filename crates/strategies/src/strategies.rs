#![forbid(unsafe_code)]

use data::Bar;
use events::{SignalEvent, SignalSide};
use indicators::{IndicatorError, SimpleMovingAverage};
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
    #[error("invalid strategy configuration: {0}")]
    InvalidConfig(#[from] StrategyConfigError),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum StrategyConfigError {
    #[error("moving_average_cross windows must be greater than zero")]
    InvalidMovingAverageWindow,
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
            )?)),
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
            )?)),
            other => Err(StrategyRegistryError::UnknownStrategy(other.to_string())),
        }
    }
}

pub struct MovingAverageCrossStrategy {
    strategy_id: String,
    symbol: String,
    fast_average: SimpleMovingAverage,
    slow_average: SimpleMovingAverage,
    last_side: Option<SignalSide>,
}

impl MovingAverageCrossStrategy {
    pub fn new(
        strategy_id: impl Into<String>,
        symbol: impl Into<String>,
        fast_window: usize,
        slow_window: usize,
    ) -> Result<Self, StrategyConfigError> {
        Ok(Self {
            strategy_id: strategy_id.into(),
            symbol: symbol.into(),
            fast_average: SimpleMovingAverage::new(fast_window).map_err(strategy_config_error)?,
            slow_average: SimpleMovingAverage::new(slow_window).map_err(strategy_config_error)?,
            last_side: None,
        })
    }

    pub fn from_context(
        context: StrategyContext,
        fast_window: usize,
        slow_window: usize,
    ) -> Result<Self, StrategyConfigError> {
        Self::new(
            context.strategy_id,
            context.symbol,
            fast_window,
            slow_window,
        )
    }
}

impl Strategy for MovingAverageCrossStrategy {
    fn on_bar(&mut self, bar: &Bar) -> Option<SignalEvent> {
        let fast = self.fast_average.update(bar.close);
        let slow = self.slow_average.update(bar.close);
        let (Some(fast), Some(slow)) = (fast, slow) else {
            return None;
        };
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

fn strategy_config_error(error: IndicatorError) -> StrategyConfigError {
    match error {
        IndicatorError::ZeroPeriod => StrategyConfigError::InvalidMovingAverageWindow,
    }
}
