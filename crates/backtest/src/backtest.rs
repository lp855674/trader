#![forbid(unsafe_code)]

use accounting::PositionBook;
use broker::{Broker, MockBroker};
use data::Bar;
use execution::immediate_order;
use portfolio::equal_weight_target;
use risk::check_max_position;
use rust_decimal::Decimal;
use serde::Serialize;
use storage::{Db, NewFill, NewOrder, NewPosition, NewStrategyRun};
use strategies::{MovingAverageCrossStrategy, Strategy};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BacktestSummary {
    pub signals: usize,
    pub orders: usize,
}

#[derive(Debug, Clone)]
pub struct BacktestSettings {
    pub run_id: String,
    pub strategy_name: String,
    pub symbol: String,
    pub account_id: String,
    pub order_qty: Decimal,
    pub max_abs_qty: Decimal,
}

impl BacktestSettings {
    pub fn sample() -> Self {
        Self {
            run_id: "sample-ma-cross".to_string(),
            strategy_name: "moving_average_cross".to_string(),
            symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
            account_id: "backtest".to_string(),
            order_qty: Decimal::ONE,
            max_abs_qty: Decimal::from(100),
        }
    }
}

#[derive(Default)]
pub struct BacktestRuntime {
    db: Option<Db>,
    settings: BacktestSettings,
}

impl BacktestRuntime {
    pub fn new(db: Db, settings: BacktestSettings) -> Self {
        Self {
            db: Some(db),
            settings,
        }
    }

    pub async fn run(&self, bars: Vec<Bar>) -> anyhow::Result<BacktestSummary> {
        let mut strategy = MovingAverageCrossStrategy::new(
            self.settings.strategy_name.clone(),
            self.settings.symbol.clone(),
            2,
            3,
        );
        let broker = MockBroker;
        let mut position_book = PositionBook::default();
        let mut signals = 0;
        let mut orders = 0;
        let started_at_ms = bars.first().map_or(0, |bar| bar.ts_ms);
        let mut ended_at_ms = started_at_ms;

        for bar in bars {
            ended_at_ms = bar.ts_ms;
            if let Some(signal) = strategy.on_bar(&bar) {
                signals += 1;
                let target = equal_weight_target(&signal, self.settings.order_qty);
                check_max_position(&target, self.settings.max_abs_qty)?;
                let order = immediate_order(&target, self.settings.account_id.clone());
                let response = broker.place_order(order.clone()).await?;
                let order_number = orders + 1;
                let order_id = format!("{}-order-{}", self.settings.run_id, order_number);
                let fill_id = format!("{}-fill-{}", self.settings.run_id, order_number);

                if let Some(db) = &self.db {
                    db.insert_order(NewOrder {
                        id: order_id.clone(),
                        run_id: self.settings.run_id.clone(),
                        client_order_id: order_id.clone(),
                        broker_order_id: Some(response.broker_order_id),
                        account_id: order.account_id.clone(),
                        symbol: order.symbol.clone(),
                        side: format!("{:?}", order.side).to_uppercase(),
                        order_type: format!("{:?}", order.order_type).to_uppercase(),
                        price: order.price.map(|price| price.to_string()),
                        qty: order.qty.to_string(),
                        filled_qty: order.qty.to_string(),
                        status: "FILLED".to_string(),
                        created_at_ms: bar.ts_ms,
                        updated_at_ms: bar.ts_ms,
                    })
                    .await?;
                    db.insert_fill(NewFill {
                        id: fill_id,
                        order_id,
                        run_id: self.settings.run_id.clone(),
                        symbol: order.symbol.clone(),
                        side: format!("{:?}", order.side).to_uppercase(),
                        price: bar.close.to_string(),
                        qty: order.qty.to_string(),
                        fee: Decimal::ZERO.to_string(),
                        ts_ms: bar.ts_ms,
                    })
                    .await?;
                }

                position_book.buy(
                    &order.symbol,
                    order.qty * Decimal::from(order.side.sign()),
                    bar.close,
                );
                orders += 1;
            }
        }

        if let Some(db) = &self.db {
            db.insert_strategy_run(NewStrategyRun {
                id: self.settings.run_id.clone(),
                name: self.settings.strategy_name.clone(),
                mode: "backtest".to_string(),
                status: "completed".to_string(),
                started_at_ms,
                ended_at_ms: Some(ended_at_ms),
                error: None,
                config_json: "{}".to_string(),
            })
            .await?;

            if let Some(position) = position_book.position(&self.settings.symbol) {
                db.upsert_position(NewPosition {
                    run_id: self.settings.run_id.clone(),
                    account_id: self.settings.account_id.clone(),
                    symbol: position.symbol.clone(),
                    qty: position.qty.to_string(),
                    avg_price: position.avg_price.to_string(),
                    updated_at_ms: ended_at_ms,
                })
                .await?;
            }
        }

        Ok(BacktestSummary { signals, orders })
    }
}

impl Default for BacktestSettings {
    fn default() -> Self {
        Self::sample()
    }
}
