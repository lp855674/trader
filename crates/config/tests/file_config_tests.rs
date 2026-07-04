use config::{AppConfig, RuntimeMode};

#[test]
fn loads_config_from_file() {
    let config = AppConfig::from_toml_file("../../configs/backtest/ma_cross.toml").unwrap();

    assert_eq!(config.runtime.mode, RuntimeMode::Backtest);
    assert_eq!(config.runtime.run_id, "sample-ma-cross");
    assert_eq!(config.database.url, "sqlite://data/trader.sqlite");
    assert_eq!(config.data.source, "csv");
    assert_eq!(config.portfolio.order_qty, "1");
    assert_eq!(config.portfolio.max_abs_qty, "100");
    assert_eq!(config.paper.account_id, "paper");
    assert_eq!(config.paper.slippage_bps, "25");
    assert_eq!(config.paper.fee_bps, "10");
}

#[test]
fn loads_multi_symbol_backtest_config_from_file() {
    let config =
        AppConfig::from_toml_file("../../configs/backtest/multi_symbol_ma_cross.toml").unwrap();

    assert_eq!(config.runtime.mode, RuntimeMode::Backtest);
    assert_eq!(config.runtime.run_id, "sample-multi-symbol-ma-cross");
    assert_eq!(config.data.inputs.len(), 2);
    assert_eq!(config.data.inputs[0].source, "csv");
    assert_eq!(config.data.inputs[0].path, "datasets/sample/aapl_1d.csv");
    assert_eq!(config.data.inputs[1].symbol, "US:NASDAQ:MSFT:EQUITY");
    assert_eq!(
        config.strategy.symbols,
        vec!["US:NASDAQ:AAPL:EQUITY", "US:NASDAQ:MSFT:EQUITY"]
    );
}

#[test]
fn loads_ema_cross_backtest_config_from_file() {
    let config = AppConfig::from_toml_file("../../configs/backtest/ema_cross.toml").unwrap();

    assert_eq!(config.runtime.mode, RuntimeMode::Backtest);
    assert_eq!(config.runtime.run_id, "sample-ema-cross");
    assert_eq!(config.strategy.name, "exponential_moving_average_cross");
    assert_eq!(config.strategy.alpha, "exponential_moving_average_cross");
    assert_eq!(config.strategy.fast_window, 2);
    assert_eq!(config.strategy.slow_window, 3);
    assert_eq!(config.risk.allow_short, Some(true));
    assert!(config.effective_allow_short());
}

#[test]
fn loads_price_momentum_backtest_config_from_file() {
    let config = AppConfig::from_toml_file("../../configs/backtest/price_momentum.toml").unwrap();

    assert_eq!(config.runtime.mode, RuntimeMode::Backtest);
    assert_eq!(config.runtime.run_id, "sample-price-momentum");
    assert_eq!(config.strategy.name, "price_momentum");
    assert_eq!(config.strategy.alpha, "price_momentum");
    assert_eq!(config.strategy.fast_window, 1);
    assert_eq!(config.strategy.slow_window, 2);
}

#[test]
fn loads_price_channel_breakout_backtest_config_from_file() {
    let config =
        AppConfig::from_toml_file("../../configs/backtest/price_channel_breakout.toml").unwrap();

    assert_eq!(config.runtime.mode, RuntimeMode::Backtest);
    assert_eq!(config.runtime.run_id, "sample-price-channel-breakout");
    assert_eq!(config.strategy.name, "price_channel_breakout");
    assert_eq!(config.strategy.alpha, "price_channel_breakout");
    assert_eq!(config.strategy.fast_window, 1);
    assert_eq!(config.strategy.slow_window, 2);
}

#[test]
fn loads_price_channel_reversion_backtest_config_from_file() {
    let config =
        AppConfig::from_toml_file("../../configs/backtest/price_channel_reversion.toml").unwrap();

    assert_eq!(config.runtime.mode, RuntimeMode::Backtest);
    assert_eq!(config.runtime.run_id, "sample-price-channel-reversion");
    assert_eq!(config.strategy.name, "price_channel_reversion");
    assert_eq!(config.strategy.alpha, "price_channel_reversion");
    assert_eq!(config.strategy.fast_window, 1);
    assert_eq!(config.strategy.slow_window, 2);
}

#[test]
fn loads_relative_strength_index_reversion_backtest_config_from_file() {
    let config = AppConfig::from_toml_file("../../configs/backtest/rsi_reversion.toml").unwrap();

    assert_eq!(config.runtime.mode, RuntimeMode::Backtest);
    assert_eq!(config.runtime.run_id, "sample-rsi-reversion");
    assert_eq!(config.strategy.name, "relative_strength_index_reversion");
    assert_eq!(config.strategy.alpha, "relative_strength_index_reversion");
    assert_eq!(config.strategy.fast_window, 3);
    assert_eq!(config.strategy.slow_window, 70);
}

#[test]
fn loads_filtered_universe_backtest_config_from_file() {
    let config =
        AppConfig::from_toml_file("../../configs/backtest/filtered_universe_ma_cross.toml")
            .unwrap();

    assert_eq!(config.runtime.mode, RuntimeMode::Backtest);
    assert_eq!(config.runtime.run_id, "sample-filtered-universe-ma-cross");
    assert_eq!(config.strategy.universe, "filtered");
    assert_eq!(
        config.strategy.universe_filter.exclude_symbols,
        vec!["US:NASDAQ:MSFT:EQUITY"]
    );
    assert_eq!(config.data.inputs.len(), 2);
}

#[test]
fn loads_ranked_universe_backtest_config_from_file() {
    let config =
        AppConfig::from_toml_file("../../configs/backtest/ranked_universe_ma_cross.toml").unwrap();

    assert_eq!(config.runtime.mode, RuntimeMode::Backtest);
    assert_eq!(config.runtime.run_id, "sample-ranked-universe-ma-cross");
    assert_eq!(config.strategy.universe, "ranked");
    assert_eq!(config.strategy.universe_filter.max_symbols, Some(1));
    assert_eq!(config.data.inputs.len(), 2);
}

#[test]
fn loads_feature_ranked_universe_backtest_config_from_file() {
    let config =
        AppConfig::from_toml_file("../../configs/backtest/feature_ranked_universe_ma_cross.toml")
            .unwrap();

    assert_eq!(config.runtime.mode, RuntimeMode::Backtest);
    assert_eq!(
        config.runtime.run_id,
        "sample-feature-ranked-universe-ma-cross"
    );
    assert_eq!(config.strategy.universe, "feature_ranked");
    assert_eq!(config.strategy.universe_filter.max_symbols, Some(1));
    let rank = config.strategy.universe_rank.unwrap();
    assert_eq!(rank.path, "datasets/features/multi_symbol_sma_1.parquet");
    assert_eq!(
        rank.manifest_path.as_deref(),
        Some("datasets/features/multi_symbol_sma_1.manifest.json")
    );
    assert_eq!(rank.feature_name, "sma_close_1");
    assert_eq!(rank.version.as_deref(), Some("v1"));
}

#[test]
fn loads_weighted_alpha_backtest_config_from_file() {
    let config =
        AppConfig::from_toml_file("../../configs/backtest/weighted_alpha_ma_cross.toml").unwrap();

    assert_eq!(config.runtime.mode, RuntimeMode::Backtest);
    assert_eq!(config.runtime.run_id, "sample-weighted-alpha-ma-cross");
    assert_eq!(
        config.strategy.alpha_conflict_resolution,
        "highest_confidence"
    );
    assert_eq!(config.strategy.alpha_components.len(), 2);
    assert_eq!(config.strategy.alpha_components[0].weight, 0.25);
    assert_eq!(config.strategy.alpha_components[1].weight, 0.5);
}

#[test]
fn loads_net_signal_alpha_backtest_config_from_file() {
    let config =
        AppConfig::from_toml_file("../../configs/backtest/net_signal_alpha_ma_cross.toml").unwrap();

    assert_eq!(config.runtime.mode, RuntimeMode::Backtest);
    assert_eq!(config.runtime.run_id, "sample-net-signal-alpha-ma-cross");
    assert_eq!(config.strategy.alpha_conflict_resolution, "net_signal");
    assert_eq!(config.strategy.alpha_components.len(), 2);
    assert_eq!(config.strategy.alpha_components[0].weight, 1.0);
    assert_eq!(config.strategy.alpha_components[1].weight, 0.25);
}

#[test]
fn loads_majority_vote_alpha_backtest_config_from_file() {
    let config =
        AppConfig::from_toml_file("../../configs/backtest/majority_vote_alpha_ma_cross.toml")
            .unwrap();

    assert_eq!(config.runtime.mode, RuntimeMode::Backtest);
    assert_eq!(config.runtime.run_id, "sample-majority-vote-alpha-ma-cross");
    assert_eq!(config.strategy.alpha_conflict_resolution, "majority_vote");
    assert_eq!(config.strategy.alpha_components.len(), 3);
    assert_eq!(config.strategy.alpha_components[0].weight, 0.25);
    assert_eq!(config.strategy.alpha_components[1].weight, 0.5);
    assert_eq!(config.strategy.alpha_components[2].weight, 1.0);
}

#[test]
fn loads_category_majority_alpha_backtest_config_from_file() {
    let config =
        AppConfig::from_toml_file("../../configs/backtest/category_majority_alpha_ma_cross.toml")
            .unwrap();

    assert_eq!(config.runtime.mode, RuntimeMode::Backtest);
    assert_eq!(
        config.runtime.run_id,
        "sample-category-majority-alpha-ma-cross"
    );
    assert_eq!(
        config.strategy.alpha_conflict_resolution,
        "category_majority"
    );
    assert_eq!(config.strategy.alpha_components.len(), 4);
    assert_eq!(
        config.strategy.alpha_components[0].category.as_deref(),
        Some("trend")
    );
    assert_eq!(
        config.strategy.alpha_components[2].category.as_deref(),
        Some("mean_reversion")
    );
    assert_eq!(
        config.strategy.alpha_components[3].category.as_deref(),
        Some("quality")
    );
}

#[test]
fn loads_sma_feature_gate_backtest_config_from_file() {
    let config = AppConfig::from_toml_file("../../configs/backtest/sma_feature_gate.toml").unwrap();

    assert_eq!(config.runtime.mode, RuntimeMode::Backtest);
    assert_eq!(config.runtime.run_id, "sample-sma-feature-gate");
    let gate = config.strategy.alpha_gate.unwrap();
    assert_eq!(gate.path, "datasets/features/aapl_sma_2.parquet");
    assert_eq!(
        gate.manifest_path.as_deref(),
        Some("datasets/features/aapl_sma_2.manifest.json")
    );
    assert_eq!(gate.feature_name, "sma_close_2");
}

#[test]
fn loads_rsi_feature_gate_backtest_config_from_file() {
    let config = AppConfig::from_toml_file("../../configs/backtest/rsi_feature_gate.toml").unwrap();

    assert_eq!(config.runtime.mode, RuntimeMode::Backtest);
    assert_eq!(config.runtime.run_id, "sample-rsi-feature-gate");
    let gate = config.strategy.alpha_gate.unwrap();
    assert_eq!(gate.path, "datasets/features/aapl_rsi_3.parquet");
    assert_eq!(
        gate.manifest_path.as_deref(),
        Some("datasets/features/aapl_rsi_3.manifest.json")
    );
    assert_eq!(gate.feature_name, "rsi_close_3");
    assert_eq!(gate.version.as_deref(), Some("v1"));
    assert_eq!(gate.build_indicator.as_deref(), Some("rsi"));
    assert_eq!(gate.build_period, Some(3));
    assert_eq!(gate.build_value_column.as_deref(), Some("close"));
    assert_eq!(gate.max_value.as_deref(), Some("30"));
}

#[test]
fn loads_sma_feature_gate_suppressed_backtest_config_from_file() {
    let config =
        AppConfig::from_toml_file("../../configs/backtest/sma_feature_gate_suppressed.toml")
            .unwrap();

    assert_eq!(config.runtime.mode, RuntimeMode::Backtest);
    assert_eq!(config.runtime.run_id, "sample-sma-feature-gate-suppressed");
    let gate = config.strategy.alpha_gate.unwrap();
    assert_eq!(gate.path, "datasets/features/aapl_sma_2.parquet");
    assert_eq!(
        gate.manifest_path.as_deref(),
        Some("datasets/features/aapl_sma_2.manifest.json")
    );
    assert_eq!(gate.feature_name, "sma_close_2");
    assert_eq!(gate.min_value.as_deref(), Some("1000"));
}

#[test]
fn loads_multi_symbol_sma_feature_gate_backtest_config_from_file() {
    let config =
        AppConfig::from_toml_file("../../configs/backtest/multi_symbol_sma_feature_gate.toml")
            .unwrap();

    assert_eq!(config.runtime.mode, RuntimeMode::Backtest);
    assert_eq!(
        config.runtime.run_id,
        "sample-multi-symbol-sma-feature-gate"
    );
    assert_eq!(config.data.inputs.len(), 2);
    assert_eq!(
        config.strategy.symbols,
        vec!["US:NASDAQ:AAPL:EQUITY", "US:NASDAQ:MSFT:EQUITY"]
    );
    let gate = config.strategy.alpha_gate.unwrap();
    assert_eq!(gate.path, "datasets/features/multi_symbol_sma_2.parquet");
    assert_eq!(
        gate.manifest_path.as_deref(),
        Some("datasets/features/multi_symbol_sma_2.manifest.json")
    );
    assert_eq!(gate.feature_name, "sma_close_2");
    assert_eq!(gate.version.as_deref(), Some("v1"));
}

#[test]
fn loads_binance_parquet_paper_config_from_file() {
    let config =
        AppConfig::from_toml_file("../../configs/paper/binance_btcusdt_1m_parquet.toml").unwrap();

    assert_eq!(config.runtime.mode, RuntimeMode::Paper);
    assert_eq!(config.runtime.run_id, "binance-btcusdt-1m-paper");
    assert_eq!(config.data.source, "parquet");
    assert_eq!(config.data.path, "datasets/binance/btcusdt_1m.parquet");
    assert_eq!(config.broker.kind, config::BrokerKind::Binance);
    assert_eq!(config.broker.mode, config::BrokerMode::Paper);
    assert!(!config.broker.order_submit_enabled);
}

#[test]
fn loads_ibkr_stock_parquet_paper_config_from_file() {
    let config =
        AppConfig::from_toml_file("../../configs/paper/ibkr_aapl_1d_parquet.toml").unwrap();

    assert_eq!(config.runtime.mode, RuntimeMode::Paper);
    assert_eq!(config.runtime.run_id, "ibkr-aapl-1d-paper");
    assert_eq!(config.data.source, "parquet");
    assert_eq!(config.data.path, "datasets/ibkr/aapl_1d.parquet");
    assert_eq!(config.broker.kind, config::BrokerKind::InteractiveBrokers);
    assert_eq!(config.broker.mode, config::BrokerMode::Paper);
    assert_eq!(config.broker.host.as_deref(), Some("127.0.0.1"));
    assert_eq!(config.broker.port, Some(7497));
    assert_eq!(config.broker.client_id, Some(1));
    assert!(!config.broker.order_submit_enabled);
    assert_eq!(config.risk.daily_loss_limit, None);
    assert_eq!(config.risk.max_order_attempts_per_day, None);
    assert_eq!(config.risk.max_order_failures_per_day, None);
    assert_eq!(config.risk.max_price_deviation_bps, None);
    assert_eq!(config.risk.max_market_data_age_ms, None);
    assert_eq!(config.risk.max_consecutive_strategy_losses, None);
    assert_eq!(config.risk.max_consecutive_strategy_errors, None);
    assert!(config.risk.trading_session.is_none());
}

#[test]
fn loads_paper_config_with_live_risk_hardening_fields() {
    let config = AppConfig::from_toml_str(
        r#"
        [runtime]
        mode = "paper"
        run_id = "risk-hardening"

        [database]
        url = "sqlite::memory:"

        [data]
        source = "csv"
        path = "datasets/sample/aapl_1d.csv"

        [strategy]
        name = "moving_average_cross"
        symbols = ["US:NASDAQ:AAPL:EQUITY"]
        fast_window = 2
        slow_window = 3

        [portfolio]
        initial_cash = "10000"
        base_currency = "USD"
        order_qty = "1"
        max_abs_qty = "10"

        [risk]
        max_order_notional = "1000"
        min_cash_after_order = "100"
        max_exposure = "5000"
        max_drawdown = "0.2"
        max_leverage = "2"
        max_margin_used = "0"
        trading_halted = false
        daily_loss_limit = "50"
        max_order_attempts_per_day = 20
        max_order_failures_per_day = 5
        max_price_deviation_bps = "50"
        max_market_data_age_ms = 5000
        max_consecutive_strategy_losses = 3
        max_consecutive_strategy_errors = 2

        [risk.trading_session]
        mode = "regular_only"
        timezone = "America/New_York"
        start = "09:30"
        end = "16:00"

        [broker]
        kind = "simulated"
        mode = "paper"

        [paper]
        account_id = "paper"
        slippage_bps = "0"
        fee_bps = "0"

        [live]
        enabled = false
        "#,
    )
    .unwrap();

    assert_eq!(config.risk.daily_loss_limit.as_deref(), Some("50"));
    assert_eq!(config.risk.max_order_attempts_per_day, Some(20));
    assert_eq!(config.risk.max_order_failures_per_day, Some(5));
    assert_eq!(config.risk.max_price_deviation_bps.as_deref(), Some("50"));
    assert_eq!(config.risk.max_market_data_age_ms, Some(5000));
    assert_eq!(config.risk.max_consecutive_strategy_losses, Some(3));
    assert_eq!(config.risk.max_consecutive_strategy_errors, Some(2));
    assert_eq!(
        config.risk.trading_session.as_ref().unwrap().timezone,
        "America/New_York"
    );
}
