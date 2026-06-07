use algorithm::{AlgorithmEngine, AlgorithmEngineSettings, EngineEventKind};
use alpha::{AlphaModel, CompositeAlphaModel};
use data::Bar;
use events::{SignalEvent, SignalSide};
use rust_decimal_macros::dec;
use strategies::{MovingAverageCrossStrategy, StrategyRuntimeMode};

#[test]
fn algorithm_engine_emits_full_decision_chain_for_selected_symbol() {
    let strategy = MovingAverageCrossStrategy::new("ma", "US:NASDAQ:AAPL:EQUITY", 2, 3);
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

struct FixedAlphaModel {
    signal: SignalEvent,
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
