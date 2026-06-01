#![forbid(unsafe_code)]

use broker::{Broker, MockBroker};
use data::Bar;
use execution::immediate_order;
use portfolio::equal_weight_target;
use risk::check_max_position;
use rust_decimal::Decimal;
use strategies::{MovingAverageCrossStrategy, Strategy};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BacktestSummary {
    pub signals: usize,
    pub orders: usize,
}

#[derive(Default)]
pub struct BacktestRuntime;

impl BacktestRuntime {
    pub async fn run(&self, bars: Vec<Bar>) -> anyhow::Result<BacktestSummary> {
        let mut strategy = MovingAverageCrossStrategy::new("ma", "US:NASDAQ:AAPL:EQUITY", 2, 3);
        let broker = MockBroker;
        let mut signals = 0;
        let mut orders = 0;

        for bar in bars {
            if let Some(signal) = strategy.on_bar(&bar) {
                signals += 1;
                let target = equal_weight_target(&signal, Decimal::ONE);
                check_max_position(&target, Decimal::from(100))?;
                let order = immediate_order(&target, "backtest");
                broker.place_order(order).await?;
                orders += 1;
            }
        }

        Ok(BacktestSummary { signals, orders })
    }
}
