pub mod engine;
pub mod executor;
pub mod models;
pub mod granularity;
pub mod performance;
pub mod storage;
pub mod portfolio;

pub use engine::{BacktestEngine, BacktestConfig, BacktestState, BacktestError};
pub use executor::{SimulatedExecutor, Order, Fill, OrderType};
pub use models::{CostModel, FixedSlippage, VolumeSlippage, NoSlippage, PercentCommission, FlatCommission, TieredCommission};
pub use granularity::{TimeGranularity, KlineResampler, TickToKline, GranularityConverter};
pub use performance::{PerformanceReport, PerformanceCalculator, EquityCurve};
pub use storage::{BacktestResult, ResultStore, BacktestResultBuilder};
pub use portfolio::{PortfolioBacktest, PortfolioConfig, PortfolioState, PortfolioError, CorrelationTracker};
