#![forbid(unsafe_code)]

pub mod binance;
pub mod ibkr;

pub use binance::{BinancePaperOrderClient, BinancePaperOrderExecutor, binance_spot_symbol};
pub use ibkr::{
    IbkrPaperGatewayOrderClient, IbkrPaperOrderClient, IbkrPaperOrderExecutor, ibkr_stock_symbol,
};

use algorithm::{
    AccountSnapshot, AlgorithmDecision, AlgorithmEngine, AlgorithmEngineSettings,
    ConfiguredTradingScheduleProvider, ContractAccountingBook, ContractFill, ContractPosition,
    EngineEvent, ExecutionReport, FundingRateEvent, SimulatedContractAccounting,
    TradingDaySchedule, TradingDaySession, TradingScheduleProvider, TradingSessionWindow,
    trading_day_for_timestamp,
};
use async_trait::async_trait;
use backtest::BacktestSummary;
use broker::{SimulatedBrokerSettings, simulate_market_fill};
use data::{Bar, MarketSlice};
use events::{EventBus, LogWriter, LogWriterSettings, SystemLogLayer};
use market_rules::{ConfiguredMarketRuleProvider, FeeRuleEngine, MarketRuleSet};
use runtime::CancellationFlag;
use rust_decimal::Decimal;
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
    str::FromStr,
    sync::{Arc, RwLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use storage::{
    CryptoPositionCommand, Db, DbSystemLogSink, PaperExecutionCommand, PaperFailedOrderCommand,
    PaperFinalStateCommand, PaperOrderCommand, PaperPortfolioSnapshotCommand, PositionCommand,
    RuntimeEventCommand, StrategyRunStartCommand,
};
use strategies::{
    StrategyAlphaComponentConfig, StrategyAlphaConflictResolution, StrategyAlphaGateConfig,
    StrategyAssemblyConfig, StrategyRegistry, StrategyRuntimeMode, StrategyUniverseFilterConfig,
};
use tokio::sync::mpsc;
use tracing_subscriber::prelude::*;
use trader_core::OrderRequest;

pub struct PaperRuntime {
    db: Db,
    settings: PaperSettings,
    executor: Box<dyn PaperOrderExecutor>,
    event_bus: Option<EventBus>,
}

#[derive(Debug, Clone)]
pub struct PaperSettings {
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
    pub max_order_qty: Decimal,
    pub max_order_notional: Decimal,
    pub min_cash_after_order: Decimal,
    pub max_exposure: Decimal,
    pub max_drawdown: Decimal,
    pub max_leverage: Decimal,
    pub max_margin_used: Decimal,
    pub trading_halted: bool,
    pub allow_short: bool,
    pub shortable_symbols: BTreeSet<String>,
    pub initial_cash: Decimal,
    pub daily_loss_limit: Option<Decimal>,
    pub max_order_attempts_per_day: Option<u32>,
    pub max_order_failures_per_day: Option<u32>,
    pub max_price_deviation_bps: Option<Decimal>,
    pub max_market_data_age_ms: Option<u64>,
    pub max_consecutive_strategy_losses: Option<u32>,
    pub max_consecutive_strategy_errors: Option<u32>,
    pub trading_session: Option<TradingSessionWindow>,
    pub base_currency: String,
    pub slippage_bps: Decimal,
    pub fee_bps: Decimal,
    pub simulated_funding_rate: Option<Decimal>,
    pub fast_window: usize,
    pub slow_window: usize,
    pub bar_delay_ms: u64,
    pub logging: LogWriterSettings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaperRunError {
    Cancelled,
}

impl fmt::Display for PaperRunError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("paper run cancelled"),
        }
    }
}

impl Error for PaperRunError {}

fn payload_response(payload_json: &str) -> serde_json::Value {
    serde_json::from_str(payload_json)
        .unwrap_or_else(|_| serde_json::Value::String(payload_json.to_string()))
}

impl PaperSettings {
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
            account_id: "paper".to_string(),
            order_qty: Decimal::ONE,
            max_abs_qty: Decimal::from(100),
            max_order_qty: Decimal::from(100),
            max_order_notional: Decimal::from(1_000_000),
            min_cash_after_order: Decimal::ZERO,
            max_exposure: Decimal::from(1_000_000),
            max_drawdown: Decimal::ONE,
            max_leverage: Decimal::from(10),
            max_margin_used: Decimal::ZERO,
            trading_halted: false,
            allow_short: false,
            shortable_symbols: BTreeSet::new(),
            initial_cash: Decimal::from(100_000),
            daily_loss_limit: None,
            max_order_attempts_per_day: None,
            max_order_failures_per_day: None,
            max_price_deviation_bps: None,
            max_market_data_age_ms: None,
            max_consecutive_strategy_losses: None,
            max_consecutive_strategy_errors: None,
            trading_session: None,
            base_currency: "USD".to_string(),
            slippage_bps: Decimal::ZERO,
            fee_bps: Decimal::ZERO,
            simulated_funding_rate: None,
            fast_window: 2,
            slow_window: 3,
            bar_delay_ms: 0,
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

impl PaperRuntime {
    pub fn new(db: Db, settings: PaperSettings) -> Self {
        let executor = Box::new(SimulatedPaperOrderExecutor {
            client_order_prefix: settings.run_id.clone(),
            settings: SimulatedBrokerSettings {
                slippage_bps: settings.slippage_bps,
                fee_bps: settings.fee_bps,
            },
        });
        Self {
            db,
            settings,
            executor,
            event_bus: None,
        }
    }

    pub fn new_with_executor(
        db: Db,
        settings: PaperSettings,
        executor: Box<dyn PaperOrderExecutor>,
    ) -> Self {
        Self {
            db,
            settings,
            executor,
            event_bus: None,
        }
    }

    pub fn new_with_event_bus(db: Db, settings: PaperSettings, event_bus: EventBus) -> Self {
        let executor = Box::new(SimulatedPaperOrderExecutor {
            client_order_prefix: settings.run_id.clone(),
            settings: SimulatedBrokerSettings {
                slippage_bps: settings.slippage_bps,
                fee_bps: settings.fee_bps,
            },
        });
        Self {
            db,
            settings,
            executor,
            event_bus: Some(event_bus),
        }
    }

    pub fn with_event_bus(mut self, event_bus: EventBus) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    pub async fn run_bars(&self, bars: Vec<Bar>) -> anyhow::Result<BacktestSummary> {
        self.run_bars_with_cancel(bars, CancellationFlag::default())
            .await
    }

    pub async fn run_market_slices(
        &self,
        market_slices: Vec<MarketSlice>,
    ) -> anyhow::Result<BacktestSummary> {
        self.run_market_slices_with_cancel(market_slices, CancellationFlag::default())
            .await
    }

    pub async fn run_bars_with_cancel(
        &self,
        bars: Vec<Bar>,
        cancel: CancellationFlag,
    ) -> anyhow::Result<BacktestSummary> {
        let symbol = self.primary_symbol();
        let market_slices = bars
            .into_iter()
            .map(|bar| MarketSlice::single(symbol.clone(), bar))
            .collect::<Vec<_>>();
        self.run_market_slices_with_cancel(market_slices, cancel)
            .await
    }

    pub async fn run_market_slices_with_cancel(
        &self,
        market_slices: Vec<MarketSlice>,
        cancel: CancellationFlag,
    ) -> anyhow::Result<BacktestSummary> {
        let log_scope = PaperLogScope::new(
            self.db.clone(),
            self.settings.run_id.clone(),
            self.settings.logging.clone(),
        );
        tracing::info!(
            run_id = %self.settings.run_id,
            mode = "paper",
            symbol = %self.primary_symbol(),
            "paper run started"
        );
        let mut session = PaperRunSession::new_for_market_slices(self, &market_slices).await?;
        let result = async {
            for market_slice in market_slices {
                self.wait_before_bar(&cancel).await?;
                session.process_market_slice(market_slice).await?;
            }
            session.finish().await
        }
        .await;
        match &result {
            Ok(summary) => tracing::info!(
                run_id = %self.settings.run_id,
                signals = summary.signals as u64,
                orders = summary.orders as u64,
                "paper run completed"
            ),
            Err(error) => tracing::error!(
                run_id = %self.settings.run_id,
                error = %error,
                "paper run failed"
            ),
        }
        if let Some(log_scope) = log_scope {
            log_scope.shutdown().await;
        }
        result
    }

    pub async fn run_bar_stream_with_cancel(
        &self,
        mut bars: mpsc::Receiver<Bar>,
        cancel: CancellationFlag,
    ) -> anyhow::Result<BacktestSummary> {
        let log_scope = PaperLogScope::new(
            self.db.clone(),
            self.settings.run_id.clone(),
            self.settings.logging.clone(),
        );
        tracing::info!(
            run_id = %self.settings.run_id,
            mode = "paper",
            symbol = %self.primary_symbol(),
            "paper stream started"
        );
        let mut session = PaperRunSession::new(self).await?;
        let result = async {
            while let Some(bar) = bars.recv().await {
                self.wait_before_bar(&cancel).await?;
                session
                    .process_market_slice(MarketSlice::single(self.primary_symbol(), bar))
                    .await?;
            }
            session.finish().await
        }
        .await;
        match &result {
            Ok(summary) => tracing::info!(
                run_id = %self.settings.run_id,
                signals = summary.signals as u64,
                orders = summary.orders as u64,
                "paper stream completed"
            ),
            Err(error) => tracing::error!(
                run_id = %self.settings.run_id,
                error = %error,
                "paper stream failed"
            ),
        }
        if let Some(log_scope) = log_scope {
            log_scope.shutdown().await;
        }
        result
    }

    pub async fn run_market_slice_stream_with_cancel(
        &self,
        mut market_slices: mpsc::Receiver<MarketSlice>,
        cancel: CancellationFlag,
    ) -> anyhow::Result<BacktestSummary> {
        let log_scope = PaperLogScope::new(
            self.db.clone(),
            self.settings.run_id.clone(),
            self.settings.logging.clone(),
        );
        tracing::info!(
            run_id = %self.settings.run_id,
            mode = "paper",
            symbol = %self.primary_symbol(),
            "paper market slice stream started"
        );
        let mut session = PaperRunSession::new(self).await?;
        let result = async {
            while let Some(market_slice) = market_slices.recv().await {
                self.wait_before_bar(&cancel).await?;
                session.process_market_slice(market_slice).await?;
            }
            session.finish().await
        }
        .await;
        match &result {
            Ok(summary) => tracing::info!(
                run_id = %self.settings.run_id,
                signals = summary.signals as u64,
                orders = summary.orders as u64,
                "paper market slice stream completed"
            ),
            Err(error) => tracing::error!(
                run_id = %self.settings.run_id,
                error = %error,
                "paper market slice stream failed"
            ),
        }
        if let Some(log_scope) = log_scope {
            log_scope.shutdown().await;
        }
        result
    }

    async fn wait_before_bar(&self, cancel: &CancellationFlag) -> anyhow::Result<()> {
        if cancel.is_cancelled() {
            return Err(PaperRunError::Cancelled.into());
        }
        if self.settings.bar_delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(self.settings.bar_delay_ms)).await;
        }
        if cancel.is_cancelled() {
            return Err(PaperRunError::Cancelled.into());
        }
        Ok(())
    }

    fn primary_symbol(&self) -> String {
        self.settings
            .assembly_symbols()
            .into_iter()
            .next()
            .unwrap_or_else(|| self.settings.symbol.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutedPaperOrder {
    pub client_order_id: String,
    pub broker_order_id: String,
    pub status: String,
    pub price: Decimal,
    pub qty: Decimal,
    pub fee: Decimal,
}

#[async_trait]
pub trait PaperOrderExecutor: Send + Sync {
    fn client_order_id(&self, run_id: &str, order_number: usize) -> String;

    fn uses_runtime_fee_rules(&self) -> bool {
        false
    }

    async fn execute_order(
        &self,
        order: OrderRequest,
        mark_price: Decimal,
        order_number: usize,
    ) -> anyhow::Result<ExecutedPaperOrder>;
}

struct SimulatedPaperOrderExecutor {
    client_order_prefix: String,
    settings: SimulatedBrokerSettings,
}

#[async_trait]
impl PaperOrderExecutor for SimulatedPaperOrderExecutor {
    fn client_order_id(&self, _run_id: &str, order_number: usize) -> String {
        format!("{}-order-{}", self.client_order_prefix, order_number)
    }

    fn uses_runtime_fee_rules(&self) -> bool {
        true
    }

    async fn execute_order(
        &self,
        order: OrderRequest,
        mark_price: Decimal,
        order_number: usize,
    ) -> anyhow::Result<ExecutedPaperOrder> {
        let fill = simulate_market_fill(order, mark_price, self.settings.clone())?;
        Ok(ExecutedPaperOrder {
            client_order_id: self.client_order_id("", order_number),
            broker_order_id: format!("simulated-{order_number}"),
            status: "FILLED".to_string(),
            price: fill.price,
            qty: fill.qty,
            fee: fill.fee,
        })
    }
}

struct PaperRunSession<'a> {
    runtime: &'a PaperRuntime,
    engine: AlgorithmEngine,
    dynamic_trading_schedule: Option<DynamicTradingScheduleProvider>,
    signals: usize,
    orders: usize,
    started_at_ms: Option<i64>,
    ended_at_ms: i64,
    last_snapshot: AccountSnapshot,
    contract_accounting: SimulatedContractAccounting,
    fee_rule_engine: FeeRuleEngine,
}

impl<'a> PaperRunSession<'a> {
    async fn new(runtime: &'a PaperRuntime) -> anyhow::Result<Self> {
        let trading_schedule = DynamicTradingScheduleProvider::default();
        Self::new_with_trading_schedule(
            runtime,
            Some(trading_schedule.clone().boxed()),
            Some(trading_schedule),
        )
        .await
    }

    async fn new_for_market_slices(
        runtime: &'a PaperRuntime,
        market_slices: &[MarketSlice],
    ) -> anyhow::Result<Self> {
        let default_timezone = runtime
            .settings
            .trading_session
            .as_ref()
            .map(|window| window.timezone.as_str())
            .unwrap_or("UTC");
        let trading_schedule = load_configured_trading_schedule(
            &runtime.db,
            &runtime.settings.assembly_symbols(),
            market_slices,
            default_timezone,
        )
        .await?;
        Self::new_with_trading_schedule(runtime, Some(Box::new(trading_schedule)), None).await
    }

    async fn new_with_trading_schedule(
        runtime: &'a PaperRuntime,
        trading_schedule: Option<Box<dyn TradingScheduleProvider + Send + Sync>>,
        dynamic_trading_schedule: Option<DynamicTradingScheduleProvider>,
    ) -> anyhow::Result<Self> {
        let registry = StrategyRegistry;
        let assembly = registry.assemble_alpha(
            StrategyAssemblyConfig {
                strategy_name: runtime.settings.strategy_name.clone(),
                universe_name: runtime.settings.universe_name.clone(),
                alpha_name: runtime.settings.alpha_name.clone(),
                symbols: runtime.settings.assembly_symbols(),
                universe_filter: runtime.settings.universe_filter.clone(),
                alpha_components: runtime.settings.alpha_components.clone(),
                alpha_conflict_resolution: runtime.settings.alpha_conflict_resolution,
                alpha_gate: runtime.settings.alpha_gate.clone(),
                fast_window: runtime.settings.fast_window,
                slow_window: runtime.settings.slow_window,
            },
            StrategyRuntimeMode::Paper,
        )?;
        let as_of_ms = current_unix_ms()?;
        let market_rules = load_configured_market_rules(
            &runtime.db,
            &runtime.settings.assembly_symbols(),
            as_of_ms,
        )
        .await?;
        let fee_rule_seed = runtime
            .db
            .load_market_fee_rules_with_account_volume(
                &runtime.settings.assembly_symbols(),
                &runtime.settings.account_id,
                as_of_ms,
            )
            .await?;
        let fee_rule_engine = FeeRuleEngine::with_volume_by_rule(
            fee_rule_seed.rules_by_symbol,
            fee_rule_seed.volume_by_rule,
        );
        let engine_settings = AlgorithmEngineSettings {
            run_id: runtime.settings.run_id.clone(),
            mode: StrategyRuntimeMode::Paper,
            account_id: runtime.settings.account_id.clone(),
            symbol: assembly.primary_symbol.clone(),
            order_qty: runtime.settings.order_qty,
            max_abs_qty: runtime.settings.max_abs_qty,
            max_order_qty: runtime.settings.max_order_qty,
            max_order_notional: runtime.settings.max_order_notional,
            min_cash_after_order: runtime.settings.min_cash_after_order,
            max_exposure: runtime.settings.max_exposure,
            max_drawdown: runtime.settings.max_drawdown,
            max_leverage: runtime.settings.max_leverage,
            max_margin_used: runtime.settings.max_margin_used,
            trading_halted: runtime.settings.trading_halted,
            allow_short: runtime.settings.allow_short,
            shortable_symbols: runtime.settings.shortable_symbols.clone(),
            initial_cash: runtime.settings.initial_cash,
            daily_loss_limit: runtime.settings.daily_loss_limit,
            max_order_attempts_per_day: runtime.settings.max_order_attempts_per_day,
            max_order_failures_per_day: runtime.settings.max_order_failures_per_day,
            max_price_deviation_bps: runtime.settings.max_price_deviation_bps,
            max_market_data_age_ms: runtime.settings.max_market_data_age_ms,
            max_consecutive_strategy_losses: runtime.settings.max_consecutive_strategy_losses,
            max_consecutive_strategy_errors: runtime.settings.max_consecutive_strategy_errors,
            trading_session: runtime.settings.trading_session.clone(),
        };
        let mut engine = if let Some(trading_schedule) = trading_schedule {
            AlgorithmEngine::new_with_universe_market_rules_and_trading_schedule(
                engine_settings,
                assembly.universe,
                assembly.alpha,
                Box::new(market_rules),
                trading_schedule,
            )
        } else {
            AlgorithmEngine::new_with_universe_and_market_rules(
                engine_settings,
                assembly.universe,
                assembly.alpha,
                Box::new(market_rules),
            )
        };
        if let Some(event_bus) = &runtime.event_bus {
            engine.set_event_bus(event_bus.clone());
        }
        let last_snapshot = engine.snapshot(Decimal::ONE)?;

        Ok(Self {
            runtime,
            engine,
            dynamic_trading_schedule,
            signals: 0,
            orders: 0,
            started_at_ms: None,
            ended_at_ms: 0,
            last_snapshot,
            contract_accounting: SimulatedContractAccounting::new(
                runtime.settings.account_id.clone(),
                runtime.settings.max_leverage,
            ),
            fee_rule_engine,
        })
    }

    async fn process_market_slice(&mut self, market_slice: MarketSlice) -> anyhow::Result<()> {
        self.refresh_dynamic_trading_schedule(&market_slice).await?;
        if self.started_at_ms.is_none() {
            let settings = &self.runtime.settings;
            self.runtime
                .db
                .start_strategy_run(StrategyRunStartCommand {
                    run_id: settings.run_id.clone(),
                    name: settings.strategy_name.clone(),
                    mode: "paper".to_string(),
                    started_at_ms: market_slice.ts_ms,
                    config: payload_response(&settings.config_json),
                })
                .await?;
            self.started_at_ms = Some(market_slice.ts_ms);
        }
        self.ended_at_ms = market_slice.ts_ms;
        let step = self.engine.on_market_slice(market_slice.clone())?;
        self.last_snapshot = step.snapshot.clone();
        for decision in step.decisions {
            self.signals += 1;
            self.persist_engine_events(&decision.events).await?;
            let settings = &self.runtime.settings;
            let Some(order) = decision.order.clone() else {
                continue;
            };
            let bar = market_slice.bar(&order.symbol).ok_or_else(|| {
                anyhow::anyhow!("missing market bar for generated order {}", order.symbol)
            })?;
            let client_order_id = self
                .runtime
                .executor
                .client_order_id(&settings.run_id, decision.order_number);
            self.persist_submitted_order(&decision, &order, &client_order_id, bar)
                .await?;
            let mut fill = match self
                .runtime
                .executor
                .execute_order(order.clone(), bar.close, decision.order_number)
                .await
            {
                Ok(fill) => fill,
                Err(error) => {
                    self.engine.record_order_failure();
                    let message = error.to_string();
                    self.persist_failed_order(&decision, &order, &client_order_id, bar, &message)
                        .await?;
                    return Err(error);
                }
            };
            self.apply_runtime_fee_rule(&order, &mut fill);
            let applied = self.engine.apply_execution(
                &order,
                &ExecutionReport {
                    broker_order_id: fill.broker_order_id.clone(),
                    status: fill.status.clone(),
                    price: fill.price,
                    qty: fill.qty,
                    fee: fill.fee,
                },
                bar.ts_ms,
            )?;
            self.persist_engine_events(&applied.events).await?;
            self.last_snapshot = applied.snapshot;
            self.persist_execution_result(&decision, &order, &client_order_id, &fill, bar)
                .await?;
            self.persist_contract_fill(&order, &fill, bar).await?;
            self.orders += 1;
        }

        self.settle_contract_funding(&market_slice).await?;
        self.persist_snapshot(market_slice.ts_ms).await
    }

    async fn refresh_dynamic_trading_schedule(
        &self,
        market_slice: &MarketSlice,
    ) -> anyhow::Result<()> {
        let Some(provider) = &self.dynamic_trading_schedule else {
            return Ok(());
        };
        let default_timezone = self
            .runtime
            .settings
            .trading_session
            .as_ref()
            .map(|window| window.timezone.as_str())
            .unwrap_or("UTC");
        for symbol in self.runtime.settings.assembly_symbols() {
            let Some((market, _exchange, _asset_class)) = market_rule_symbol_parts(&symbol) else {
                continue;
            };
            let trading_day = trading_day_for_timestamp(default_timezone, market_slice.ts_ms)?;
            let schedule = load_trading_day_schedule(
                &self.runtime.db,
                &market,
                &trading_day,
                default_timezone,
            )
            .await?;
            provider.set_schedule(symbol, trading_day, schedule)?;
        }
        Ok(())
    }

    fn apply_runtime_fee_rule(&mut self, order: &OrderRequest, fill: &mut ExecutedPaperOrder) {
        if !self.runtime.executor.uses_runtime_fee_rules() {
            return;
        }
        if let Some(breakdown) =
            self.fee_rule_engine
                .apply_fill(&order.symbol, order.order_type, fill.price, fill.qty)
        {
            fill.fee = breakdown.total;
        }
    }

    async fn persist_submitted_order(
        &self,
        decision: &AlgorithmDecision,
        order: &OrderRequest,
        client_order_id: &str,
        bar: &Bar,
    ) -> anyhow::Result<()> {
        tracing::info!(
            run_id = %self.runtime.settings.run_id,
            order_id = %decision.order_id,
            symbol = %order.symbol,
            qty = %order.qty,
            side = ?order.side,
            ts_ms = bar.ts_ms,
            category = "trading",
            client_order_id = client_order_id,
            "paper order submitted"
        );
        self.runtime
            .db
            .record_paper_order_submitted(PaperOrderCommand {
                run_id: self.runtime.settings.run_id.clone(),
                order_id: decision.order_id.clone(),
                client_order_id: client_order_id.to_string(),
                order: order.clone(),
                ts_ms: bar.ts_ms,
            })
            .await?;
        Ok(())
    }

    async fn persist_failed_order(
        &self,
        decision: &AlgorithmDecision,
        order: &OrderRequest,
        client_order_id: &str,
        bar: &Bar,
        error: &str,
    ) -> anyhow::Result<()> {
        tracing::error!(
            run_id = %self.runtime.settings.run_id,
            order_id = %decision.order_id,
            symbol = %order.symbol,
            qty = %order.qty,
            side = ?order.side,
            ts_ms = bar.ts_ms,
            category = "trading",
            client_order_id = client_order_id,
            error = error,
            "paper order failed"
        );
        self.runtime
            .db
            .record_paper_order_failed(PaperFailedOrderCommand {
                run_id: self.runtime.settings.run_id.clone(),
                order_id: decision.order_id.clone(),
                client_order_id: client_order_id.to_string(),
                order: order.clone(),
                error: error.to_string(),
                ts_ms: bar.ts_ms,
            })
            .await?;
        Ok(())
    }

    async fn persist_execution_result(
        &self,
        decision: &AlgorithmDecision,
        order: &OrderRequest,
        client_order_id: &str,
        fill: &ExecutedPaperOrder,
        bar: &Bar,
    ) -> anyhow::Result<()> {
        tracing::info!(
            run_id = %self.runtime.settings.run_id,
            order_id = %decision.order_id,
            fill_id = %decision.fill_id,
            symbol = %order.symbol,
            qty = %fill.qty,
            price = %fill.price,
            fee = %fill.fee,
            status = %fill.status,
            ts_ms = bar.ts_ms,
            category = "trading",
            client_order_id = client_order_id,
            broker_order_id = %fill.broker_order_id,
            "paper order filled"
        );
        self.runtime
            .db
            .record_paper_execution_result(PaperExecutionCommand {
                run_id: self.runtime.settings.run_id.clone(),
                order_id: decision.order_id.clone(),
                fill_id: decision.fill_id.clone(),
                client_order_id: client_order_id.to_string(),
                order: order.clone(),
                broker_order_id: fill.broker_order_id.clone(),
                status: fill.status.clone(),
                price: fill.price,
                qty: fill.qty,
                fee: fill.fee,
                ts_ms: bar.ts_ms,
            })
            .await?;
        Ok(())
    }

    async fn persist_contract_fill(
        &mut self,
        order: &OrderRequest,
        fill: &ExecutedPaperOrder,
        bar: &Bar,
    ) -> anyhow::Result<()> {
        if fill.qty == Decimal::ZERO {
            return Ok(());
        }
        let Some((exchange, asset_class)) = contract_symbol_parts(&order.symbol) else {
            return Ok(());
        };

        self.contract_accounting
            .on_fill(&ContractFill {
                run_id: self.runtime.settings.run_id.clone(),
                account_id: self.runtime.settings.account_id.clone(),
                exchange,
                symbol: order.symbol.clone(),
                asset_class,
                margin_mode: "cross".to_string(),
                side: order.side,
                qty: fill.qty,
                price: fill.price,
                fee: fill.fee,
                ts_ms: bar.ts_ms,
            })
            .await?;
        self.persist_contract_positions_for_symbol(&order.symbol)
            .await
    }

    async fn settle_contract_funding(&mut self, market_slice: &MarketSlice) -> anyhow::Result<()> {
        let Some(funding_rate) = self.runtime.settings.simulated_funding_rate else {
            return Ok(());
        };

        for (symbol, bar) in market_slice.iter() {
            let Some((exchange, _asset_class)) = contract_symbol_parts(symbol) else {
                continue;
            };
            self.contract_accounting
                .on_funding(&FundingRateEvent {
                    exchange,
                    symbol: symbol.to_string(),
                    funding_time_ms: bar.ts_ms,
                    funding_rate,
                    mark_price: bar.close,
                })
                .await?;
            self.persist_contract_positions_for_symbol(symbol).await?;
        }
        Ok(())
    }

    async fn persist_contract_positions_for_symbol(&self, symbol: &str) -> anyhow::Result<()> {
        let positions = self
            .contract_accounting
            .positions()
            .filter(|position| position.symbol == symbol)
            .cloned()
            .collect::<Vec<_>>();

        for position in positions {
            self.persist_contract_position(position).await?;
        }
        Ok(())
    }

    async fn persist_contract_position(&self, position: ContractPosition) -> anyhow::Result<()> {
        self.runtime
            .db
            .record_crypto_position(CryptoPositionCommand {
                run_id: position.run_id,
                account_id: position.account_id,
                exchange: position.exchange,
                symbol: position.symbol,
                asset_class: position.asset_class,
                margin_mode: position.margin_mode,
                position_side: position.position_side.as_str().to_string(),
                leverage: position.leverage,
                qty: position.qty,
                avg_price: position.avg_price,
                margin_used: position.margin_used,
                funding_fee: position.funding_fee,
                realized_pnl: position.realized_pnl,
                unrealized_pnl: position.unrealized_pnl,
                updated_at_ms: position.updated_at_ms,
            })
            .await?;
        Ok(())
    }

    async fn persist_snapshot(&mut self, ts_ms: i64) -> anyhow::Result<()> {
        let settings = &self.runtime.settings;
        self.last_snapshot = self.engine.snapshot_from_prices()?;
        self.runtime
            .db
            .record_paper_portfolio_snapshot(PaperPortfolioSnapshotCommand {
                run_id: settings.run_id.clone(),
                account_id: settings.account_id.clone(),
                ts_ms,
                base_currency: settings.base_currency.clone(),
                cash: self.last_snapshot.cash,
                market_value: self.last_snapshot.market_value,
                equity: self.last_snapshot.equity,
                realized_pnl: self.last_snapshot.realized_pnl,
                unrealized_pnl: self.last_snapshot.unrealized_pnl,
                positions: self
                    .last_snapshot
                    .positions
                    .iter()
                    .map(|position| PositionCommand {
                        run_id: settings.run_id.clone(),
                        account_id: settings.account_id.clone(),
                        symbol: position.symbol.clone(),
                        qty: position.qty,
                        avg_price: position.avg_price,
                        updated_at_ms: ts_ms,
                    })
                    .collect(),
            })
            .await?;
        Ok(())
    }

    async fn persist_engine_events(&self, events: &[EngineEvent]) -> anyhow::Result<()> {
        for event in events {
            self.runtime
                .db
                .record_runtime_event(RuntimeEventCommand {
                    ts_ms: event.ts_ms,
                    source: self.runtime.settings.run_id.clone(),
                    category: event.category.clone(),
                    payload: event.payload.clone(),
                })
                .await?;
        }
        Ok(())
    }

    async fn finish(self) -> anyhow::Result<BacktestSummary> {
        let settings = &self.runtime.settings;
        let started_at_ms = self.started_at_ms.unwrap_or(0);
        self.runtime
            .db
            .complete_paper_run(PaperFinalStateCommand {
                run_id: settings.run_id.clone(),
                strategy_name: settings.strategy_name.clone(),
                account_id: settings.account_id.clone(),
                symbol: self.primary_symbol(),
                base_currency: settings.base_currency.clone(),
                started_at_ms,
                ended_at_ms: self.ended_at_ms,
                config_json: settings.config_json.clone(),
                cash: self.last_snapshot.cash,
                market_value: self.last_snapshot.market_value,
                equity: self.last_snapshot.equity,
                realized_pnl: self.last_snapshot.realized_pnl,
                unrealized_pnl: self.last_snapshot.unrealized_pnl,
                position_qty: self.last_snapshot.position_qty,
                position_avg_price: self.last_snapshot.position_avg_price,
                positions: self
                    .last_snapshot
                    .positions
                    .iter()
                    .map(|position| PositionCommand {
                        run_id: settings.run_id.clone(),
                        account_id: settings.account_id.clone(),
                        symbol: position.symbol.clone(),
                        qty: position.qty,
                        avg_price: position.avg_price,
                        updated_at_ms: self.ended_at_ms,
                    })
                    .collect(),
            })
            .await?;

        Ok(BacktestSummary {
            signals: self.signals,
            orders: self.orders,
        })
    }

    fn primary_symbol(&self) -> String {
        self.runtime
            .settings
            .assembly_symbols()
            .into_iter()
            .next()
            .unwrap_or_else(|| self.runtime.settings.symbol.clone())
    }
}

async fn load_configured_market_rules(
    db: &Db,
    symbols: &[String],
    as_of_ms: i64,
) -> anyhow::Result<ConfiguredMarketRuleProvider> {
    let mut rules_by_symbol = BTreeMap::new();
    for symbol in symbols {
        let Some((market, exchange, asset_class)) = market_rule_symbol_parts(symbol) else {
            continue;
        };
        let lot_rule = db
            .find_lot_size_rule(&market, &exchange, &asset_class, symbol, as_of_ms)
            .await?;
        let price_rule = db
            .find_price_limit_rule(&market, &exchange, &asset_class, symbol, as_of_ms)
            .await?;
        if lot_rule.is_none() && price_rule.is_none() {
            continue;
        }

        let mut rules = MarketRuleSet::for_symbol(symbol)?;
        if let Some(lot_rule) = lot_rule {
            rules.lot_size = parse_rule_decimal("lot_size", &lot_rule.lot_size)?;
            rules.min_qty = parse_rule_decimal("min_qty", &lot_rule.min_qty)?;
            rules.min_notional = parse_rule_decimal("min_notional", &lot_rule.min_notional)?;
        }
        if let Some(price_rule) = price_rule {
            rules.tick_size = parse_rule_decimal("tick_size", &price_rule.tick_size)?;
        }
        rules_by_symbol.insert(symbol.clone(), rules);
    }

    Ok(ConfiguredMarketRuleProvider::new(rules_by_symbol))
}

async fn load_configured_trading_schedule(
    db: &Db,
    symbols: &[String],
    market_slices: &[MarketSlice],
    default_timezone: &str,
) -> anyhow::Result<ConfiguredTradingScheduleProvider> {
    let mut schedules_by_symbol = BTreeMap::new();
    for symbol in symbols {
        let Some((market, _exchange, _asset_class)) = market_rule_symbol_parts(symbol) else {
            continue;
        };
        let mut schedules = Vec::new();
        for trading_day in trading_days_for_market_slices(market_slices, default_timezone)? {
            if let Some(schedule) =
                load_trading_day_schedule(db, &market, &trading_day, default_timezone).await?
            {
                schedules.push(schedule);
            }
        }
        if !schedules.is_empty() {
            schedules_by_symbol.insert(symbol.clone(), schedules);
        }
    }
    Ok(ConfiguredTradingScheduleProvider::new(schedules_by_symbol))
}

async fn load_trading_day_schedule(
    db: &Db,
    market: &str,
    trading_day: &str,
    default_timezone: &str,
) -> anyhow::Result<Option<TradingDaySchedule>> {
    let calendar = db.find_market_calendar(market, trading_day).await?;
    let sessions = db.list_trading_session_rules(market, trading_day).await?;
    if calendar.is_none() && sessions.is_empty() {
        return Ok(None);
    }
    let timezone = sessions
        .first()
        .map(|session| session.timezone.clone())
        .unwrap_or_else(|| default_timezone.to_string());
    let is_open = calendar.as_ref().map_or(true, |calendar| calendar.is_open);
    Ok(Some(TradingDaySchedule::new(
        trading_day.to_string(),
        timezone,
        is_open,
        sessions
            .into_iter()
            .map(|session| {
                TradingDaySession::new(
                    session.session_name,
                    session.timezone,
                    session.open_time,
                    session.close_time,
                )
            })
            .collect(),
    )))
}

#[derive(Clone, Default)]
struct DynamicTradingScheduleProvider {
    schedules_by_symbol: Arc<RwLock<BTreeMap<String, Vec<TradingDaySchedule>>>>,
}

impl DynamicTradingScheduleProvider {
    fn boxed(self) -> Box<dyn TradingScheduleProvider + Send + Sync> {
        Box::new(self)
    }

    fn set_schedule(
        &self,
        symbol: String,
        trading_day: String,
        schedule: Option<TradingDaySchedule>,
    ) -> anyhow::Result<()> {
        let mut schedules_by_symbol = self
            .schedules_by_symbol
            .write()
            .map_err(|_| anyhow::anyhow!("dynamic trading schedule lock poisoned"))?;
        let schedules = schedules_by_symbol.entry(symbol.clone()).or_default();
        schedules.retain(|existing| existing.trading_day != trading_day);
        if let Some(schedule) = schedule {
            schedules.push(schedule);
        }
        if schedules.is_empty() {
            schedules_by_symbol.remove(&symbol);
        }
        Ok(())
    }
}

impl TradingScheduleProvider for DynamicTradingScheduleProvider {
    fn check(&self, symbol: &str, ts_ms: i64) -> Option<anyhow::Result<()>> {
        let schedules = match self.schedules_by_symbol.read() {
            Ok(guard) => guard,
            Err(_) => {
                return Some(Err(anyhow::anyhow!(
                    "dynamic trading schedule lock poisoned"
                )));
            }
        }
        .get(symbol)?
        .clone();
        let provider = ConfiguredTradingScheduleProvider::new(BTreeMap::from([(
            symbol.to_string(),
            schedules,
        )]));
        provider.check(symbol, ts_ms)
    }
}

fn trading_days_for_market_slices(
    market_slices: &[MarketSlice],
    timezone: &str,
) -> anyhow::Result<BTreeSet<String>> {
    let mut trading_days = BTreeSet::new();
    for market_slice in market_slices {
        trading_days.insert(trading_day_for_timestamp(timezone, market_slice.ts_ms)?);
    }
    Ok(trading_days)
}

fn parse_rule_decimal(field: &str, value: &str) -> anyhow::Result<Decimal> {
    Decimal::from_str(value).map_err(|error| anyhow::anyhow!("invalid {field} {value}: {error}"))
}

fn market_rule_symbol_parts(symbol: &str) -> Option<(String, String, String)> {
    let mut parts = symbol.split(':');
    let market = parts.next()?;
    let exchange = parts.next()?;
    let _code = parts.next()?;
    let asset_class = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    Some((
        market.to_string(),
        exchange.to_string(),
        asset_class.to_string(),
    ))
}

fn current_unix_ms() -> anyhow::Result<i64> {
    let elapsed = SystemTime::now().duration_since(UNIX_EPOCH)?;
    i64::try_from(elapsed.as_millis()).map_err(|_| anyhow::anyhow!("current time is out of range"))
}

fn contract_symbol_parts(symbol: &str) -> Option<(String, String)> {
    let mut parts = symbol.split(':');
    let market = parts.next()?;
    let exchange = parts.next()?;
    let _code = parts.next()?;
    let asset_class = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    if market == "CRYPTO" && matches!(asset_class, "CRYPTO_PERP" | "CRYPTO_FUTURE") {
        Some((exchange.to_string(), asset_class.to_string()))
    } else {
        None
    }
}

struct PaperLogScope {
    _guard: tracing::subscriber::DefaultGuard,
    writer: LogWriter<DbSystemLogSink>,
}

impl PaperLogScope {
    fn new(db: Db, run_id: String, settings: LogWriterSettings) -> Option<Self> {
        if !settings.enabled {
            return None;
        }
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
        Some(Self {
            _guard: guard,
            writer,
        })
    }

    async fn shutdown(self) {
        self.writer.shutdown().await;
    }
}
