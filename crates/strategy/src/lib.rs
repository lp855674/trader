//! 策略与配套模块：`quantd` / `pipeline` 使用精简 `strategy` 子模块，其余为研究与数据工具。

pub mod strategy;
pub mod backtest;

pub mod core {
    pub mod r#trait;
    pub mod combinator;
    pub mod combinators;
    pub mod registry;
    pub mod hot_reload;
    pub mod logger;
    pub mod metrics;
    pub use r#trait::{DataSourceError, Granularity, HistoricalData, Kline, Tick};
    pub use combinator::{Conditional, Pipeline, RoundRobin, SignalFilter, WeightedAverage};
    pub use combinators::{
        DynamicWeightedAverage, Ensemble, MinQuantityFilter, PerformanceAwareRoundRobin,
        PerformanceTracker, PositionSizingFilter, SignalNormalizer, SignalOutcome, StopLossFilter,
        StrategyStats, VotingPolicy,
    };
    pub use registry::{RegistryError, StrategyFactory, StrategyRegistry};
    pub use hot_reload::{ConfigDiff, HotReloadHandle, HotReloadWatcher, ReloadConfig};
    pub use logger::{LogEvent, LoggingStrategy, StructuredLogger};
    pub use metrics::{EvaluationTimer, MeteredStrategy, MetricsRegistry, StrategyMetrics};
}

pub mod data {
    pub mod kline;
    pub mod sources;
}

pub mod event;
pub mod scheduler;
pub mod optimizer;
pub mod analysis;
pub mod trading;
pub mod config;
pub mod api;

pub use strategy::{AlwaysLongOne, NoOpStrategy, Strategy, StrategyContext};

pub mod lstm;
pub use lstm::LstmStrategy;
