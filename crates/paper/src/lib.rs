#![forbid(unsafe_code)]

use backtest::{BacktestRuntime, BacktestSettings, BacktestSummary};
use data::Bar;
use storage::Db;

pub struct PaperRuntime {
    inner: BacktestRuntime,
}

impl PaperRuntime {
    pub fn new(db: Db, settings: BacktestSettings) -> Self {
        Self {
            inner: BacktestRuntime::new(db, settings),
        }
    }

    pub async fn run_bars(&self, bars: Vec<Bar>) -> anyhow::Result<BacktestSummary> {
        self.inner.run(bars).await
    }
}
