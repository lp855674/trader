#![forbid(unsafe_code)]

pub mod binance;
pub mod ibkr;

pub use binance::{BinancePaperOrderClient, BinancePaperOrderExecutor, binance_spot_symbol};
pub use ibkr::{
    IbkrPaperGatewayOrderClient, IbkrPaperOrderClient, IbkrPaperOrderExecutor, ibkr_stock_symbol,
};

use accounting::AccountBook;
use async_trait::async_trait;
use backtest::BacktestSummary;
use broker::{SimulatedBrokerSettings, simulate_market_fill};
use data::Bar;
use execution::order_for_target_delta;
use market_rules::MarketRuleSet;
use oms::OrderStateMachine;
use portfolio::equal_weight_target;
use risk::{PortfolioRiskPolicy, PortfolioRiskState, RiskPolicy, check_max_position};
use runtime::CancellationFlag;
use rust_decimal::Decimal;
use std::{error::Error, fmt, time::Duration};
use storage::{
    Db, NewAccountBalance, NewEventRecord, NewFill, NewOrder, NewPortfolioSnapshot, NewPosition,
    NewStrategyRun,
};
use strategies::{Strategy, StrategyContext, StrategyRegistry, StrategyRuntimeMode};
use tokio::sync::mpsc;
use trader_core::{OrderRequest, OrderSide};

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

fn order_event(
    run_id: &str,
    category: &str,
    order_id: &str,
    client_order_id: &str,
    broker_order_id: Option<&str>,
    order: &OrderRequest,
    filled_qty: Decimal,
    status: String,
    ts_ms: i64,
) -> NewEventRecord {
    let payload_json = serde_json::json!({
        "run_id": run_id,
        "order_id": order_id,
        "client_order_id": client_order_id,
        "broker_order_id": broker_order_id,
        "account_id": &order.account_id,
        "symbol": &order.symbol,
        "side": format!("{:?}", order.side).to_uppercase(),
        "order_type": format!("{:?}", order.order_type).to_uppercase(),
        "qty": order.qty.to_string(),
        "filled_qty": filled_qty.to_string(),
        "status": status
    })
    .to_string();

    NewEventRecord {
        event_id: uuid::Uuid::new_v4().to_string(),
        ts_ms,
        source: run_id.to_string(),
        category: category.to_string(),
        payload_json,
    }
}

struct PaperRunSession<'a> {
    runtime: &'a PaperRuntime,
    strategy: Box<dyn Strategy + Send>,
    account_book: AccountBook,
    portfolio_risk: PortfolioRiskPolicy,
    peak_equity: Decimal,
    signals: usize,
    orders: usize,
    started_at_ms: Option<i64>,
    ended_at_ms: i64,
}

impl<'a> PaperRunSession<'a> {
    fn new(runtime: &'a PaperRuntime) -> anyhow::Result<Self> {
        let registry = StrategyRegistry;
        let strategy = registry.create(
            &runtime.settings.strategy_name,
            StrategyContext::new(
                runtime.settings.strategy_name.clone(),
                runtime.settings.symbol.clone(),
                StrategyRuntimeMode::Paper,
            ),
            runtime.settings.fast_window,
            runtime.settings.slow_window,
        )?;
        let account_book = AccountBook::new(
            runtime.settings.account_id.clone(),
            runtime.settings.initial_cash,
        );
        let portfolio_risk = PortfolioRiskPolicy::new(
            runtime.settings.max_exposure,
            runtime.settings.max_drawdown,
            runtime.settings.max_leverage,
            runtime.settings.max_margin_used,
        );

        Ok(Self {
            runtime,
            strategy,
            account_book,
            portfolio_risk,
            peak_equity: runtime.settings.initial_cash,
            signals: 0,
            orders: 0,
            started_at_ms: None,
            ended_at_ms: 0,
        })
    }

    async fn process_bar(&mut self, bar: Bar) -> anyhow::Result<()> {
        self.started_at_ms.get_or_insert(bar.ts_ms);
        self.ended_at_ms = bar.ts_ms;
        if let Some(signal) = self.strategy.on_bar(&bar) {
            self.signals += 1;
            let settings = &self.runtime.settings;
            let target = equal_weight_target(&signal, settings.order_qty);
            check_max_position(&target, settings.max_abs_qty)?;
            let current_qty = self
                .account_book
                .position(&settings.symbol)
                .map_or(Decimal::ZERO, |position| position.qty);
            let Some(order) =
                order_for_target_delta(&target, current_qty, settings.account_id.clone())
            else {
                self.persist_snapshot(&bar).await?;
                return Ok(());
            };
            MarketRuleSet::for_symbol(&order.symbol)?.validate_order(&order, bar.close)?;
            let gross_exposure = self.account_book.market_value(&settings.symbol, bar.close);
            let equity = self.account_book.equity(&settings.symbol, bar.close);
            let portfolio_state = PortfolioRiskState::new(
                equity,
                self.peak_equity,
                gross_exposure,
                Decimal::ZERO,
                settings.trading_halted,
            );
            self.portfolio_risk
                .check_projected_order(&order, bar.close, &portfolio_state)?;
            RiskPolicy::new(
                settings.max_order_qty,
                settings.max_order_notional,
                settings.min_cash_after_order,
            )
            .check_order(
                &order,
                bar.close,
                self.account_book.cash(),
                settings.trading_halted,
            )?;
            let mut order_state = OrderStateMachine::with_order_qty(order.qty);
            order_state.submit()?;
            order_state.accept()?;
            let order_number = self.orders + 1;
            let order_id = format!("{}-order-{}", settings.run_id, order_number);
            let fill_id = format!("{}-fill-{}", settings.run_id, order_number);
            let client_order_id = self
                .runtime
                .executor
                .client_order_id(&settings.run_id, order_number);
            self.runtime
                .db
                .insert_order(NewOrder {
                    id: order_id.clone(),
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
                    status: format!("{:?}", order_state.status()).to_uppercase(),
                    created_at_ms: bar.ts_ms,
                    updated_at_ms: bar.ts_ms,
                })
                .await?;
            self.runtime
                .db
                .insert_event(order_event(
                    &settings.run_id,
                    "paper.order.submitted",
                    &order_id,
                    &client_order_id,
                    None,
                    &order,
                    Decimal::ZERO,
                    format!("{:?}", order_state.status()).to_uppercase(),
                    bar.ts_ms,
                ))
                .await?;
            let fill = self
                .runtime
                .executor
                .execute_order(order.clone(), bar.close, order_number)
                .await?;
            self.runtime
                .db
                .insert_order(NewOrder {
                    id: order_id.clone(),
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
                order_state.record_fill(fill.qty)?;
                self.runtime
                    .db
                    .insert_fill(NewFill {
                        id: fill_id,
                        order_id: order_id.clone(),
                        run_id: settings.run_id.clone(),
                        symbol: order.symbol.clone(),
                        side: format!("{:?}", order.side).to_uppercase(),
                        price: fill.price.to_string(),
                        qty: fill.qty.to_string(),
                        fee: fill.fee.to_string(),
                        ts_ms: bar.ts_ms,
                    })
                    .await?;

                match order.side {
                    OrderSide::Buy => {
                        self.account_book
                            .buy(&order.symbol, fill.qty, fill.price, fill.fee)
                    }
                    OrderSide::Sell => {
                        self.account_book
                            .sell(&order.symbol, fill.qty, fill.price, fill.fee)?
                    }
                }
                self.runtime
                    .db
                    .insert_event(order_event(
                        &settings.run_id,
                        "paper.order.filled",
                        &order_id,
                        &fill.client_order_id,
                        Some(&fill.broker_order_id),
                        &order,
                        fill.qty,
                        fill.status.clone(),
                        bar.ts_ms,
                    ))
                    .await?;
            } else {
                self.runtime
                    .db
                    .insert_event(order_event(
                        &settings.run_id,
                        "paper.order.unfilled",
                        &order_id,
                        &fill.client_order_id,
                        Some(&fill.broker_order_id),
                        &order,
                        fill.qty,
                        fill.status.clone(),
                        bar.ts_ms,
                    ))
                    .await?;
            }
            self.orders += 1;
        }

        self.persist_snapshot(&bar).await
    }

    async fn persist_snapshot(&mut self, bar: &Bar) -> anyhow::Result<()> {
        let settings = &self.runtime.settings;
        let market_value = self.account_book.market_value(&settings.symbol, bar.close);
        let equity = self.account_book.equity(&settings.symbol, bar.close);
        self.portfolio_risk
            .check_portfolio(&PortfolioRiskState::new(
                equity,
                self.peak_equity,
                market_value,
                Decimal::ZERO,
                settings.trading_halted,
            ))?;
        if equity > self.peak_equity {
            self.peak_equity = equity;
        }
        let unrealized_pnl = self
            .account_book
            .unrealized_pnl(&settings.symbol, bar.close);
        self.runtime
            .db
            .insert_portfolio_snapshot(NewPortfolioSnapshot {
                id: format!("{}-snapshot-{}", settings.run_id, bar.ts_ms),
                run_id: settings.run_id.clone(),
                account_id: settings.account_id.clone(),
                ts_ms: bar.ts_ms,
                cash: self.account_book.cash().to_string(),
                market_value: market_value.to_string(),
                equity: equity.to_string(),
                realized_pnl: self.account_book.realized_pnl().to_string(),
                unrealized_pnl: unrealized_pnl.to_string(),
            })
            .await?;
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
                total: self.account_book.cash().to_string(),
                available: self.account_book.cash().to_string(),
                frozen: Decimal::ZERO.to_string(),
                updated_at_ms: self.ended_at_ms,
            })
            .await?;

        if let Some(position) = self.account_book.position(&settings.symbol) {
            self.runtime
                .db
                .upsert_position(NewPosition {
                    run_id: settings.run_id.clone(),
                    account_id: settings.account_id.clone(),
                    symbol: position.symbol.clone(),
                    qty: position.qty.to_string(),
                    avg_price: position.avg_price.to_string(),
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
