use algorithm::{AlgorithmEngine, AlgorithmEngineSettings, EngineEventKind};
use data::Bar;
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

fn bar(ts_ms: i64, close: rust_decimal::Decimal) -> Bar {
    Bar::new(ts_ms, close, close, close, close, dec!(1))
}
