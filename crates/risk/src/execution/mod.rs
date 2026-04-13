pub mod backtest;
pub mod live;
pub mod paper;

pub use backtest::{BacktestExecConfig, BacktestExecutionMode, BacktestSlippage};
pub use live::{LiveConfig, LiveExecutionMode};
pub use paper::{PaperExecConfig, PaperExecutionMode};
