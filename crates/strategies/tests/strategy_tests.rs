use data::Bar;
use events::SignalSide;
use rust_decimal_macros::dec;
use strategies::{
    MovingAverageCrossStrategy, Strategy, StrategyAssemblyConfig, StrategyContext,
    StrategyRegistry, StrategyRuntimeMode,
};
use universe::{UniverseContext, UniverseSelector};

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
    let registry = StrategyRegistry;
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
    let registry = StrategyRegistry;
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

#[test]
fn strategy_registry_assembles_named_static_universe_and_alpha() {
    let registry = StrategyRegistry;
    let config = StrategyAssemblyConfig {
        strategy_name: "moving_average_cross".to_string(),
        universe_name: "static".to_string(),
        alpha_name: "moving_average_cross".to_string(),
        symbols: vec![
            "US:NASDAQ:AAPL:EQUITY".to_string(),
            "US:NASDAQ:MSFT:EQUITY".to_string(),
        ],
        fast_window: 2,
        slow_window: 3,
    };

    let mut assembly = registry
        .assemble_alpha(config, StrategyRuntimeMode::Paper)
        .unwrap();
    let selected = assembly
        .universe
        .select(&UniverseContext {
            primary_symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
            bar: Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
        })
        .unwrap();

    assert_eq!(assembly.primary_symbol, "US:NASDAQ:AAPL:EQUITY");
    assert_eq!(
        selected,
        vec![
            "US:NASDAQ:AAPL:EQUITY".to_string(),
            "US:NASDAQ:MSFT:EQUITY".to_string()
        ]
    );

    assembly
        .alpha
        .on_bar(&Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)));
    assembly
        .alpha
        .on_bar(&Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)));
    let signal = assembly
        .alpha
        .on_bar(&Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)))
        .unwrap();

    assert_eq!(signal.symbol, "US:NASDAQ:AAPL:EQUITY");
    assert_eq!(signal.side, SignalSide::Buy);
}

#[test]
fn strategy_registry_assembles_independent_alpha_state_per_symbol() {
    let registry = StrategyRegistry;
    let config = StrategyAssemblyConfig {
        strategy_name: "moving_average_cross".to_string(),
        universe_name: "static".to_string(),
        alpha_name: "moving_average_cross".to_string(),
        symbols: vec![
            "US:NASDAQ:AAPL:EQUITY".to_string(),
            "US:NASDAQ:MSFT:EQUITY".to_string(),
        ],
        fast_window: 2,
        slow_window: 3,
    };
    let mut assembly = registry
        .assemble_alpha(config, StrategyRuntimeMode::Backtest)
        .unwrap();

    let mut aapl_signal = None;
    for (ts_ms, close) in [(1, dec!(10)), (2, dec!(11)), (3, dec!(20))] {
        aapl_signal = assembly.alpha.on_bar_for_symbol(
            "US:NASDAQ:AAPL:EQUITY",
            &Bar::new(ts_ms, close, close, close, close, dec!(1)),
        );
    }
    let mut msft_signal = None;
    for (ts_ms, close) in [(1, dec!(30)), (2, dec!(29)), (3, dec!(20))] {
        msft_signal = assembly.alpha.on_bar_for_symbol(
            "US:NASDAQ:MSFT:EQUITY",
            &Bar::new(ts_ms, close, close, close, close, dec!(1)),
        );
    }

    let aapl_signal = aapl_signal.unwrap();
    let msft_signal = msft_signal.unwrap();

    assert_eq!(aapl_signal.symbol, "US:NASDAQ:AAPL:EQUITY");
    assert_eq!(aapl_signal.side, SignalSide::Buy);
    assert_eq!(msft_signal.symbol, "US:NASDAQ:MSFT:EQUITY");
    assert_eq!(msft_signal.side, SignalSide::Sell);
}
