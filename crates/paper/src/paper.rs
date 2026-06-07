#![forbid(unsafe_code)]

pub mod binance;
pub mod ibkr;

pub use binance::{BinancePaperOrderClient, BinancePaperOrderExecutor, binance_spot_symbol};
pub use ibkr::{
    IbkrPaperGatewayOrderClient, IbkrPaperOrderClient, IbkrPaperOrderExecutor, ibkr_stock_symbol,
};

use algorithm::{
    AccountSnapshot, AlgorithmEngine, AlgorithmEngineSettings, EngineEvent, ExecutionReport,
};
use async_trait::async_trait;
use backtest::BacktestSummary;
use broker::{SimulatedBrokerSettings, simulate_market_fill};
use data::Bar;
use runtime::CancellationFlag;
use rust_decimal::Decimal;
use std::{error::Error, fmt, time::Duration};
use storage::{
    Db, NewAccountBalance, NewEventRecord, NewFill, NewOrder, NewPortfolioSnapshot, NewPosition,
    NewStrategyRun,
};
use strategies::{StrategyContext, StrategyRegistry, StrategyRuntimeMode};
use tokio::sync::mpsc;
use trader_core::OrderRequest;

pub struct PaperRuntime {
    db: Db,
    settings: PaperSettings,
    executor: Box<dyn PaperOrderExecutor>,
}

#[derive(Debug, Clone)]
pub struct PaperSettings {
    pub run_id: String,
    pub strategy_name: String,
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
        }
    }

    pub async fn run_bars(&self, bars: Vec<Bar>) -> anyhow::Result<BacktestSummary> {
        self.run_bars_with_cancel(bars, CancellationFlag::default())
            .await
    }

    pub async fn run_bars_with_cancel(
        &self,
        bars: Vec<Bar>,
        cancel: CancellationFlag,
    ) -> anyhow::Result<BacktestSummary> {
        let mut session = PaperRunSession::new(self)?;
        for bar in bars {
            self.wait_before_bar(&cancel).await?;
            session.process_bar(bar).await?;
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
            session.process_bar(bar).await?;
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
        let strategy = registry.create_alpha(
            &runtime.settings.strategy_name,
            StrategyContext::new(
                runtime.settings.strategy_name.clone(),
                runtime.settings.symbol.clone(),
                StrategyRuntimeMode::Paper,
            ),
            runtime.settings.fast_window,
            runtime.settings.slow_window,
        )?;
        let mut engine = AlgorithmEngine::new(
            AlgorithmEngineSettings {
                run_id: runtime.settings.run_id.clone(),
                mode: StrategyRuntimeMode::Paper,
                account_id: runtime.settings.account_id.clone(),
                symbol: runtime.settings.symbol.clone(),
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
            strategy,
        );
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

    async fn process_bar(&mut self, bar: Bar) -> anyhow::Result<()> {
        self.started_at_ms.get_or_insert(bar.ts_ms);
        self.ended_at_ms = bar.ts_ms;
        let step = self.engine.on_bar(bar.clone())?;
        self.last_snapshot = step.snapshot.clone();
        if let Some(decision) = step.decision {
            self.signals += 1;
            let settings = &self.runtime.settings;
            let Some(order) = decision.order else {
                self.persist_snapshot(&bar).await?;
                return Ok(());
            };
            self.persist_engine_events(&decision.events).await?;
            let client_order_id = self
                .runtime
                .executor
                .client_order_id(&settings.run_id, decision.order_number);
            self.runtime
                .db
                .insert_order(NewOrder {
                    id: decision.order_id.clone(),
                    run_id: settings.run_id.clone(),
                    client_order_id: client_order_id.clone(),
                    broker_order_id: None,
                    account_id: order.account_id.clone(),
                    symbol: order.symbol.clone(),
                    side: format!("{:?}", order.side).to_uppercase(),
                    order_type: format!("{:?}", order.order_type).to_uppercase(),
                    price: order.price.map(|price| price.to_string()),
                    qty: order.qty.to_string(),
                    filled_qty: Decimal::ZERO.to_string(),
                    status: "SUBMITTED".to_string(),
                    created_at_ms: bar.ts_ms,
                    updated_at_ms: bar.ts_ms,
                })
                .await?;
            self.runtime
                .db
                .insert_event(NewEventRecord {
                    event_id: uuid::Uuid::new_v4().to_string(),
                    ts_ms: bar.ts_ms,
                    source: settings.run_id.clone(),
                    category: "broker.order.submitted".to_string(),
                    payload_json: serde_json::json!({
                        "run_id": &settings.run_id,
                        "order_id": &decision.order_id,
                        "client_order_id": &client_order_id,
                        "broker_order_id": null,
                        "account_id": &order.account_id,
                        "symbol": &order.symbol,
                        "side": format!("{:?}", order.side).to_uppercase(),
                        "order_type": format!("{:?}", order.order_type).to_uppercase(),
                        "qty": order.qty.to_string(),
                        "filled_qty": Decimal::ZERO.to_string(),
                        "status": "SUBMITTED"
                    })
                    .to_string(),
                })
                .await?;
            let fill = self
                .runtime
                .executor
                .execute_order(order.clone(), bar.close, decision.order_number)
                .await?;
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
            self.runtime
                .db
                .insert_order(NewOrder {
                    id: decision.order_id.clone(),
                    run_id: settings.run_id.clone(),
                    client_order_id,
                    broker_order_id: Some(fill.broker_order_id.clone()),
                    account_id: order.account_id.clone(),
                    symbol: order.symbol.clone(),
                    side: format!("{:?}", order.side).to_uppercase(),
                    order_type: format!("{:?}", order.order_type).to_uppercase(),
                    price: order.price.map(|price| price.to_string()),
                    qty: order.qty.to_string(),
                    filled_qty: fill.qty.to_string(),
                    status: fill.status.clone(),
                    created_at_ms: bar.ts_ms,
                    updated_at_ms: bar.ts_ms,
                })
                .await?;
            if fill.qty > Decimal::ZERO {
                self.runtime
                    .db
                    .insert_fill(NewFill {
                        id: decision.fill_id,
                        order_id: decision.order_id.clone(),
                        run_id: settings.run_id.clone(),
                        symbol: order.symbol.clone(),
                        side: format!("{:?}", order.side).to_uppercase(),
                        price: fill.price.to_string(),
                        qty: fill.qty.to_string(),
                        fee: fill.fee.to_string(),
                        ts_ms: bar.ts_ms,
                    })
                    .await?;
            }
            self.orders += 1;
        }

        self.persist_snapshot(&bar).await
    }

    async fn persist_snapshot(&mut self, bar: &Bar) -> anyhow::Result<()> {
        let settings = &self.runtime.settings;
        self.last_snapshot = self.engine.snapshot(bar.close)?;
        self.runtime
            .db
            .insert_portfolio_snapshot(NewPortfolioSnapshot {
                id: format!("{}-snapshot-{}", settings.run_id, bar.ts_ms),
                run_id: settings.run_id.clone(),
                account_id: settings.account_id.clone(),
                ts_ms: bar.ts_ms,
                cash: self.last_snapshot.cash.to_string(),
                market_value: self.last_snapshot.market_value.to_string(),
                equity: self.last_snapshot.equity.to_string(),
                realized_pnl: self.last_snapshot.realized_pnl.to_string(),
                unrealized_pnl: self.last_snapshot.unrealized_pnl.to_string(),
            })
            .await?;
        Ok(())
    }

    async fn persist_engine_events(&self, events: &[EngineEvent]) -> anyhow::Result<()> {
        for event in events {
            self.runtime
                .db
                .insert_event(NewEventRecord {
                    event_id: uuid::Uuid::new_v4().to_string(),
                    ts_ms: event.ts_ms,
                    source: self.runtime.settings.run_id.clone(),
                    category: event.category.clone(),
                    payload_json: event.payload_json.clone(),
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
            .insert_strategy_run(NewStrategyRun {
                id: settings.run_id.clone(),
                name: settings.strategy_name.clone(),
                mode: "paper".to_string(),
                status: "completed".to_string(),
                started_at_ms,
                ended_at_ms: Some(self.ended_at_ms),
                error: None,
                config_json: "{}".to_string(),
            })
            .await?;

        self.runtime
            .db
            .upsert_account_balance(NewAccountBalance {
                run_id: settings.run_id.clone(),
                account_id: settings.account_id.clone(),
                asset: settings.base_currency.clone(),
                total: self.last_snapshot.cash.to_string(),
                available: self.last_snapshot.cash.to_string(),
                frozen: Decimal::ZERO.to_string(),
                updated_at_ms: self.ended_at_ms,
            })
            .await?;

        if self.last_snapshot.position_qty != Decimal::ZERO {
            self.runtime
                .db
                .upsert_position(NewPosition {
                    run_id: settings.run_id.clone(),
                    account_id: settings.account_id.clone(),
                    symbol: settings.symbol.clone(),
                    qty: self.last_snapshot.position_qty.to_string(),
                    avg_price: self.last_snapshot.position_avg_price.to_string(),
                    updated_at_ms: self.ended_at_ms,
                })
                .await?;
        }

        Ok(BacktestSummary {
            signals: self.signals,
            orders: self.orders,
        })
    }
}
