#![forbid(unsafe_code)]

use algorithm::{AlgorithmEngine, AlgorithmEngineSettings, ExecutionReport};
use data::Bar;
use events::EventBus;
use rust_decimal::Decimal;
use serde::Serialize;
use storage::{
    BacktestCompletedRun, BacktestFilledExecutionCommand, BacktestPositionCommand, Db,
    RuntimeEventCommand,
};
use strategies::{StrategyAssemblyConfig, StrategyRegistry, StrategyRuntimeMode};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BacktestSummary {
    pub signals: usize,
    pub orders: usize,
}

#[derive(Debug, Clone)]
pub struct BacktestSettings {
    pub run_id: String,
    pub strategy_name: String,
    pub universe_name: String,
    pub alpha_name: String,
    pub symbols: Vec<String>,
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
            universe_name: "static".to_string(),
            alpha_name: "moving_average_cross".to_string(),
            symbols: vec!["US:NASDAQ:AAPL:EQUITY".to_string()],
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

    fn assembly_symbols(&self) -> Vec<String> {
        if self.symbols.iter().any(|symbol| symbol == &self.symbol) {
            return self.symbols.clone();
        }
        vec![self.symbol.clone()]
    }
}

#[derive(Default)]
pub struct BacktestRuntime {
    db: Option<Db>,
    settings: BacktestSettings,
    event_bus: Option<EventBus>,
}

impl BacktestRuntime {
    pub fn new(db: Db, settings: BacktestSettings) -> Self {
        Self {
            db: Some(db),
            settings,
            event_bus: None,
        }
    }

    pub fn with_event_bus(mut self, event_bus: EventBus) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    pub async fn run(&self, bars: Vec<Bar>) -> anyhow::Result<BacktestSummary> {
        let registry = StrategyRegistry;
        let assembly = registry.assemble_alpha(
            StrategyAssemblyConfig {
                strategy_name: self.settings.strategy_name.clone(),
                universe_name: self.settings.universe_name.clone(),
                alpha_name: self.settings.alpha_name.clone(),
                symbols: self.settings.assembly_symbols(),
                fast_window: self.settings.fast_window,
                slow_window: self.settings.slow_window,
            },
            StrategyRuntimeMode::Backtest,
        )?;
        let mut engine = AlgorithmEngine::new_with_universe(
            AlgorithmEngineSettings {
                run_id: self.settings.run_id.clone(),
                mode: StrategyRuntimeMode::Backtest,
                account_id: self.settings.account_id.clone(),
                symbol: assembly.primary_symbol.clone(),
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
            Box::new(assembly.universe),
            assembly.alpha,
        );
        if let Some(event_bus) = &self.event_bus {
            engine.set_event_bus(event_bus.clone());
        }
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
                    record_runtime_events(db, &self.settings.run_id, &decision.events).await?;
                    record_runtime_events(db, &self.settings.run_id, &execution.events).await?;
                    db.record_backtest_filled_execution(BacktestFilledExecutionCommand {
                        run_id: self.settings.run_id.clone(),
                        order_id: decision.order_id,
                        fill_id: decision.fill_id,
                        broker_order_id,
                        order,
                        fill_price: bar.close,
                        fee: Decimal::ZERO,
                        ts_ms: bar.ts_ms,
                    })
                    .await?;
                }
                orders += 1;
            }
        }

        if let Some(db) = &self.db {
            db.complete_backtest_run(BacktestCompletedRun {
                run_id: self.settings.run_id.clone(),
                strategy_name: self.settings.strategy_name.clone(),
                started_at_ms,
                ended_at_ms,
                config_json: "{}".to_string(),
            })
            .await?;

            let snapshot = engine.snapshot(last_close)?;
            if snapshot.position_qty != Decimal::ZERO {
                db.record_backtest_position(BacktestPositionCommand {
                    run_id: self.settings.run_id.clone(),
                    account_id: self.settings.account_id.clone(),
                    symbol: self.primary_symbol(),
                    qty: snapshot.position_qty,
                    avg_price: snapshot.position_avg_price,
                    updated_at_ms: ended_at_ms,
                })
                .await?;
            }
        }

        Ok(BacktestSummary { signals, orders })
    }

    fn primary_symbol(&self) -> String {
        self.settings
            .assembly_symbols()
            .into_iter()
            .next()
            .unwrap_or_else(|| self.settings.symbol.clone())
    }
}

async fn record_runtime_events(
    db: &Db,
    source: &str,
    events: &[algorithm::EngineEvent],
) -> anyhow::Result<()> {
    for event in events {
        db.record_runtime_event(RuntimeEventCommand {
            source: source.to_string(),
            ts_ms: event.ts_ms,
            category: event.category.clone(),
            payload: event.payload.clone(),
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
