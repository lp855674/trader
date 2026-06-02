#![forbid(unsafe_code)]

use accounting::AccountBook;
use backtest::{BacktestSettings, BacktestSummary};
use broker::{SimulatedBrokerSettings, simulate_market_fill};
use data::Bar;
use execution::immediate_order;
use portfolio::equal_weight_target;
use risk::check_max_position;
use rust_decimal::Decimal;
use storage::{
    Db, NewAccountBalance, NewFill, NewOrder, NewPortfolioSnapshot, NewPosition, NewStrategyRun,
};
use strategies::{MovingAverageCrossStrategy, Strategy};
use trader_core::OrderSide;

pub struct PaperRuntime {
    db: Db,
    settings: BacktestSettings,
}

impl PaperRuntime {
    pub fn new(db: Db, settings: BacktestSettings) -> Self {
        Self { db, settings }
    }

    pub async fn run_bars(&self, bars: Vec<Bar>) -> anyhow::Result<BacktestSummary> {
        let mut strategy = MovingAverageCrossStrategy::new(
            self.settings.strategy_name.clone(),
            self.settings.symbol.clone(),
            2,
            3,
        );
        let broker_settings = SimulatedBrokerSettings {
            slippage_bps: Decimal::ZERO,
            fee_bps: Decimal::ZERO,
        };
        let mut account_book =
            AccountBook::new(self.settings.account_id.clone(), Decimal::from(10_000));
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
                let fill = simulate_market_fill(order.clone(), bar.close, broker_settings.clone())?;
                let order_number = orders + 1;
                let order_id = format!("{}-order-{}", self.settings.run_id, order_number);
                let fill_id = format!("{}-fill-{}", self.settings.run_id, order_number);

                self.db
                    .insert_order(NewOrder {
                        id: order_id.clone(),
                        run_id: self.settings.run_id.clone(),
                        client_order_id: order_id.clone(),
                        broker_order_id: Some(format!("simulated-{}", order_number)),
                        account_id: order.account_id.clone(),
                        symbol: order.symbol.clone(),
                        side: format!("{:?}", order.side).to_uppercase(),
                        order_type: format!("{:?}", order.order_type).to_uppercase(),
                        price: order.price.map(|price| price.to_string()),
                        qty: order.qty.to_string(),
                        filled_qty: fill.qty.to_string(),
                        status: "FILLED".to_string(),
                        created_at_ms: bar.ts_ms,
                        updated_at_ms: bar.ts_ms,
                    })
                    .await?;
                self.db
                    .insert_fill(NewFill {
                        id: fill_id,
                        order_id,
                        run_id: self.settings.run_id.clone(),
                        symbol: order.symbol.clone(),
                        side: format!("{:?}", order.side).to_uppercase(),
                        price: fill.price.to_string(),
                        qty: fill.qty.to_string(),
                        fee: fill.fee.to_string(),
                        ts_ms: bar.ts_ms,
                    })
                    .await?;

                let signed_qty = match order.side {
                    OrderSide::Buy => fill.qty,
                    OrderSide::Sell => -fill.qty,
                };
                account_book.buy(&order.symbol, signed_qty, fill.price, fill.fee);
                orders += 1;
            }

            let market_value = account_book.market_value(&self.settings.symbol, bar.close);
            let equity = account_book.equity(&self.settings.symbol, bar.close);
            self.db
                .insert_portfolio_snapshot(NewPortfolioSnapshot {
                    id: format!("{}-snapshot-{}", self.settings.run_id, bar.ts_ms),
                    run_id: self.settings.run_id.clone(),
                    account_id: self.settings.account_id.clone(),
                    ts_ms: bar.ts_ms,
                    cash: account_book.cash().to_string(),
                    market_value: market_value.to_string(),
                    equity: equity.to_string(),
                    realized_pnl: account_book.realized_pnl.to_string(),
                    unrealized_pnl: Decimal::ZERO.to_string(),
                })
                .await?;
        }

        self.db
            .insert_strategy_run(NewStrategyRun {
                id: self.settings.run_id.clone(),
                name: self.settings.strategy_name.clone(),
                mode: "paper".to_string(),
                status: "completed".to_string(),
                started_at_ms,
                ended_at_ms: Some(ended_at_ms),
                config_json: "{}".to_string(),
            })
            .await?;

        self.db
            .upsert_account_balance(NewAccountBalance {
                run_id: self.settings.run_id.clone(),
                account_id: self.settings.account_id.clone(),
                asset: "USD".to_string(),
                total: account_book.cash().to_string(),
                available: account_book.cash().to_string(),
                frozen: Decimal::ZERO.to_string(),
                updated_at_ms: ended_at_ms,
            })
            .await?;

        if let Some(position) = account_book.position(&self.settings.symbol) {
            self.db
                .upsert_position(NewPosition {
                    run_id: self.settings.run_id.clone(),
                    account_id: self.settings.account_id.clone(),
                    symbol: position.symbol.clone(),
                    qty: position.qty.to_string(),
                    avg_price: position.avg_price.to_string(),
                    updated_at_ms: ended_at_ms,
                })
                .await?;
        }

        Ok(BacktestSummary { signals, orders })
    }
}
