#![forbid(unsafe_code)]

use accounting::AccountBook;
use alpha::AlphaModel;
use data::Bar;
use events::{EventBus, RuntimeEvent, TraderEvent, envelope};
use execution::order_for_target_delta;
use market_rules::MarketRuleSet;
use oms::OrderStateMachine;
use portfolio::equal_weight_target;
use risk::{PortfolioRiskPolicy, PortfolioRiskState, RiskPolicy, check_max_position};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use strategies::StrategyRuntimeMode;
use trader_core::{OrderRequest, OrderSide};
use universe::{StaticUniverseSelector, UniverseContext, UniverseSelector};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlgorithmEngineSettings {
    pub run_id: String,
    pub mode: StrategyRuntimeMode,
    pub account_id: String,
    pub symbol: String,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineEventKind {
    UniverseSelected,
    AlphaGenerated,
    PortfolioTargetGenerated,
    MarketRuleValidated,
    RiskApproved,
    ExecutionOrderGenerated,
    OmsSubmitted,
    OmsAccepted,
    BrokerOrderSubmitted,
    BrokerOrderFilled,
    BrokerOrderUnfilled,
    AccountingUpdated,
    PortfolioSnapshot,
}

impl EngineEventKind {
    pub fn category(self) -> &'static str {
        match self {
            Self::UniverseSelected => "algorithm.universe.selected",
            Self::AlphaGenerated => "algorithm.alpha.generated",
            Self::PortfolioTargetGenerated => "algorithm.portfolio.target",
            Self::MarketRuleValidated => "algorithm.market_rule.validated",
            Self::RiskApproved => "algorithm.risk.approved",
            Self::ExecutionOrderGenerated => "algorithm.execution.order",
            Self::OmsSubmitted => "algorithm.oms.submitted",
            Self::OmsAccepted => "algorithm.oms.accepted",
            Self::BrokerOrderSubmitted => "broker.order.submitted",
            Self::BrokerOrderFilled => "broker.order.filled",
            Self::BrokerOrderUnfilled => "broker.order.unfilled",
            Self::AccountingUpdated => "accounting.updated",
            Self::PortfolioSnapshot => "portfolio.snapshot",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineEvent {
    pub kind: EngineEventKind,
    pub category: String,
    pub ts_ms: i64,
    pub payload_json: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AlgorithmDecision {
    pub order_number: usize,
    pub order_id: String,
    pub fill_id: String,
    pub order: Option<OrderRequest>,
    pub events: Vec<EngineEvent>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AlgorithmStep {
    pub decision: Option<AlgorithmDecision>,
    pub snapshot: AccountSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountSnapshot {
    pub cash: Decimal,
    pub market_value: Decimal,
    pub equity: Decimal,
    pub realized_pnl: Decimal,
    pub unrealized_pnl: Decimal,
    pub position_qty: Decimal,
    pub position_avg_price: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedExecution {
    pub events: Vec<EngineEvent>,
    pub snapshot: AccountSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionReport {
    pub broker_order_id: String,
    pub status: String,
    pub price: Decimal,
    pub qty: Decimal,
    pub fee: Decimal,
}

pub struct AlgorithmEngine {
    settings: AlgorithmEngineSettings,
    universe: Box<dyn UniverseSelector>,
    alpha: Box<dyn AlphaModel + Send + Sync>,
    account_book: AccountBook,
    portfolio_risk: PortfolioRiskPolicy,
    event_bus: Option<EventBus>,
    peak_equity: Decimal,
    orders: usize,
}

impl AlgorithmEngine {
    pub fn new(
        settings: AlgorithmEngineSettings,
        alpha: Box<dyn AlphaModel + Send + Sync>,
    ) -> Self {
        let universe = Box::new(StaticUniverseSelector::new(vec![settings.symbol.clone()]));
        Self::new_with_universe(settings, universe, alpha)
    }

    pub fn new_with_universe(
        settings: AlgorithmEngineSettings,
        universe: Box<dyn UniverseSelector>,
        alpha: Box<dyn AlphaModel + Send + Sync>,
    ) -> Self {
        let account_book = AccountBook::new(settings.account_id.clone(), settings.initial_cash);
        let portfolio_risk = PortfolioRiskPolicy::new(
            settings.max_exposure,
            settings.max_drawdown,
            settings.max_leverage,
            settings.max_margin_used,
        );
        Self {
            peak_equity: settings.initial_cash,
            settings,
            universe,
            alpha,
            account_book,
            portfolio_risk,
            event_bus: None,
            orders: 0,
        }
    }

    pub fn set_event_bus(&mut self, event_bus: EventBus) {
        self.event_bus = Some(event_bus);
    }

    pub fn on_bar(&mut self, bar: Bar) -> anyhow::Result<AlgorithmStep> {
        let mut events = Vec::new();
        let context = UniverseContext {
            primary_symbol: self.settings.symbol.clone(),
            bar: bar.clone(),
        };
        let selected = self.universe.select(&context)?;
        events.push(self.event(
            EngineEventKind::UniverseSelected,
            bar.ts_ms,
            serde_json::json!({
                "run_id": self.settings.run_id,
                "mode": format!("{:?}", self.settings.mode),
                "symbols": selected
            }),
        ));
        if !selected
            .iter()
            .any(|symbol| symbol == &self.settings.symbol)
        {
            self.publish_events(&events);
            return Ok(AlgorithmStep {
                decision: None,
                snapshot: self.snapshot(bar.close)?,
            });
        }

        let Some(signal) = self.alpha.on_bar(&bar) else {
            self.publish_events(&events);
            return Ok(AlgorithmStep {
                decision: None,
                snapshot: self.snapshot(bar.close)?,
            });
        };
        events.push(self.event(
            EngineEventKind::AlphaGenerated,
            bar.ts_ms,
            serde_json::json!({
                "run_id": self.settings.run_id,
                "symbol": signal.symbol,
                "side": format!("{:?}", signal.side),
                "confidence": signal.confidence
            }),
        ));

        let target = equal_weight_target(&signal, self.settings.order_qty);
        check_max_position(&target, self.settings.max_abs_qty)?;
        events.push(self.event(
            EngineEventKind::PortfolioTargetGenerated,
            bar.ts_ms,
            serde_json::json!({
                "run_id": self.settings.run_id,
                "symbol": target.symbol,
                "target_qty": target.target_qty.to_string()
            }),
        ));

        let current_qty = self
            .account_book
            .position(&self.settings.symbol)
            .map_or(Decimal::ZERO, |position| position.qty);
        let order = order_for_target_delta(&target, current_qty, self.settings.account_id.clone());
        let Some(order) = order else {
            self.publish_events(&events);
            return Ok(AlgorithmStep {
                decision: None,
                snapshot: self.snapshot(bar.close)?,
            });
        };
        MarketRuleSet::for_symbol(&order.symbol)?.validate_order(&order, bar.close)?;
        events.push(self.event(
            EngineEventKind::MarketRuleValidated,
            bar.ts_ms,
            order_payload(
                &self.settings.run_id,
                &order,
                Decimal::ZERO,
                "validated",
                None,
            ),
        ));

        let gross_exposure = self
            .account_book
            .market_value(&self.settings.symbol, bar.close);
        let equity = self.account_book.equity(&self.settings.symbol, bar.close);
        let portfolio_state = PortfolioRiskState::new(
            equity,
            self.peak_equity,
            gross_exposure,
            Decimal::ZERO,
            self.settings.trading_halted,
        );
        self.portfolio_risk
            .check_projected_order(&order, bar.close, &portfolio_state)?;
        RiskPolicy::new(
            self.settings.max_order_qty,
            self.settings.max_order_notional,
            self.settings.min_cash_after_order,
        )
        .check_order(
            &order,
            bar.close,
            self.account_book.cash(),
            self.settings.trading_halted,
        )?;
        events.push(self.event(
            EngineEventKind::RiskApproved,
            bar.ts_ms,
            order_payload(
                &self.settings.run_id,
                &order,
                Decimal::ZERO,
                "approved",
                None,
            ),
        ));

        events.push(self.event(
            EngineEventKind::ExecutionOrderGenerated,
            bar.ts_ms,
            order_payload(
                &self.settings.run_id,
                &order,
                Decimal::ZERO,
                "generated",
                None,
            ),
        ));
        let mut order_state = OrderStateMachine::with_order_qty(order.qty);
        order_state.submit()?;
        events.push(self.event(
            EngineEventKind::OmsSubmitted,
            bar.ts_ms,
            order_payload(
                &self.settings.run_id,
                &order,
                Decimal::ZERO,
                "submitted",
                None,
            ),
        ));
        order_state.accept()?;
        events.push(self.event(
            EngineEventKind::OmsAccepted,
            bar.ts_ms,
            order_payload(
                &self.settings.run_id,
                &order,
                Decimal::ZERO,
                "accepted",
                None,
            ),
        ));

        self.orders += 1;
        let order_number = self.orders;
        let decision = AlgorithmDecision {
            order_number,
            order_id: format!("{}-order-{}", self.settings.run_id, order_number),
            fill_id: format!("{}-fill-{}", self.settings.run_id, order_number),
            order: Some(order),
            events,
        };
        self.publish_events(&decision.events);
        Ok(AlgorithmStep {
            decision: Some(decision),
            snapshot: self.snapshot(bar.close)?,
        })
    }

    pub fn apply_execution(
        &mut self,
        order: &OrderRequest,
        report: &ExecutionReport,
        ts_ms: i64,
    ) -> anyhow::Result<AppliedExecution> {
        if report.qty > Decimal::ZERO {
            match order.side {
                OrderSide::Buy => {
                    self.account_book
                        .buy(&order.symbol, report.qty, report.price, report.fee)
                }
                OrderSide::Sell => {
                    self.account_book
                        .sell(&order.symbol, report.qty, report.price, report.fee)?
                }
            }
        }
        let fill_kind = if report.qty > Decimal::ZERO {
            EngineEventKind::BrokerOrderFilled
        } else {
            EngineEventKind::BrokerOrderUnfilled
        };
        let events = vec![
            self.event(
                fill_kind,
                ts_ms,
                order_payload(
                    &self.settings.run_id,
                    order,
                    report.qty,
                    &report.status,
                    Some(&report.broker_order_id),
                ),
            ),
            self.event(
                EngineEventKind::AccountingUpdated,
                ts_ms,
                serde_json::json!({
                    "run_id": self.settings.run_id,
                    "cash": self.account_book.cash().to_string(),
                    "realized_pnl": self.account_book.realized_pnl().to_string()
                }),
            ),
        ];
        self.publish_events(&events);
        Ok(AppliedExecution {
            events,
            snapshot: self.snapshot(report.price)?,
        })
    }

    pub fn snapshot(&mut self, mark_price: Decimal) -> anyhow::Result<AccountSnapshot> {
        let market_value = self
            .account_book
            .market_value(&self.settings.symbol, mark_price);
        let equity = self.account_book.equity(&self.settings.symbol, mark_price);
        self.portfolio_risk
            .check_portfolio(&PortfolioRiskState::new(
                equity,
                self.peak_equity,
                market_value,
                Decimal::ZERO,
                self.settings.trading_halted,
            ))?;
        if equity > self.peak_equity {
            self.peak_equity = equity;
        }
        let position = self.account_book.position(&self.settings.symbol);
        Ok(AccountSnapshot {
            cash: self.account_book.cash(),
            market_value,
            equity,
            realized_pnl: self.account_book.realized_pnl(),
            unrealized_pnl: self
                .account_book
                .unrealized_pnl(&self.settings.symbol, mark_price),
            position_qty: position.map_or(Decimal::ZERO, |position| position.qty),
            position_avg_price: position.map_or(Decimal::ZERO, |position| position.avg_price),
        })
    }

    fn event(&self, kind: EngineEventKind, ts_ms: i64, payload: serde_json::Value) -> EngineEvent {
        EngineEvent {
            kind,
            category: kind.category().to_string(),
            ts_ms,
            payload_json: payload.to_string(),
        }
    }

    fn publish_events(&self, events: &[EngineEvent]) {
        let Some(event_bus) = &self.event_bus else {
            return;
        };
        for event in events {
            // best-effort: runtime observers may lag or disconnect.
            let _ = event_bus.publish(envelope(
                "algorithm",
                TraderEvent::Runtime(RuntimeEvent {
                    category: event.category.clone(),
                    payload_json: event.payload_json.clone(),
                }),
            ));
        }
    }
}

fn order_payload(
    run_id: &str,
    order: &OrderRequest,
    filled_qty: Decimal,
    status: &str,
    broker_order_id: Option<&str>,
) -> serde_json::Value {
    serde_json::json!({
        "run_id": run_id,
        "broker_order_id": broker_order_id,
        "account_id": order.account_id,
        "symbol": order.symbol,
        "side": format!("{:?}", order.side).to_uppercase(),
        "order_type": format!("{:?}", order.order_type).to_uppercase(),
        "qty": order.qty.to_string(),
        "filled_qty": filled_qty.to_string(),
        "status": status,
    })
}
