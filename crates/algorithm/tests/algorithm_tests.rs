use algorithm::{
    AccountingUpdatedPayload, AlgorithmEngine, AlgorithmEngineSettings, AlgorithmOrderPayload,
    AlphaGeneratedPayload, EngineEventKind, UniverseSelectedPayload,
};
use alpha::{AlphaModel, CompositeAlphaModel};
use data::{Bar, MarketSlice, SymbolBar};
use events::{EventBus, SignalEvent, SignalSide};
use rust_decimal_macros::dec;
use strategies::{MovingAverageCrossStrategy, StrategyRuntimeMode};
use universe::StaticUniverseSelector;

#[test]
fn algorithm_engine_emits_full_decision_chain_for_selected_symbol() {
    let strategy = MovingAverageCrossStrategy::new("ma", "US:NASDAQ:AAPL:EQUITY", 2, 3).unwrap();
    let mut engine = AlgorithmEngine::new(
        AlgorithmEngineSettings {
            run_id: "run-a".to_string(),
            mode: StrategyRuntimeMode::Paper,
            account_id: "paper".to_string(),
            symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
            order_qty: dec!(1),
            max_abs_qty: dec!(100),
            max_order_qty: dec!(100),
            max_order_notional: dec!(1000000),
            min_cash_after_order: dec!(0),
            max_exposure: dec!(1000000),
            max_drawdown: dec!(1),
            max_leverage: dec!(10),
            max_margin_used: dec!(0),
            trading_halted: false,
            initial_cash: dec!(100000),
        },
        Box::new(strategy),
    );

    let bars = [bar(1, dec!(100)), bar(2, dec!(101)), bar(3, dec!(102))];
    let mut decision = None;
    for current_bar in bars {
        decision = engine.on_bar(current_bar).unwrap().decision;
    }

    let decision = decision.expect("third bar should produce a trading decision");
    let kinds = decision
        .events
        .iter()
        .map(|event| event.kind)
        .collect::<Vec<_>>();

    assert_eq!(
        kinds,
        vec![
            EngineEventKind::UniverseSelected,
            EngineEventKind::AlphaGenerated,
            EngineEventKind::PortfolioTargetGenerated,
            EngineEventKind::MarketRuleValidated,
            EngineEventKind::RiskApproved,
            EngineEventKind::ExecutionOrderGenerated,
            EngineEventKind::OmsSubmitted,
            EngineEventKind::OmsAccepted,
        ]
    );
    assert_eq!(decision.order.unwrap().symbol, "US:NASDAQ:AAPL:EQUITY");
}

#[test]
fn algorithm_engine_uses_highest_confidence_composite_alpha_signal() {
    let strategy = CompositeAlphaModel::new(vec![
        Box::new(FixedAlphaModel::new("low", SignalSide::Sell, 0.2)),
        Box::new(FixedAlphaModel::new("high", SignalSide::Buy, 0.9)),
    ]);
    let mut engine = AlgorithmEngine::new(
        AlgorithmEngineSettings {
            run_id: "run-composite".to_string(),
            mode: StrategyRuntimeMode::Backtest,
            account_id: "backtest".to_string(),
            symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
            order_qty: dec!(1),
            max_abs_qty: dec!(100),
            max_order_qty: dec!(100),
            max_order_notional: dec!(1000000),
            min_cash_after_order: dec!(0),
            max_exposure: dec!(1000000),
            max_drawdown: dec!(1),
            max_leverage: dec!(10),
            max_margin_used: dec!(0),
            trading_halted: false,
            initial_cash: dec!(100000),
        },
        Box::new(strategy),
    );

    let decision = engine
        .on_bar(bar(1, dec!(100)))
        .unwrap()
        .decision
        .expect("composite alpha should emit a decision");

    assert_eq!(decision.order.unwrap().side, trader_core::OrderSide::Buy);
}

#[test]
fn algorithm_engine_generates_orders_for_each_selected_symbol_in_market_slice() {
    let mut engine = AlgorithmEngine::new_with_universe(
        AlgorithmEngineSettings {
            run_id: "run-multi".to_string(),
            mode: StrategyRuntimeMode::Paper,
            account_id: "paper".to_string(),
            symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
            order_qty: dec!(1),
            max_abs_qty: dec!(100),
            max_order_qty: dec!(100),
            max_order_notional: dec!(1000000),
            min_cash_after_order: dec!(0),
            max_exposure: dec!(1000000),
            max_drawdown: dec!(1),
            max_leverage: dec!(10),
            max_margin_used: dec!(0),
            trading_halted: false,
            initial_cash: dec!(100000),
        },
        Box::new(StaticUniverseSelector::new(vec![
            "US:NASDAQ:AAPL:EQUITY".to_string(),
            "US:NASDAQ:MSFT:EQUITY".to_string(),
        ])),
        Box::new(SymbolEchoAlphaModel),
    );

    let step = engine
        .on_market_slice(market_slice(1, dec!(20), dec!(50)))
        .unwrap();

    let symbols = step
        .decisions
        .iter()
        .map(|decision| decision.order.as_ref().unwrap().symbol.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        symbols,
        vec![
            "US:NASDAQ:AAPL:EQUITY".to_string(),
            "US:NASDAQ:MSFT:EQUITY".to_string()
        ]
    );
    assert_eq!(step.snapshot.positions.len(), 0);
    assert_eq!(step.decisions[0].order_number, 1);
    assert_eq!(step.decisions[1].order_number, 2);
}

#[test]
fn algorithm_engine_publishes_runtime_events_with_run_id_source() {
    let event_bus = EventBus::new(16);
    let mut receiver = event_bus.subscribe();
    let strategy = CompositeAlphaModel::new(vec![Box::new(FixedAlphaModel::new(
        "source",
        SignalSide::Buy,
        0.9,
    ))]);
    let mut engine = AlgorithmEngine::new(
        AlgorithmEngineSettings {
            run_id: "run-source".to_string(),
            mode: StrategyRuntimeMode::Paper,
            account_id: "paper".to_string(),
            symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
            order_qty: dec!(1),
            max_abs_qty: dec!(100),
            max_order_qty: dec!(100),
            max_order_notional: dec!(1000000),
            min_cash_after_order: dec!(0),
            max_exposure: dec!(1000000),
            max_drawdown: dec!(1),
            max_leverage: dec!(10),
            max_margin_used: dec!(0),
            trading_halted: false,
            initial_cash: dec!(100000),
        },
        Box::new(strategy),
    );
    engine.set_event_bus(event_bus);

    engine.on_bar(bar(1, dec!(100))).unwrap();

    let event = receiver.try_recv().unwrap();
    assert_eq!(event.source, "run-source");
}

#[test]
fn algorithm_payload_structs_serialize_stable_schema_fields() {
    let order = serde_json::to_value(AlgorithmOrderPayload {
        run_id: "run-schema".to_string(),
        broker_order_id: Some("broker-1".to_string()),
        account_id: "paper".to_string(),
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        side: "BUY".to_string(),
        order_type: "MARKET".to_string(),
        qty: "1".to_string(),
        filled_qty: "1".to_string(),
        status: "FILLED".to_string(),
    })
    .unwrap();
    assert_eq!(order["run_id"], serde_json::json!("run-schema"));
    assert_eq!(order["symbol"], serde_json::json!("US:NASDAQ:AAPL:EQUITY"));
    assert_eq!(order["status"], serde_json::json!("FILLED"));
    assert_eq!(order["filled_qty"], serde_json::json!("1"));
    assert_eq!(order["broker_order_id"], serde_json::json!("broker-1"));

    let accounting = serde_json::to_value(AccountingUpdatedPayload {
        run_id: "run-schema".to_string(),
        cash: "99999".to_string(),
        realized_pnl: "12.5".to_string(),
    })
    .unwrap();
    assert_eq!(accounting["cash"], serde_json::json!("99999"));
    assert_eq!(accounting["realized_pnl"], serde_json::json!("12.5"));

    let universe = serde_json::to_value(UniverseSelectedPayload {
        run_id: "run-schema".to_string(),
        mode: "Paper".to_string(),
        symbols: vec!["US:NASDAQ:AAPL:EQUITY".to_string()],
    })
    .unwrap();
    assert_eq!(universe["run_id"], serde_json::json!("run-schema"));

    let alpha = serde_json::to_value(AlphaGeneratedPayload {
        run_id: "run-schema".to_string(),
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        side: "BUY".to_string(),
        confidence: 0.9,
    })
    .unwrap();
    assert_eq!(alpha["symbol"], serde_json::json!("US:NASDAQ:AAPL:EQUITY"));
}

struct FixedAlphaModel {
    signal: SignalEvent,
}

struct SymbolEchoAlphaModel;

impl AlphaModel for SymbolEchoAlphaModel {
    fn on_bar(&mut self, _bar: &Bar) -> Option<SignalEvent> {
        None
    }

    fn on_bar_for_symbol(&mut self, symbol: &str, _bar: &Bar) -> Option<SignalEvent> {
        Some(SignalEvent {
            strategy_id: "echo".to_string(),
            symbol: symbol.to_string(),
            side: SignalSide::Buy,
            confidence: 0.9,
            ts: chrono::Utc::now(),
        })
    }
}

impl FixedAlphaModel {
    fn new(strategy_id: &str, side: SignalSide, confidence: f64) -> Self {
        Self {
            signal: SignalEvent {
                strategy_id: strategy_id.to_string(),
                symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
                side,
                confidence,
                ts: chrono::Utc::now(),
            },
        }
    }
}

impl AlphaModel for FixedAlphaModel {
    fn on_bar(&mut self, _bar: &Bar) -> Option<SignalEvent> {
        Some(self.signal.clone())
    }
}

fn bar(ts_ms: i64, close: rust_decimal::Decimal) -> Bar {
    Bar::new(ts_ms, close, close, close, close, dec!(1))
}

fn market_slice(
    ts_ms: i64,
    aapl_close: rust_decimal::Decimal,
    msft_close: rust_decimal::Decimal,
) -> MarketSlice {
    MarketSlice::new(
        ts_ms,
        vec![
            SymbolBar::new("US:NASDAQ:AAPL:EQUITY", bar(ts_ms, aapl_close)),
            SymbolBar::new("US:NASDAQ:MSFT:EQUITY", bar(ts_ms, msft_close)),
        ],
    )
}
