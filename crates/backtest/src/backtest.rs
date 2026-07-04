#![forbid(unsafe_code)]

use algorithm::{AlgorithmEngine, AlgorithmEngineSettings, ExecutionReport};
use data::{Bar, MarketSlice};
use events::{EventBus, LogWriter, LogWriterSettings, SystemLogLayer};
use rust_decimal::Decimal;
use serde::Serialize;
use std::collections::BTreeSet;
use storage::{
    BacktestCompletedRun, BacktestFilledExecutionCommand, BacktestPositionCommand, Db,
    DbSystemLogSink, RuntimeEventCommand,
};
use strategies::{
    StrategyAlphaComponentConfig, StrategyAlphaConflictResolution, StrategyAlphaGateConfig,
    StrategyAssemblyConfig, StrategyRegistry, StrategyRuntimeMode, StrategyUniverseFilterConfig,
};
use tracing_subscriber::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BacktestSummary {
    pub signals: usize,
    pub orders: usize,
}

#[derive(Debug, Clone)]
pub struct BacktestSettings {
    pub run_id: String,
    pub strategy_name: String,
    pub config_json: String,
    pub universe_name: String,
    pub alpha_name: String,
    pub symbols: Vec<String>,
    pub universe_filter: StrategyUniverseFilterConfig,
    pub alpha_components: Vec<StrategyAlphaComponentConfig>,
    pub alpha_conflict_resolution: StrategyAlphaConflictResolution,
    pub alpha_gate: Option<StrategyAlphaGateConfig>,
    pub symbol: String,
    pub account_id: String,
    pub order_qty: Decimal,
    pub max_abs_qty: Decimal,
    pub max_exposure: Decimal,
    pub max_drawdown: Decimal,
    pub max_leverage: Decimal,
    pub max_margin_used: Decimal,
    pub trading_halted: bool,
    pub allow_short: bool,
    pub shortable_symbols: BTreeSet<String>,
    pub initial_equity: Decimal,
    pub daily_loss_limit: Option<Decimal>,
    pub max_order_attempts_per_day: Option<u32>,
    pub max_order_failures_per_day: Option<u32>,
    pub max_price_deviation_bps: Option<Decimal>,
    pub max_market_data_age_ms: Option<u64>,
    pub max_consecutive_strategy_losses: Option<u32>,
    pub max_consecutive_strategy_errors: Option<u32>,
    pub trading_session: Option<algorithm::TradingSessionWindow>,
    pub fast_window: usize,
    pub slow_window: usize,
    pub logging: LogWriterSettings,
}

impl BacktestSettings {
    pub fn sample() -> Self {
        Self {
            run_id: "sample-ma-cross".to_string(),
            strategy_name: "moving_average_cross".to_string(),
            config_json: "{}".to_string(),
            universe_name: "static".to_string(),
            alpha_name: "moving_average_cross".to_string(),
            symbols: vec!["US:NASDAQ:AAPL:EQUITY".to_string()],
            universe_filter: StrategyUniverseFilterConfig::default(),
            alpha_components: Vec::new(),
            alpha_conflict_resolution: StrategyAlphaConflictResolution::HighestConfidence,
            alpha_gate: None,
            symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
            account_id: "backtest".to_string(),
            order_qty: Decimal::ONE,
            max_abs_qty: Decimal::from(100),
            max_exposure: Decimal::from(1_000_000),
            max_drawdown: Decimal::ONE,
            max_leverage: Decimal::from(10),
            max_margin_used: Decimal::ZERO,
            trading_halted: false,
            allow_short: false,
            shortable_symbols: BTreeSet::new(),
            initial_equity: Decimal::from(100_000),
            daily_loss_limit: None,
            max_order_attempts_per_day: None,
            max_order_failures_per_day: None,
            max_price_deviation_bps: None,
            max_market_data_age_ms: None,
            max_consecutive_strategy_losses: None,
            max_consecutive_strategy_errors: None,
            trading_session: None,
            fast_window: 2,
            slow_window: 3,
            logging: LogWriterSettings::default(),
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
        let symbol = self.primary_symbol();
        let slices = bars
            .into_iter()
            .map(|bar| MarketSlice::single(symbol.clone(), bar))
            .collect::<Vec<_>>();
        self.run_market_slices(slices).await
    }

    pub async fn run_market_slices(
        &self,
        market_slices: Vec<MarketSlice>,
    ) -> anyhow::Result<BacktestSummary> {
        let log_scope = self.db.clone().and_then(|db| {
            self.settings.logging.enabled.then(|| {
                BacktestLogScope::new(
                    db,
                    self.settings.run_id.clone(),
                    self.settings.logging.clone(),
                )
            })
        });
        if log_scope.is_some() {
            tracing::info!(
                run_id = %self.settings.run_id,
                mode = "backtest",
                symbol = %self.primary_symbol(),
                "backtest run started"
            );
        }
        let registry = StrategyRegistry;
        let assembly = registry.assemble_alpha(
            StrategyAssemblyConfig {
                strategy_name: self.settings.strategy_name.clone(),
                universe_name: self.settings.universe_name.clone(),
                alpha_name: self.settings.alpha_name.clone(),
                symbols: self.settings.assembly_symbols(),
                universe_filter: self.settings.universe_filter.clone(),
                alpha_components: self.settings.alpha_components.clone(),
                alpha_conflict_resolution: self.settings.alpha_conflict_resolution,
                alpha_gate: self.settings.alpha_gate.clone(),
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
                allow_short: self.settings.allow_short,
                shortable_symbols: self.settings.shortable_symbols.clone(),
                initial_cash: self.settings.initial_equity,
                daily_loss_limit: self.settings.daily_loss_limit,
                max_order_attempts_per_day: self.settings.max_order_attempts_per_day,
                max_order_failures_per_day: self.settings.max_order_failures_per_day,
                max_price_deviation_bps: self.settings.max_price_deviation_bps,
                max_market_data_age_ms: self.settings.max_market_data_age_ms,
                max_consecutive_strategy_losses: self.settings.max_consecutive_strategy_losses,
                max_consecutive_strategy_errors: self.settings.max_consecutive_strategy_errors,
                trading_session: self.settings.trading_session.clone(),
            },
            assembly.universe,
            assembly.alpha,
        );
        if let Some(event_bus) = &self.event_bus {
            engine.set_event_bus(event_bus.clone());
        }
        let mut signals = 0;
        let mut orders = 0;
        let started_at_ms = market_slices.first().map_or(0, |slice| slice.ts_ms);
        let mut ended_at_ms = started_at_ms;

        let result = async {
            for market_slice in market_slices {
                ended_at_ms = market_slice.ts_ms;
                let step = engine.on_market_slice(market_slice.clone())?;
                for decision in step.decisions {
                    signals += 1;
                    let Some(order) = decision.order else {
                        continue;
                    };
                    let bar = market_slice.bar(&order.symbol).ok_or_else(|| {
                        anyhow::anyhow!("missing market bar for generated order {}", order.symbol)
                    })?;
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

                    if log_scope.is_some() {
                        tracing::info!(
                            run_id = %self.settings.run_id,
                            order_id = %decision.order_id,
                            fill_id = %decision.fill_id,
                            symbol = %order.symbol,
                            qty = %order.qty,
                            price = %bar.close,
                            ts_ms = bar.ts_ms,
                            category = "trading",
                            broker_order_id = %broker_order_id,
                            "backtest order filled"
                        );
                    }

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
                    config_json: self.settings.config_json.clone(),
                })
                .await?;

                let snapshot = engine.snapshot_from_prices()?;
                for position in snapshot.positions {
                    db.record_backtest_position(BacktestPositionCommand {
                        run_id: self.settings.run_id.clone(),
                        account_id: self.settings.account_id.clone(),
                        symbol: position.symbol,
                        qty: position.qty,
                        avg_price: position.avg_price,
                        updated_at_ms: ended_at_ms,
                    })
                    .await?;
                }
            }

            Ok(BacktestSummary { signals, orders })
        }
        .await;
        match &result {
            Ok(summary) if log_scope.is_some() => tracing::info!(
                run_id = %self.settings.run_id,
                signals = summary.signals as u64,
                orders = summary.orders as u64,
                "backtest run completed"
            ),
            Err(error) if log_scope.is_some() => tracing::error!(
                run_id = %self.settings.run_id,
                error = %error,
                "backtest run failed"
            ),
            _ => {}
        }
        if let Some(log_scope) = log_scope {
            log_scope.shutdown().await;
        }
        result
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

struct BacktestLogScope {
    _guard: tracing::subscriber::DefaultGuard,
    writer: LogWriter<DbSystemLogSink>,
}

impl BacktestLogScope {
    fn new(db: Db, run_id: String, settings: LogWriterSettings) -> Self {
        let writer = LogWriter::new_with_metrics(
            DbSystemLogSink::new(db),
            settings.buffer_size,
            settings.batch_size,
            settings.flush_interval_ms,
            settings.metrics.clone(),
        );
        let subscriber = tracing_subscriber::registry().with(
            SystemLogLayer::new(writer.sender(), Some(run_id))
                .with_settings(settings)
                .with_metrics(writer.metrics()),
        );
        let guard = tracing::subscriber::set_default(subscriber);
        Self {
            _guard: guard,
            writer,
        }
    }

    async fn shutdown(self) {
        self.writer.shutdown().await;
    }
}
