//! 策略与配套模块：`quantd` / `pipeline` 使用精简 `strategy` 子模块，其余为研究与数据工具。

pub mod backtest;
pub mod strategy;

pub mod core {
    pub mod combinator;
    pub mod combinators;
    pub mod hot_reload;
    pub mod logger;
    pub mod metrics;
    pub mod registry;
    pub mod r#trait;
    pub use combinator::{Conditional, Pipeline, RoundRobin, SignalFilter, WeightedAverage};
    pub use combinators::{
        DynamicWeightedAverage, Ensemble, MinQuantityFilter, PerformanceAwareRoundRobin,
        PerformanceTracker, PositionSizingFilter, SignalNormalizer, SignalOutcome, StopLossFilter,
        StrategyStats, VotingPolicy,
    };
    pub use hot_reload::{ConfigDiff, HotReloadHandle, HotReloadWatcher, ReloadConfig};
    pub use logger::{LogEvent, LoggingStrategy, StructuredLogger};
    pub use metrics::{EvaluationTimer, MeteredStrategy, MetricsRegistry, StrategyMetrics};
    pub use registry::{RegistryError, StrategyFactory, StrategyRegistry};
    pub use r#trait::{DataSourceError, Granularity, HistoricalData, Kline, Tick};
}

pub mod data {
    pub mod kline;
    pub mod sources;
}

pub mod analysis;
pub mod api;
pub mod config;
pub mod event;
pub mod optimizer;
pub mod scheduler;
pub mod trading;

pub use strategy::{AlwaysLongOne, NoOpStrategy, ScoredCandidate, Strategy, StrategyContext};

pub mod model;
pub use model::LstmStrategy;
pub use model::ModelStrategy;
pub use model as lstm;
