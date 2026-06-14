use data::Bar;
use events::SignalSide;
use feature_store::FeatureRecord;
use rust_decimal_macros::dec;
use strategies::{
    ExponentialMovingAverageCrossStrategy, MovingAverageCrossStrategy,
    PriceChannelBreakoutStrategy, PriceChannelReversionStrategy, PriceMomentumStrategy,
    RelativeStrengthIndexReversionStrategy, Strategy, StrategyAlphaComponentConfig,
    StrategyAlphaConflictResolution, StrategyAlphaGateConfig, StrategyAssemblyConfig,
    StrategyContext, StrategyRegistry, StrategyRuntimeMode, StrategyUniverseFilterConfig,
    StrategyUniverseRankConfig,
};
use universe::UniverseContext;

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
fn exponential_moving_average_cross_emits_buy_signal() {
    let mut strategy = ExponentialMovingAverageCrossStrategy::new("ema", "AAPL", 2, 3).unwrap();
    strategy.on_bar(&Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)));
    let signal = strategy
        .on_bar(&Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)))
        .unwrap();

    assert_eq!(signal.side, SignalSide::Buy);
}

#[test]
fn price_momentum_emits_buy_when_short_horizon_slope_is_stronger() {
    let mut strategy = PriceMomentumStrategy::new("momentum", "AAPL", 1, 2).unwrap();
    strategy.on_bar(&Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)));
    strategy.on_bar(&Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)));
    let signal = strategy
        .on_bar(&Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)))
        .unwrap();

    assert_eq!(signal.strategy_id, "momentum");
    assert_eq!(signal.side, SignalSide::Buy);
}

#[test]
fn price_momentum_emits_sell_when_short_horizon_slope_is_weaker() {
    let mut strategy = PriceMomentumStrategy::new("momentum", "AAPL", 1, 2).unwrap();
    strategy.on_bar(&Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)));
    strategy.on_bar(&Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(19), dec!(1)));
    let signal = strategy
        .on_bar(&Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)))
        .unwrap();

    assert_eq!(signal.strategy_id, "momentum");
    assert_eq!(signal.side, SignalSide::Sell);
}

#[test]
fn price_channel_breakout_emits_buy_after_confirmed_upside_breakout() {
    let mut strategy = PriceChannelBreakoutStrategy::new("channel", "AAPL", 1, 2).unwrap();
    strategy.on_bar(&Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)));
    strategy.on_bar(&Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)));
    let signal = strategy
        .on_bar(&Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)))
        .unwrap();

    assert_eq!(signal.strategy_id, "channel");
    assert_eq!(signal.side, SignalSide::Buy);
}

#[test]
fn price_channel_breakout_emits_sell_after_confirmed_downside_breakout() {
    let mut strategy = PriceChannelBreakoutStrategy::new("channel", "AAPL", 1, 2).unwrap();
    strategy.on_bar(&Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)));
    strategy.on_bar(&Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(19), dec!(1)));
    let signal = strategy
        .on_bar(&Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)))
        .unwrap();

    assert_eq!(signal.strategy_id, "channel");
    assert_eq!(signal.side, SignalSide::Sell);
}

#[test]
fn price_channel_breakout_requires_positive_windows() {
    assert!(PriceChannelBreakoutStrategy::new("channel", "AAPL", 0, 2).is_err());
    assert!(PriceChannelBreakoutStrategy::new("channel", "AAPL", 1, 0).is_err());
}

#[test]
fn price_channel_reversion_emits_sell_after_upside_channel_extension() {
    let mut strategy = PriceChannelReversionStrategy::new("reversion", "AAPL", 1, 2).unwrap();
    strategy.on_bar(&Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)));
    strategy.on_bar(&Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)));
    let signal = strategy
        .on_bar(&Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)))
        .unwrap();

    assert_eq!(signal.strategy_id, "reversion");
    assert_eq!(signal.side, SignalSide::Sell);
}

#[test]
fn price_channel_reversion_emits_buy_after_downside_channel_extension() {
    let mut strategy = PriceChannelReversionStrategy::new("reversion", "AAPL", 1, 2).unwrap();
    strategy.on_bar(&Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)));
    strategy.on_bar(&Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(19), dec!(1)));
    let signal = strategy
        .on_bar(&Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)))
        .unwrap();

    assert_eq!(signal.strategy_id, "reversion");
    assert_eq!(signal.side, SignalSide::Buy);
}

#[test]
fn price_channel_reversion_requires_positive_windows() {
    assert!(PriceChannelReversionStrategy::new("reversion", "AAPL", 0, 2).is_err());
    assert!(PriceChannelReversionStrategy::new("reversion", "AAPL", 1, 0).is_err());
}

#[test]
fn relative_strength_index_reversion_emits_buy_when_oversold() {
    let mut strategy =
        RelativeStrengthIndexReversionStrategy::new("rsi_reversion", "AAPL", 3, 70).unwrap();
    strategy.on_bar(&Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)));
    strategy.on_bar(&Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(9), dec!(1)));
    strategy.on_bar(&Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(8), dec!(1)));
    let signal = strategy
        .on_bar(&Bar::new(4, dec!(1), dec!(1), dec!(1), dec!(7), dec!(1)))
        .unwrap();

    assert_eq!(signal.strategy_id, "rsi_reversion");
    assert_eq!(signal.side, SignalSide::Buy);
}

#[test]
fn relative_strength_index_reversion_emits_sell_when_overbought() {
    let mut strategy =
        RelativeStrengthIndexReversionStrategy::new("rsi_reversion", "AAPL", 3, 70).unwrap();
    strategy.on_bar(&Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)));
    strategy.on_bar(&Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)));
    strategy.on_bar(&Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(12), dec!(1)));
    let signal = strategy
        .on_bar(&Bar::new(4, dec!(1), dec!(1), dec!(1), dec!(13), dec!(1)))
        .unwrap();

    assert_eq!(signal.strategy_id, "rsi_reversion");
    assert_eq!(signal.side, SignalSide::Sell);
}

#[test]
fn relative_strength_index_reversion_requires_positive_period_and_thresholds() {
    assert!(RelativeStrengthIndexReversionStrategy::new("rsi", "AAPL", 0, 70).is_err());
    assert!(RelativeStrengthIndexReversionStrategy::new("rsi", "AAPL", 3, 100).is_err());
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
fn strategy_registry_creates_exponential_moving_average_cross_by_name() {
    let registry = StrategyRegistry;
    let context = StrategyContext::new(
        "exponential_moving_average_cross",
        "US:NASDAQ:AAPL:EQUITY",
        StrategyRuntimeMode::Backtest,
    );
    let mut strategy = registry
        .create("exponential_moving_average_cross", context, 2, 3)
        .unwrap();

    strategy.on_bar(&Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)));
    let signal = strategy
        .on_bar(&Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)))
        .unwrap();

    assert_eq!(signal.strategy_id, "exponential_moving_average_cross");
    assert_eq!(signal.symbol, "US:NASDAQ:AAPL:EQUITY");
    assert_eq!(signal.side, SignalSide::Buy);
}

#[test]
fn strategy_registry_creates_price_momentum_by_name() {
    let registry = StrategyRegistry;
    let context = StrategyContext::new(
        "price_momentum",
        "US:NASDAQ:AAPL:EQUITY",
        StrategyRuntimeMode::Backtest,
    );
    let mut strategy = registry.create("price_momentum", context, 1, 2).unwrap();

    strategy.on_bar(&Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)));
    strategy.on_bar(&Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)));
    let signal = strategy
        .on_bar(&Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)))
        .unwrap();

    assert_eq!(signal.strategy_id, "price_momentum");
    assert_eq!(signal.symbol, "US:NASDAQ:AAPL:EQUITY");
    assert_eq!(signal.side, SignalSide::Buy);
}

#[test]
fn strategy_registry_creates_price_channel_breakout_by_name() {
    let registry = StrategyRegistry;
    let context = StrategyContext::new(
        "price_channel_breakout",
        "US:NASDAQ:AAPL:EQUITY",
        StrategyRuntimeMode::Backtest,
    );
    let mut strategy = registry
        .create("price_channel_breakout", context, 1, 2)
        .unwrap();

    strategy.on_bar(&Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)));
    strategy.on_bar(&Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)));
    let signal = strategy
        .on_bar(&Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)))
        .unwrap();

    assert_eq!(signal.strategy_id, "price_channel_breakout");
    assert_eq!(signal.symbol, "US:NASDAQ:AAPL:EQUITY");
    assert_eq!(signal.side, SignalSide::Buy);
}

#[test]
fn strategy_registry_creates_price_channel_reversion_by_name() {
    let registry = StrategyRegistry;
    let context = StrategyContext::new(
        "price_channel_reversion",
        "US:NASDAQ:AAPL:EQUITY",
        StrategyRuntimeMode::Backtest,
    );
    let mut strategy = registry
        .create("price_channel_reversion", context, 1, 2)
        .unwrap();

    strategy.on_bar(&Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)));
    strategy.on_bar(&Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)));
    let signal = strategy
        .on_bar(&Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)))
        .unwrap();

    assert_eq!(signal.strategy_id, "price_channel_reversion");
    assert_eq!(signal.symbol, "US:NASDAQ:AAPL:EQUITY");
    assert_eq!(signal.side, SignalSide::Sell);
}

#[test]
fn strategy_registry_creates_relative_strength_index_reversion_by_name() {
    let registry = StrategyRegistry;
    let context = StrategyContext::new(
        "relative_strength_index_reversion",
        "US:NASDAQ:AAPL:EQUITY",
        StrategyRuntimeMode::Backtest,
    );
    let mut strategy = registry
        .create("relative_strength_index_reversion", context, 3, 70)
        .unwrap();

    strategy.on_bar(&Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)));
    strategy.on_bar(&Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(9), dec!(1)));
    strategy.on_bar(&Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(8), dec!(1)));
    let signal = strategy
        .on_bar(&Bar::new(4, dec!(1), dec!(1), dec!(1), dec!(7), dec!(1)))
        .unwrap();

    assert_eq!(signal.strategy_id, "relative_strength_index_reversion");
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
        universe_filter: StrategyUniverseFilterConfig::default(),
        alpha_components: Vec::new(),
        alpha_conflict_resolution: StrategyAlphaConflictResolution::HighestConfidence,
        alpha_gate: None,
        fast_window: 2,
        slow_window: 3,
    };

    let mut assembly = registry
        .assemble_alpha(config, StrategyRuntimeMode::Paper)
        .unwrap();
    let selected = assembly
        .universe
        .select(&UniverseContext::new(
            "US:NASDAQ:AAPL:EQUITY",
            Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
        ))
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
fn strategy_registry_assembles_exponential_moving_average_cross_alpha() {
    let registry = StrategyRegistry;
    let config = StrategyAssemblyConfig {
        strategy_name: "exponential_moving_average_cross".to_string(),
        universe_name: "static".to_string(),
        alpha_name: "exponential_moving_average_cross".to_string(),
        symbols: vec!["US:NASDAQ:AAPL:EQUITY".to_string()],
        universe_filter: StrategyUniverseFilterConfig::default(),
        alpha_components: Vec::new(),
        alpha_conflict_resolution: StrategyAlphaConflictResolution::HighestConfidence,
        alpha_gate: None,
        fast_window: 2,
        slow_window: 3,
    };
    let mut assembly = registry
        .assemble_alpha(config, StrategyRuntimeMode::Backtest)
        .unwrap();

    assembly
        .alpha
        .on_bar(&Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)));
    let signal = assembly
        .alpha
        .on_bar(&Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)))
        .unwrap();

    assert_eq!(signal.strategy_id, "exponential_moving_average_cross");
    assert_eq!(signal.symbol, "US:NASDAQ:AAPL:EQUITY");
    assert_eq!(signal.side, SignalSide::Buy);
}

#[test]
fn strategy_registry_assembles_price_momentum_alpha() {
    let registry = StrategyRegistry;
    let config = StrategyAssemblyConfig {
        strategy_name: "price_momentum".to_string(),
        universe_name: "static".to_string(),
        alpha_name: "price_momentum".to_string(),
        symbols: vec!["US:NASDAQ:AAPL:EQUITY".to_string()],
        universe_filter: StrategyUniverseFilterConfig::default(),
        alpha_components: Vec::new(),
        alpha_conflict_resolution: StrategyAlphaConflictResolution::HighestConfidence,
        alpha_gate: None,
        fast_window: 1,
        slow_window: 2,
    };
    let mut assembly = registry
        .assemble_alpha(config, StrategyRuntimeMode::Backtest)
        .unwrap();

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

    assert_eq!(signal.strategy_id, "price_momentum");
    assert_eq!(signal.symbol, "US:NASDAQ:AAPL:EQUITY");
    assert_eq!(signal.side, SignalSide::Buy);
}

#[test]
fn strategy_registry_assembles_price_channel_breakout_alpha() {
    let registry = StrategyRegistry;
    let config = StrategyAssemblyConfig {
        strategy_name: "price_channel_breakout".to_string(),
        universe_name: "static".to_string(),
        alpha_name: "price_channel_breakout".to_string(),
        symbols: vec!["US:NASDAQ:AAPL:EQUITY".to_string()],
        universe_filter: StrategyUniverseFilterConfig::default(),
        alpha_components: Vec::new(),
        alpha_conflict_resolution: StrategyAlphaConflictResolution::HighestConfidence,
        alpha_gate: None,
        fast_window: 1,
        slow_window: 2,
    };
    let mut assembly = registry
        .assemble_alpha(config, StrategyRuntimeMode::Backtest)
        .unwrap();

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

    assert_eq!(signal.strategy_id, "price_channel_breakout");
    assert_eq!(signal.symbol, "US:NASDAQ:AAPL:EQUITY");
    assert_eq!(signal.side, SignalSide::Buy);
}

#[test]
fn strategy_registry_assembles_price_channel_reversion_alpha() {
    let registry = StrategyRegistry;
    let config = StrategyAssemblyConfig {
        strategy_name: "price_channel_reversion".to_string(),
        universe_name: "static".to_string(),
        alpha_name: "price_channel_reversion".to_string(),
        symbols: vec!["US:NASDAQ:AAPL:EQUITY".to_string()],
        universe_filter: StrategyUniverseFilterConfig::default(),
        alpha_components: Vec::new(),
        alpha_conflict_resolution: StrategyAlphaConflictResolution::HighestConfidence,
        alpha_gate: None,
        fast_window: 1,
        slow_window: 2,
    };
    let mut assembly = registry
        .assemble_alpha(config, StrategyRuntimeMode::Backtest)
        .unwrap();

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

    assert_eq!(signal.strategy_id, "price_channel_reversion");
    assert_eq!(signal.symbol, "US:NASDAQ:AAPL:EQUITY");
    assert_eq!(signal.side, SignalSide::Sell);
}

#[test]
fn strategy_registry_assembles_relative_strength_index_reversion_alpha() {
    let registry = StrategyRegistry;
    let config = StrategyAssemblyConfig {
        strategy_name: "relative_strength_index_reversion".to_string(),
        universe_name: "static".to_string(),
        alpha_name: "relative_strength_index_reversion".to_string(),
        symbols: vec!["US:NASDAQ:AAPL:EQUITY".to_string()],
        universe_filter: StrategyUniverseFilterConfig::default(),
        alpha_components: Vec::new(),
        alpha_conflict_resolution: StrategyAlphaConflictResolution::HighestConfidence,
        alpha_gate: None,
        fast_window: 3,
        slow_window: 70,
    };
    let mut assembly = registry
        .assemble_alpha(config, StrategyRuntimeMode::Backtest)
        .unwrap();

    for (ts_ms, close) in [(1, dec!(10)), (2, dec!(9)), (3, dec!(8))] {
        assembly
            .alpha
            .on_bar(&Bar::new(ts_ms, close, close, close, close, dec!(1)));
    }
    let signal = assembly
        .alpha
        .on_bar(&Bar::new(4, dec!(7), dec!(7), dec!(7), dec!(7), dec!(1)))
        .unwrap();

    assert_eq!(signal.strategy_id, "relative_strength_index_reversion");
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
        universe_filter: StrategyUniverseFilterConfig::default(),
        alpha_components: Vec::new(),
        alpha_conflict_resolution: StrategyAlphaConflictResolution::HighestConfidence,
        alpha_gate: None,
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

#[test]
fn strategy_registry_assembles_filtered_universe() {
    let registry = StrategyRegistry;
    let config = StrategyAssemblyConfig {
        strategy_name: "moving_average_cross".to_string(),
        universe_name: "filtered".to_string(),
        alpha_name: "moving_average_cross".to_string(),
        symbols: vec![
            "US:NASDAQ:AAPL:EQUITY".to_string(),
            "US:NASDAQ:MSFT:EQUITY".to_string(),
            "US:NYSE:IBM:EQUITY".to_string(),
        ],
        universe_filter: StrategyUniverseFilterConfig {
            include_symbols: Vec::new(),
            exclude_symbols: vec!["US:NASDAQ:MSFT:EQUITY".to_string()],
            symbol_prefixes: vec!["US:NASDAQ:".to_string()],
            require_current_data: false,
            max_symbols: None,
            feature_rank: None,
        },
        alpha_components: Vec::new(),
        alpha_conflict_resolution: StrategyAlphaConflictResolution::HighestConfidence,
        alpha_gate: None,
        fast_window: 2,
        slow_window: 3,
    };

    let assembly = registry
        .assemble_alpha(config, StrategyRuntimeMode::Backtest)
        .unwrap();
    let selected = assembly
        .universe
        .select(&UniverseContext::new(
            "US:NASDAQ:AAPL:EQUITY",
            Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
        ))
        .unwrap();

    assert_eq!(selected, vec!["US:NASDAQ:AAPL:EQUITY".to_string()]);
}

#[test]
fn strategy_registry_assembles_ranked_universe() {
    let registry = StrategyRegistry;
    let config = StrategyAssemblyConfig {
        strategy_name: "moving_average_cross".to_string(),
        universe_name: "ranked".to_string(),
        alpha_name: "moving_average_cross".to_string(),
        symbols: vec![
            "US:NASDAQ:AAPL:EQUITY".to_string(),
            "US:NASDAQ:MSFT:EQUITY".to_string(),
            "US:NYSE:IBM:EQUITY".to_string(),
        ],
        universe_filter: StrategyUniverseFilterConfig {
            max_symbols: Some(2),
            require_current_data: true,
            ..StrategyUniverseFilterConfig::default()
        },
        alpha_components: Vec::new(),
        alpha_conflict_resolution: StrategyAlphaConflictResolution::HighestConfidence,
        alpha_gate: None,
        fast_window: 2,
        slow_window: 3,
    };

    let assembly = registry
        .assemble_alpha(config, StrategyRuntimeMode::Backtest)
        .unwrap();
    let selected = assembly
        .universe
        .select(
            &UniverseContext::new(
                "US:NASDAQ:AAPL:EQUITY",
                Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
            )
            .with_available_symbols(vec![
                "US:NYSE:IBM:EQUITY".to_string(),
                "US:NASDAQ:MSFT:EQUITY".to_string(),
                "US:NASDAQ:AAPL:EQUITY".to_string(),
            ]),
        )
        .unwrap();

    assert_eq!(
        selected,
        vec![
            "US:NASDAQ:AAPL:EQUITY".to_string(),
            "US:NASDAQ:MSFT:EQUITY".to_string()
        ]
    );
}

#[test]
fn strategy_registry_assembles_feature_ranked_universe_by_latest_feature_value() {
    let registry = StrategyRegistry;
    let config = StrategyAssemblyConfig {
        strategy_name: "moving_average_cross".to_string(),
        universe_name: "feature_ranked".to_string(),
        alpha_name: "moving_average_cross".to_string(),
        symbols: vec![
            "US:NASDAQ:AAPL:EQUITY".to_string(),
            "US:NASDAQ:MSFT:EQUITY".to_string(),
            "US:NYSE:IBM:EQUITY".to_string(),
        ],
        universe_filter: StrategyUniverseFilterConfig {
            max_symbols: Some(2),
            require_current_data: true,
            feature_rank: Some(StrategyUniverseRankConfig {
                run_id: "research-rank".to_string(),
                feature_name: "quality_score".to_string(),
                version: Some("v1".to_string()),
                descending: true,
                records: vec![
                    FeatureRecord::new(
                        "research-rank",
                        "US:NASDAQ:AAPL:EQUITY",
                        1,
                        "quality_score",
                        dec!(0.4),
                        "v1",
                    ),
                    FeatureRecord::new(
                        "research-rank",
                        "US:NASDAQ:AAPL:EQUITY",
                        3,
                        "quality_score",
                        dec!(0.99),
                        "v1",
                    ),
                    FeatureRecord::new(
                        "research-rank",
                        "US:NASDAQ:MSFT:EQUITY",
                        2,
                        "quality_score",
                        dec!(0.8),
                        "v1",
                    ),
                    FeatureRecord::new(
                        "research-rank",
                        "US:NYSE:IBM:EQUITY",
                        2,
                        "quality_score",
                        dec!(0.6),
                        "v1",
                    ),
                    FeatureRecord::new(
                        "research-rank",
                        "US:NYSE:IBM:EQUITY",
                        1,
                        "quality_score",
                        dec!(0.9),
                        "v2",
                    ),
                ],
            }),
            ..StrategyUniverseFilterConfig::default()
        },
        alpha_components: Vec::new(),
        alpha_conflict_resolution: StrategyAlphaConflictResolution::HighestConfidence,
        alpha_gate: None,
        fast_window: 2,
        slow_window: 3,
    };

    let assembly = registry
        .assemble_alpha(config, StrategyRuntimeMode::Backtest)
        .unwrap();
    let selected = assembly
        .universe
        .select(
            &UniverseContext::new(
                "US:NASDAQ:AAPL:EQUITY",
                Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
            )
            .with_available_symbols(vec![
                "US:NYSE:IBM:EQUITY".to_string(),
                "US:NASDAQ:MSFT:EQUITY".to_string(),
                "US:NASDAQ:AAPL:EQUITY".to_string(),
            ]),
        )
        .unwrap();

    assert_eq!(
        selected,
        vec![
            "US:NASDAQ:MSFT:EQUITY".to_string(),
            "US:NYSE:IBM:EQUITY".to_string()
        ]
    );
}

#[test]
fn strategy_registry_assembles_weighted_composite_alpha() {
    let registry = StrategyRegistry;
    let config = StrategyAssemblyConfig {
        strategy_name: "moving_average_cross".to_string(),
        universe_name: "static".to_string(),
        alpha_name: "moving_average_cross".to_string(),
        symbols: vec!["US:NASDAQ:AAPL:EQUITY".to_string()],
        universe_filter: StrategyUniverseFilterConfig::default(),
        alpha_components: vec![
            StrategyAlphaComponentConfig {
                name: "moving_average_cross".to_string(),
                category: None,
                fast_window: Some(2),
                slow_window: Some(3),
                weight: 0.25,
            },
            StrategyAlphaComponentConfig {
                name: "moving_average_cross".to_string(),
                category: None,
                fast_window: Some(2),
                slow_window: Some(3),
                weight: 0.5,
            },
        ],
        alpha_conflict_resolution: StrategyAlphaConflictResolution::HighestConfidence,
        alpha_gate: None,
        fast_window: 2,
        slow_window: 3,
    };
    let mut assembly = registry
        .assemble_alpha(config, StrategyRuntimeMode::Backtest)
        .unwrap();

    for close in [dec!(10), dec!(11)] {
        assembly
            .alpha
            .on_bar(&Bar::new(1, close, close, close, close, dec!(1)));
    }
    let signal = assembly
        .alpha
        .on_bar(&Bar::new(
            3,
            dec!(20),
            dec!(20),
            dec!(20),
            dec!(20),
            dec!(1),
        ))
        .unwrap();

    assert_eq!(signal.symbol, "US:NASDAQ:AAPL:EQUITY");
    assert_eq!(signal.side, SignalSide::Buy);
    assert_eq!(signal.confidence, 0.4);
}

#[test]
fn strategy_registry_assembles_net_signal_composite_alpha() {
    let registry = StrategyRegistry;
    let config = StrategyAssemblyConfig {
        strategy_name: "moving_average_cross".to_string(),
        universe_name: "static".to_string(),
        alpha_name: "moving_average_cross".to_string(),
        symbols: vec!["US:NASDAQ:AAPL:EQUITY".to_string()],
        universe_filter: StrategyUniverseFilterConfig::default(),
        alpha_components: vec![
            StrategyAlphaComponentConfig {
                name: "moving_average_cross".to_string(),
                category: None,
                fast_window: Some(1),
                slow_window: Some(2),
                weight: 1.0,
            },
            StrategyAlphaComponentConfig {
                name: "moving_average_cross".to_string(),
                category: None,
                fast_window: Some(2),
                slow_window: Some(1),
                weight: 0.25,
            },
        ],
        alpha_conflict_resolution: StrategyAlphaConflictResolution::NetSignal,
        alpha_gate: None,
        fast_window: 2,
        slow_window: 3,
    };
    let mut assembly = registry
        .assemble_alpha(config, StrategyRuntimeMode::Backtest)
        .unwrap();

    assembly.alpha.on_bar(&Bar::new(
        1,
        dec!(10),
        dec!(10),
        dec!(10),
        dec!(10),
        dec!(1),
    ));
    let signal = assembly
        .alpha
        .on_bar(&Bar::new(
            2,
            dec!(20),
            dec!(20),
            dec!(20),
            dec!(20),
            dec!(1),
        ))
        .unwrap();

    assert_eq!(signal.symbol, "US:NASDAQ:AAPL:EQUITY");
    assert_eq!(signal.side, SignalSide::Buy);
    assert!((signal.confidence - 0.6).abs() < 1e-9);
}

#[test]
fn strategy_registry_assembles_majority_vote_composite_alpha() {
    let registry = StrategyRegistry;
    let config = StrategyAssemblyConfig {
        strategy_name: "moving_average_cross".to_string(),
        universe_name: "static".to_string(),
        alpha_name: "moving_average_cross".to_string(),
        symbols: vec!["US:NASDAQ:AAPL:EQUITY".to_string()],
        universe_filter: StrategyUniverseFilterConfig::default(),
        alpha_components: vec![
            StrategyAlphaComponentConfig {
                name: "moving_average_cross".to_string(),
                category: None,
                fast_window: Some(1),
                slow_window: Some(2),
                weight: 0.25,
            },
            StrategyAlphaComponentConfig {
                name: "moving_average_cross".to_string(),
                category: None,
                fast_window: Some(1),
                slow_window: Some(2),
                weight: 0.5,
            },
            StrategyAlphaComponentConfig {
                name: "moving_average_cross".to_string(),
                category: None,
                fast_window: Some(2),
                slow_window: Some(1),
                weight: 1.0,
            },
        ],
        alpha_conflict_resolution: StrategyAlphaConflictResolution::MajorityVote,
        alpha_gate: None,
        fast_window: 2,
        slow_window: 3,
    };
    let mut assembly = registry
        .assemble_alpha(config, StrategyRuntimeMode::Backtest)
        .unwrap();

    assembly.alpha.on_bar(&Bar::new(
        1,
        dec!(10),
        dec!(10),
        dec!(10),
        dec!(10),
        dec!(1),
    ));
    let signal = assembly
        .alpha
        .on_bar(&Bar::new(
            2,
            dec!(20),
            dec!(20),
            dec!(20),
            dec!(20),
            dec!(1),
        ))
        .unwrap();

    assert_eq!(signal.symbol, "US:NASDAQ:AAPL:EQUITY");
    assert_eq!(signal.side, SignalSide::Buy);
    assert!((signal.confidence - 0.3).abs() < 1e-9);
}

#[test]
fn strategy_registry_assembles_category_majority_composite_alpha() {
    let registry = StrategyRegistry;
    let config = StrategyAssemblyConfig {
        strategy_name: "moving_average_cross".to_string(),
        universe_name: "static".to_string(),
        alpha_name: "moving_average_cross".to_string(),
        symbols: vec!["US:NASDAQ:AAPL:EQUITY".to_string()],
        universe_filter: StrategyUniverseFilterConfig::default(),
        alpha_components: vec![
            StrategyAlphaComponentConfig {
                name: "moving_average_cross".to_string(),
                category: Some("trend".to_string()),
                fast_window: Some(2),
                slow_window: Some(1),
                weight: 0.25,
            },
            StrategyAlphaComponentConfig {
                name: "moving_average_cross".to_string(),
                category: Some("trend".to_string()),
                fast_window: Some(2),
                slow_window: Some(1),
                weight: 0.5,
            },
            StrategyAlphaComponentConfig {
                name: "moving_average_cross".to_string(),
                category: Some("mean_reversion".to_string()),
                fast_window: Some(1),
                slow_window: Some(2),
                weight: 1.0,
            },
            StrategyAlphaComponentConfig {
                name: "moving_average_cross".to_string(),
                category: Some("quality".to_string()),
                fast_window: Some(1),
                slow_window: Some(2),
                weight: 0.5,
            },
        ],
        alpha_conflict_resolution: StrategyAlphaConflictResolution::CategoryMajority,
        alpha_gate: None,
        fast_window: 2,
        slow_window: 3,
    };
    let mut assembly = registry
        .assemble_alpha(config, StrategyRuntimeMode::Backtest)
        .unwrap();

    assembly.alpha.on_bar(&Bar::new(
        1,
        dec!(10),
        dec!(10),
        dec!(10),
        dec!(10),
        dec!(1),
    ));
    let signal = assembly
        .alpha
        .on_bar(&Bar::new(
            2,
            dec!(20),
            dec!(20),
            dec!(20),
            dec!(20),
            dec!(1),
        ))
        .unwrap();

    assert_eq!(signal.symbol, "US:NASDAQ:AAPL:EQUITY");
    assert_eq!(signal.side, SignalSide::Buy);
    assert!((signal.confidence - 0.6).abs() < 1e-9);
}

#[test]
fn strategy_registry_gates_alpha_signals_by_latest_feature_value() {
    let registry = StrategyRegistry;
    let config = StrategyAssemblyConfig {
        strategy_name: "moving_average_cross".to_string(),
        universe_name: "static".to_string(),
        alpha_name: "moving_average_cross".to_string(),
        symbols: vec![
            "US:NASDAQ:AAPL:EQUITY".to_string(),
            "US:NASDAQ:MSFT:EQUITY".to_string(),
        ],
        universe_filter: StrategyUniverseFilterConfig::default(),
        alpha_components: Vec::new(),
        alpha_conflict_resolution: StrategyAlphaConflictResolution::HighestConfidence,
        alpha_gate: Some(StrategyAlphaGateConfig {
            run_id: "research-run".to_string(),
            feature_name: "quality_score".to_string(),
            version: None,
            min_value: Some(dec!(0.7)),
            max_value: None,
            records: vec![
                FeatureRecord::new(
                    "research-run",
                    "US:NASDAQ:AAPL:EQUITY",
                    3,
                    "quality_score",
                    dec!(0.8),
                    "v1",
                ),
                FeatureRecord::new(
                    "research-run",
                    "US:NASDAQ:MSFT:EQUITY",
                    3,
                    "quality_score",
                    dec!(0.2),
                    "v1",
                ),
            ],
        }),
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
    assert_eq!(aapl_signal.symbol, "US:NASDAQ:AAPL:EQUITY");
    assert_eq!(aapl_signal.side, SignalSide::Buy);
    assert!(msft_signal.is_none());
}

#[test]
fn strategy_registry_gates_alpha_signals_by_feature_version() {
    let registry = StrategyRegistry;
    let config = StrategyAssemblyConfig {
        strategy_name: "moving_average_cross".to_string(),
        universe_name: "static".to_string(),
        alpha_name: "moving_average_cross".to_string(),
        symbols: vec!["US:NASDAQ:AAPL:EQUITY".to_string()],
        universe_filter: StrategyUniverseFilterConfig::default(),
        alpha_components: Vec::new(),
        alpha_conflict_resolution: StrategyAlphaConflictResolution::HighestConfidence,
        alpha_gate: Some(StrategyAlphaGateConfig {
            run_id: "research-run".to_string(),
            feature_name: "quality_score".to_string(),
            version: Some("v2".to_string()),
            min_value: Some(dec!(0.7)),
            max_value: None,
            records: vec![
                FeatureRecord::new(
                    "research-run",
                    "US:NASDAQ:AAPL:EQUITY",
                    12,
                    "quality_score",
                    dec!(0.1),
                    "v1",
                ),
                FeatureRecord::new(
                    "research-run",
                    "US:NASDAQ:AAPL:EQUITY",
                    11,
                    "quality_score",
                    dec!(0.8),
                    "v2",
                ),
            ],
        }),
        fast_window: 2,
        slow_window: 3,
    };
    let mut assembly = registry
        .assemble_alpha(config, StrategyRuntimeMode::Backtest)
        .unwrap();

    let mut signal = None;
    for (ts_ms, close) in [(10, dec!(10)), (11, dec!(11)), (12, dec!(20))] {
        signal = assembly.alpha.on_bar_for_symbol(
            "US:NASDAQ:AAPL:EQUITY",
            &Bar::new(ts_ms, close, close, close, close, dec!(1)),
        );
    }

    let signal = signal.unwrap();
    assert_eq!(signal.symbol, "US:NASDAQ:AAPL:EQUITY");
    assert_eq!(signal.side, SignalSide::Buy);
}
