#![forbid(unsafe_code)]

pub mod binance;
pub mod ibkr;

pub use binance::{BinancePaperOrderClient, BinancePaperOrderExecutor, binance_spot_symbol};
pub use ibkr::{
    IbkrPaperGatewayOrderClient, IbkrPaperOrderClient, IbkrPaperOrderExecutor, ibkr_stock_symbol,
};

use algorithm::{
    AccountSnapshot, AlgorithmDecision, AlgorithmEngine, AlgorithmEngineSettings, EngineEvent,
    ExecutionReport,
};
use async_trait::async_trait;
use backtest::BacktestSummary;
use broker::{SimulatedBrokerSettings, simulate_market_fill};
use data::{Bar, MarketSlice};
use events::EventBus;
use runtime::CancellationFlag;
use rust_decimal::Decimal;
use std::{error::Error, fmt, time::Duration};
use storage::{
    Db, PaperExecutionCommand, PaperFailedOrderCommand, PaperFinalStateCommand, PaperOrderCommand,
    PaperPortfolioSnapshotCommand, PositionCommand, RuntimeEventCommand,
};
use strategies::{StrategyAssemblyConfig, StrategyRegistry, StrategyRuntimeMode};
use tokio::sync::mpsc;
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
    pub universe_name: String,
    pub alpha_name: String,
    pub symbols: Vec<String>,
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
    pub initial_cash: Decimal,
    pub base_currency: String,
    pub slippage_bps: Decimal,
    pub fee_bps: Decimal,
    pub fast_window: usize,
    pub slow_window: usize,
    pub bar_delay_ms: u64,
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

impl PaperSettings {
    pub fn sample() -> Self {
        Self {
            run_id: "sample-ma-cross".to_string(),
            strategy_name: "moving_average_cross".to_string(),
            universe_name: "static".to_string(),
            alpha_name: "moving_average_cross".to_string(),
            symbols: vec!["US:NASDAQ:AAPL:EQUITY".to_string()],
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
            initial_cash: Decimal::from(100_000),
            base_currency: "USD".to_string(),
            slippage_bps: Decimal::ZERO,
            fee_bps: Decimal::ZERO,
            fast_window: 2,
            slow_window: 3,
            bar_delay_ms: 0,
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
        let mut session = PaperRunSession::new(self)?;
        for market_slice in market_slices {
            self.wait_before_bar(&cancel).await?;
            session.process_market_slice(market_slice).await?;
        }
        session.finish().await
    }

    pub async fn run_bar_stream_with_cancel(
        &self,
        mut bars: mpsc::Receiver<Bar>,
        cancel: CancellationFlag,
    ) -> anyhow::Result<BacktestSummary> {
        let mut session = PaperRunSession::new(self)?;
        while let Some(bar) = bars.recv().await {
            self.wait_before_bar(&cancel).await?;
            session
                .process_market_slice(MarketSlice::single(self.primary_symbol(), bar))
                .await?;
        }
        session.finish().await
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
    signals: usize,
    orders: usize,
    started_at_ms: Option<i64>,
    ended_at_ms: i64,
    last_snapshot: AccountSnapshot,
}

impl<'a> PaperRunSession<'a> {
    fn new(runtime: &'a PaperRuntime) -> anyhow::Result<Self> {
        let registry = StrategyRegistry;
        let assembly = registry.assemble_alpha(
            StrategyAssemblyConfig {
                strategy_name: runtime.settings.strategy_name.clone(),
                universe_name: runtime.settings.universe_name.clone(),
                alpha_name: runtime.settings.alpha_name.clone(),
                symbols: runtime.settings.assembly_symbols(),
                fast_window: runtime.settings.fast_window,
                slow_window: runtime.settings.slow_window,
            },
            StrategyRuntimeMode::Paper,
        )?;
        let mut engine = AlgorithmEngine::new_with_universe(
            AlgorithmEngineSettings {
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
                initial_cash: runtime.settings.initial_cash,
            },
            Box::new(assembly.universe),
            assembly.alpha,
        );
        if let Some(event_bus) = &runtime.event_bus {
            engine.set_event_bus(event_bus.clone());
        }
        let last_snapshot = engine.snapshot(Decimal::ONE)?;

        Ok(Self {
            runtime,
            engine,
            signals: 0,
            orders: 0,
            started_at_ms: None,
            ended_at_ms: 0,
            last_snapshot,
        })
    }

    async fn process_market_slice(&mut self, market_slice: MarketSlice) -> anyhow::Result<()> {
        self.started_at_ms.get_or_insert(market_slice.ts_ms);
        self.ended_at_ms = market_slice.ts_ms;
        let step = self.engine.on_market_slice(market_slice.clone())?;
        self.last_snapshot = step.snapshot.clone();
        for decision in step.decisions {
            self.signals += 1;
            let settings = &self.runtime.settings;
            let Some(order) = decision.order.clone() else {
                continue;
            };
            let bar = market_slice.bar(&order.symbol).ok_or_else(|| {
                anyhow::anyhow!("missing market bar for generated order {}", order.symbol)
            })?;
            self.persist_engine_events(&decision.events).await?;
            let client_order_id = self
                .runtime
                .executor
                .client_order_id(&settings.run_id, decision.order_number);
            self.persist_submitted_order(&decision, &order, &client_order_id, bar)
                .await?;
            let fill = match self
                .runtime
                .executor
                .execute_order(order.clone(), bar.close, decision.order_number)
                .await
            {
                Ok(fill) => fill,
                Err(error) => {
                    let message = error.to_string();
                    self.persist_failed_order(&decision, &order, &client_order_id, bar, &message)
                        .await?;
                    return Err(error);
                }
            };
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
            self.orders += 1;
        }

        self.persist_snapshot(market_slice.ts_ms).await
    }

    async fn persist_submitted_order(
        &self,
        decision: &AlgorithmDecision,
        order: &OrderRequest,
        client_order_id: &str,
        bar: &Bar,
    ) -> anyhow::Result<()> {
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

    async fn persist_snapshot(&mut self, ts_ms: i64) -> anyhow::Result<()> {
        let settings = &self.runtime.settings;
        self.last_snapshot = self.engine.snapshot_from_prices()?;
        self.runtime
            .db
            .record_paper_portfolio_snapshot(PaperPortfolioSnapshotCommand {
                run_id: settings.run_id.clone(),
                account_id: settings.account_id.clone(),
                ts_ms,
                cash: self.last_snapshot.cash,
                market_value: self.last_snapshot.market_value,
                equity: self.last_snapshot.equity,
                realized_pnl: self.last_snapshot.realized_pnl,
                unrealized_pnl: self.last_snapshot.unrealized_pnl,
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
                config_json: "{}".to_string(),
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
