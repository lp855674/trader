pub mod engine;
pub mod executor;
pub mod granularity;
pub mod models;
pub mod performance;
pub mod portfolio;
pub mod storage;

pub use engine::{BacktestConfig, BacktestEngine, BacktestError, BacktestState};
pub use executor::{Fill, Order, OrderType, SimulatedExecutor};
pub use granularity::{GranularityConverter, KlineResampler, TickToKline, TimeGranularity};
pub use models::{
    CostModel, FixedSlippage, FlatCommission, NoSlippage, PercentCommission, TieredCommission,
    VolumeSlippage,
};
pub use performance::{EquityCurve, PerformanceCalculator, PerformanceReport};
pub use portfolio::{
    CorrelationTracker, PortfolioBacktest, PortfolioConfig, PortfolioError, PortfolioState,
};
pub use storage::{BacktestResult, BacktestResultBuilder, ResultStore};
