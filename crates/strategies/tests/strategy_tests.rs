use data::Bar;
use events::SignalSide;
use rust_decimal_macros::dec;
use strategies::{
    MovingAverageCrossStrategy, Strategy, StrategyContext, StrategyRegistry, StrategyRuntimeMode,
};

#[test]
fn moving_average_cross_emits_buy_signal() {
    let mut strategy = MovingAverageCrossStrategy::new("ma", "AAPL", 2, 3).unwrap();
    strategy.on_bar(&Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)));
    strategy.on_bar(&Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)));
    let signal = strategy
        .on_bar(&Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)))
        .unwrap();

    assert_eq!(signal.side, SignalSide::Buy);
}

#[test]
fn moving_average_cross_rejects_zero_windows() {
    assert!(MovingAverageCrossStrategy::new("ma", "AAPL", 0, 3).is_err());
    assert!(MovingAverageCrossStrategy::new("ma", "AAPL", 2, 0).is_err());
}

#[test]
fn strategy_registry_creates_moving_average_cross_by_name() {
    let registry = StrategyRegistry::default();
    let context = StrategyContext::new(
        "moving_average_cross",
        "US:NASDAQ:AAPL:EQUITY",
        StrategyRuntimeMode::Backtest,
    );
    let mut strategy = registry
        .create("moving_average_cross", context, 2, 3)
        .unwrap();

    strategy.on_bar(&Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)));
    strategy.on_bar(&Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)));
    let signal = strategy
        .on_bar(&Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)))
        .unwrap();

    assert_eq!(signal.strategy_id, "moving_average_cross");
    assert_eq!(signal.symbol, "US:NASDAQ:AAPL:EQUITY");
    assert_eq!(signal.side, SignalSide::Buy);
}

#[test]
fn strategy_registry_rejects_unknown_strategy_name() {
    let registry = StrategyRegistry::default();
    let context = StrategyContext::new("unknown", "AAPL", StrategyRuntimeMode::Paper);

    let error = match registry.create("unknown", context, 2, 3) {
        Ok(_) => panic!("unknown strategy should fail"),
        Err(error) => error,
    };

    assert_eq!(error.to_string(), "unknown strategy unknown");
}

#[test]
fn strategy_context_preserves_runtime_mode_and_symbol() {
    let context = StrategyContext::new(
        "moving_average_cross",
        "US:NASDAQ:AAPL:EQUITY",
        StrategyRuntimeMode::Paper,
    );

    assert_eq!(context.strategy_id, "moving_average_cross");
    assert_eq!(context.symbol, "US:NASDAQ:AAPL:EQUITY");
    assert_eq!(context.runtime_mode, StrategyRuntimeMode::Paper);
}
