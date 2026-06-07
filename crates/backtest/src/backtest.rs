#![forbid(unsafe_code)]

use algorithm::{AlgorithmEngine, AlgorithmEngineSettings, EngineEvent, ExecutionReport};
use data::Bar;
use rust_decimal::Decimal;
use serde::Serialize;
use storage::{Db, NewEventRecord, NewFill, NewOrder, NewPosition, NewStrategyRun};
use strategies::{StrategyContext, StrategyRegistry, StrategyRuntimeMode};

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
    pub max_exposure: Decimal,
    pub max_drawdown: Decimal,
    pub max_leverage: Decimal,
    pub max_margin_used: Decimal,
    pub trading_halted: bool,
    pub initial_equity: Decimal,
    pub fast_window: usize,
    pub slow_window: usize,
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
            max_exposure: Decimal::from(1_000_000),
            max_drawdown: Decimal::ONE,
            max_leverage: Decimal::from(10),
            max_margin_used: Decimal::ZERO,
            trading_halted: false,
            initial_equity: Decimal::from(100_000),
            fast_window: 2,
            slow_window: 3,
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
        let registry = StrategyRegistry;
        let strategy = registry.create_alpha(
            &self.settings.strategy_name,
            StrategyContext::new(
                self.settings.strategy_name.clone(),
                self.settings.symbol.clone(),
                StrategyRuntimeMode::Backtest,
            ),
            self.settings.fast_window,
            self.settings.slow_window,
        )?;
        let mut engine = AlgorithmEngine::new(
            AlgorithmEngineSettings {
                run_id: self.settings.run_id.clone(),
                mode: StrategyRuntimeMode::Backtest,
                account_id: self.settings.account_id.clone(),
                symbol: self.settings.symbol.clone(),
                order_qty: self.settings.order_qty,
                max_abs_qty: self.settings.max_abs_qty,
                max_order_qty: self.settings.max_abs_qty,
                max_order_notional: self.settings.max_exposure,
                min_cash_after_order: Decimal::ZERO,
                max_exposure: self.settings.max_exposure,
                max_drawdown: self.settings.max_drawdown,
                max_leverage: self.settings.max_leverage,
                max_margin_used: self.settings.max_margin_used,
                trading_halted: self.settings.trading_halted,
                initial_cash: self.settings.initial_equity,
            },
            strategy,
        );
        let mut signals = 0;
        let mut orders = 0;
        let started_at_ms = bars.first().map_or(0, |bar| bar.ts_ms);
        let mut ended_at_ms = started_at_ms;
        let mut last_close = Decimal::ONE;

        for bar in bars {
            ended_at_ms = bar.ts_ms;
            last_close = bar.close;
            let step = engine.on_bar(bar.clone())?;
            if let Some(decision) = step.decision {
                signals += 1;
                let Some(order) = decision.order else {
                    continue;
                };
                let broker_order_id = format!("backtest-{}", decision.order_number);
                let execution = engine.apply_execution(
                    &order,
                    &ExecutionReport {
                        broker_order_id: broker_order_id.clone(),
                        status: "FILLED".to_string(),
                        price: bar.close,
                        qty: order.qty,
                        fee: Decimal::ZERO,
                    },
                    bar.ts_ms,
                )?;

                if let Some(db) = &self.db {
                    persist_engine_events(db, &self.settings.run_id, &decision.events).await?;
                    persist_engine_events(db, &self.settings.run_id, &execution.events).await?;
                    db.insert_order(NewOrder {
                        id: decision.order_id.clone(),
                        run_id: self.settings.run_id.clone(),
                        client_order_id: decision.order_id.clone(),
                        broker_order_id: Some(broker_order_id),
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
                        id: decision.fill_id,
                        order_id: decision.order_id,
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

            let snapshot = engine.snapshot(last_close)?;
            if snapshot.position_qty != Decimal::ZERO {
                db.upsert_position(NewPosition {
                    run_id: self.settings.run_id.clone(),
                    account_id: self.settings.account_id.clone(),
                    symbol: self.settings.symbol.clone(),
                    qty: snapshot.position_qty.to_string(),
                    avg_price: snapshot.position_avg_price.to_string(),
                    updated_at_ms: ended_at_ms,
                })
                .await?;
            }
        }

        Ok(BacktestSummary { signals, orders })
    }
}

async fn persist_engine_events(
    db: &Db,
    run_id: &str,
    events: &[EngineEvent],
) -> anyhow::Result<()> {
    for event in events {
        db.insert_event(NewEventRecord {
            event_id: uuid::Uuid::new_v4().to_string(),
            ts_ms: event.ts_ms,
            source: run_id.to_string(),
            category: event.category.clone(),
            payload_json: event.payload_json.clone(),
        })
        .await?;
    }
    Ok(())
}

impl Default for BacktestSettings {
    fn default() -> Self {
        Self::sample()
    }
}
