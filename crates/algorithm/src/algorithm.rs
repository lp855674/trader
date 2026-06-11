#![forbid(unsafe_code)]

use accounting::AccountBook;
use alpha::AlphaModel;
use data::{Bar, MarketSlice};
use events::{EventBus, runtime_envelope};
use execution::order_for_target_delta;
use market_rules::MarketRuleSet;
use oms::OrderStateMachine;
use portfolio::equal_weight_target;
use risk::{PortfolioRiskPolicy, PortfolioRiskState, RiskPolicy, check_max_position};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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
    BrokerOrderPartiallyFilled,
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
            Self::BrokerOrderPartiallyFilled => "broker.order.partially_filled",
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
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UniverseSelectedPayload {
    pub run_id: String,
    pub mode: String,
    pub symbols: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlphaGeneratedPayload {
    pub run_id: String,
    pub symbol: String,
    pub side: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortfolioTargetGeneratedPayload {
    pub run_id: String,
    pub symbol: String,
    pub target_qty: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlgorithmOrderPayload {
    pub run_id: String,
    pub broker_order_id: Option<String>,
    pub account_id: String,
    pub symbol: String,
    pub side: String,
    pub order_type: String,
    pub qty: String,
    pub filled_qty: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountingUpdatedPayload {
    pub run_id: String,
    pub cash: String,
    pub realized_pnl: String,
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
    pub decisions: Vec<AlgorithmDecision>,
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
    pub positions: Vec<PositionSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PositionSnapshot {
    pub symbol: String,
    pub qty: Decimal,
    pub avg_price: Decimal,
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
    last_prices: BTreeMap<String, Decimal>,
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
            last_prices: BTreeMap::new(),
            orders: 0,
        }
    }

    pub fn set_event_bus(&mut self, event_bus: EventBus) {
        self.event_bus = Some(event_bus);
    }

    pub fn on_bar(&mut self, bar: Bar) -> anyhow::Result<AlgorithmStep> {
        let symbol = self.settings.symbol.clone();
        self.on_market_slice(MarketSlice::single(symbol, bar))
    }

    pub fn on_market_slice(&mut self, market_slice: MarketSlice) -> anyhow::Result<AlgorithmStep> {
        for (symbol, bar) in market_slice.iter() {
            self.last_prices.insert(symbol.to_string(), bar.close);
        }
        let Some(primary_bar) = market_slice
            .bar(&self.settings.symbol)
            .or_else(|| market_slice.iter().next().map(|(_, bar)| bar))
        else {
            return Ok(AlgorithmStep {
                decision: None,
                decisions: Vec::new(),
                snapshot: self.snapshot_from_prices()?,
            });
        };

        let context = UniverseContext {
            primary_symbol: self.settings.symbol.clone(),
            bar: primary_bar.clone(),
        };
        let selected = self.universe.select(&context)?;
        let universe_event = self.event(
            EngineEventKind::UniverseSelected,
            market_slice.ts_ms,
            payload_value(UniverseSelectedPayload {
                run_id: self.settings.run_id.clone(),
                mode: format!("{:?}", self.settings.mode),
                symbols: selected.clone(),
            }),
        );
        let mut decisions = Vec::new();
        for symbol in selected {
            let Some(bar) = market_slice.bar(&symbol) else {
                continue;
            };
            let include_universe_event = decisions.is_empty();
            if let Some(decision) =
                self.decision_for_symbol(&symbol, bar, include_universe_event, &universe_event)?
            {
                decisions.push(decision);
            }
        }

        if decisions.is_empty() {
            self.publish_events(std::slice::from_ref(&universe_event));
            return Ok(AlgorithmStep {
                decision: None,
                decisions,
                snapshot: self.snapshot_from_prices()?,
            });
        };

        for decision in &decisions {
            self.publish_events(&decision.events);
        }
        Ok(AlgorithmStep {
            decision: decisions.first().cloned(),
            decisions,
            snapshot: self.snapshot_from_prices()?,
        })
    }

    fn decision_for_symbol(
        &mut self,
        symbol: &str,
        bar: &Bar,
        include_universe_event: bool,
        universe_event: &EngineEvent,
    ) -> anyhow::Result<Option<AlgorithmDecision>> {
        let mut events = if include_universe_event {
            vec![universe_event.clone()]
        } else {
            Vec::new()
        };

        let Some(signal) = self.alpha.on_bar_for_symbol(symbol, bar) else {
            return Ok(None);
        };
        events.push(self.event(
            EngineEventKind::AlphaGenerated,
            bar.ts_ms,
            payload_value(AlphaGeneratedPayload {
                run_id: self.settings.run_id.clone(),
                symbol: signal.symbol.clone(),
                side: format!("{:?}", signal.side),
                confidence: signal.confidence,
            }),
        ));

        let target = equal_weight_target(&signal, self.settings.order_qty);
        check_max_position(&target, self.settings.max_abs_qty)?;
        events.push(self.event(
            EngineEventKind::PortfolioTargetGenerated,
            bar.ts_ms,
            payload_value(PortfolioTargetGeneratedPayload {
                run_id: self.settings.run_id.clone(),
                symbol: target.symbol.clone(),
                target_qty: target.target_qty.to_string(),
            }),
        ));

        let current_qty = self
            .account_book
            .position(&target.symbol)
            .map_or(Decimal::ZERO, |position| position.qty);
        let order = order_for_target_delta(&target, current_qty, self.settings.account_id.clone());
        let Some(order) = order else {
            return Ok(None);
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
            .market_value_with_prices(&self.last_prices);
        let equity = self.account_book.equity_with_prices(&self.last_prices);
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
        Ok(Some(decision))
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
        self.last_prices.insert(order.symbol.clone(), report.price);
        let fill_kind = if report.qty > Decimal::ZERO && report.qty < order.qty {
            EngineEventKind::BrokerOrderPartiallyFilled
        } else if report.qty > Decimal::ZERO {
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
                payload_value(AccountingUpdatedPayload {
                    run_id: self.settings.run_id.clone(),
                    cash: self.account_book.cash().to_string(),
                    realized_pnl: self.account_book.realized_pnl().to_string(),
                }),
            ),
        ];
        self.publish_events(&events);
        Ok(AppliedExecution {
            events,
            snapshot: self.snapshot_from_prices()?,
        })
    }

    pub fn snapshot(&mut self, mark_price: Decimal) -> anyhow::Result<AccountSnapshot> {
        self.last_prices
            .insert(self.settings.symbol.clone(), mark_price);
        self.snapshot_from_prices()
    }

    pub fn snapshot_from_prices(&mut self) -> anyhow::Result<AccountSnapshot> {
        let market_value = self
            .account_book
            .market_value_with_prices(&self.last_prices);
        let equity = self.account_book.equity_with_prices(&self.last_prices);
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
        let positions = self
            .account_book
            .positions()
            .into_iter()
            .filter(|position| position.qty != Decimal::ZERO)
            .map(|position| PositionSnapshot {
                symbol: position.symbol.clone(),
                qty: position.qty,
                avg_price: position.avg_price,
            })
            .collect::<Vec<_>>();
        Ok(AccountSnapshot {
            cash: self.account_book.cash(),
            market_value,
            equity,
            realized_pnl: self.account_book.realized_pnl(),
            unrealized_pnl: self
                .account_book
                .unrealized_pnl_with_prices(&self.last_prices),
            position_qty: position.map_or(Decimal::ZERO, |position| position.qty),
            position_avg_price: position.map_or(Decimal::ZERO, |position| position.avg_price),
            positions,
        })
    }

    fn event(&self, kind: EngineEventKind, ts_ms: i64, payload: serde_json::Value) -> EngineEvent {
        EngineEvent {
            kind,
            category: kind.category().to_string(),
            ts_ms,
            payload,
        }
    }

    fn publish_events(&self, events: &[EngineEvent]) {
        let Some(event_bus) = &self.event_bus else {
            return;
        };
        for event in events {
            let Ok(envelope) = runtime_envelope(
                self.settings.run_id.clone(),
                event.category.clone(),
                &event.payload,
            ) else {
                continue;
            };
            // best-effort: runtime observers may lag or disconnect.
            let _ = event_bus.publish(envelope);
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
    payload_value(AlgorithmOrderPayload {
        run_id: run_id.to_string(),
        broker_order_id: broker_order_id.map(str::to_string),
        account_id: order.account_id.clone(),
        symbol: order.symbol.clone(),
        side: format!("{:?}", order.side).to_uppercase(),
        order_type: format!("{:?}", order.order_type).to_uppercase(),
        qty: order.qty.to_string(),
        filled_qty: filled_qty.to_string(),
        status: status.to_string(),
    })
}

fn payload_value(payload: impl Serialize) -> serde_json::Value {
    match serde_json::to_value(payload) {
        Ok(value) => value,
        Err(_) => serde_json::Value::Object(Default::default()),
    }
}
