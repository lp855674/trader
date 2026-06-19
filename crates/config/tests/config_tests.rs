use config::{AppConfig, RuntimeMode};

#[test]
fn parses_backtest_config() {
    let input = r#"
        [runtime]
        mode = "backtest"
        run_id = "sample-ma-cross"

        [database]
        url = "sqlite://data/trader.sqlite"

        [data]
        source = "csv"
        path = "datasets/sample/aapl_1d.csv"

        [strategy]
        name = "moving_average_cross"
        symbols = ["US:NASDAQ:AAPL:EQUITY"]
        fast_window = 20
        slow_window = 60

        [portfolio]
        initial_cash = "100000"
        base_currency = "USD"
        order_qty = "1"
        max_abs_qty = "100"

        [risk]
        max_order_notional = "1000000"
        min_cash_after_order = "0"
        max_exposure = "1000000"
        max_drawdown = "1"
        max_leverage = "10"
        max_margin_used = "0"
        trading_halted = false

        [broker]
        kind = "simulated"
        mode = "paper"

        [paper]
        account_id = "paper"
        slippage_bps = "25"
        fee_bps = "10"

        [live]
        enabled = false
        broker_snapshot_interval_ms = 250
    "#;

    let config = AppConfig::from_toml_str(input).unwrap();

    assert_eq!(config.runtime.mode, RuntimeMode::Backtest);
    assert_eq!(config.runtime.run_id, "sample-ma-cross");
    assert_eq!(config.database.url, "sqlite://data/trader.sqlite");
    assert_eq!(config.strategy.name, "moving_average_cross");
    assert_eq!(config.strategy.universe, "static");
    assert_eq!(config.strategy.alpha, "moving_average_cross");
    assert_eq!(config.data.path, "datasets/sample/aapl_1d.csv");
    assert_eq!(config.portfolio.base_currency, "USD");
    assert_eq!(config.paper.account_id, "paper");
    assert_eq!(config.paper.slippage_bps, "25");
    assert_eq!(config.paper.fee_bps, "10");
    assert_eq!(config.risk.max_order_notional, "1000000");
    assert_eq!(config.risk.min_cash_after_order, "0");
    assert_eq!(config.risk.max_exposure, "1000000");
    assert_eq!(config.risk.max_drawdown, "1");
    assert_eq!(config.risk.max_leverage, "10");
    assert_eq!(config.risk.max_margin_used, "0");
    assert!(!config.risk.trading_halted);
    assert_eq!(config.risk.allow_short, None);
    assert!(!config.effective_allow_short());
    assert_eq!(config.broker.kind, config::BrokerKind::Simulated);
    assert_eq!(config.broker.mode, config::BrokerMode::Paper);
    assert!(!config.broker.order_submit_enabled);
    assert!(!config.live.enabled);
    assert_eq!(config.live.broker_snapshot_interval_ms, Some(250));
    assert!(!config.ingestion.enabled);
    assert_eq!(config.ingestion.fetch_interval_minutes, 60);
}

#[test]
fn parses_ingestion_config() {
    let config = AppConfig::from_toml_str(
        r#"
        [runtime]
        mode = "paper"
        run_id = "ingestion-config"

        [database]
        url = "sqlite::memory:"

        [data]
        source = "csv"
        path = "datasets/sample/aapl_1d.csv"

        [strategy]
        name = "moving_average_cross"
        symbols = ["CRYPTO:BINANCE:BTCUSDT:CRYPTO_SPOT"]
        fast_window = 2
        slow_window = 3

        [portfolio]
        initial_cash = "100000"
        base_currency = "USDT"
        order_qty = "0.001"
        max_abs_qty = "1"

        [risk]
        max_order_notional = "50"
        min_cash_after_order = "10"
        max_exposure = "1000"
        max_drawdown = "0.2"
        max_leverage = "1"
        max_margin_used = "0"
        trading_halted = false

        [broker]
        kind = "binance"
        mode = "paper"

        [paper]
        account_id = "paper"
        slippage_bps = "5"
        fee_bps = "10"

        [live]
        enabled = false

        [ingestion]
        enabled = true
        sources = ["binance", "yahoo"]
        fetch_interval_minutes = 30
        symbols = ["BTCUSDT", "AAPL"]
        "#,
    )
    .unwrap();

    assert!(config.ingestion.enabled);
    assert_eq!(config.ingestion.sources, vec!["binance", "yahoo"]);
    assert_eq!(config.ingestion.fetch_interval_minutes, 30);
    assert_eq!(config.ingestion.symbols, vec!["BTCUSDT", "AAPL"]);
}

#[test]
fn parses_named_universe_and_alpha_strategy_assembly() {
    let config = AppConfig::from_toml_str(
        r#"
        [runtime]
        mode = "backtest"
        run_id = "named-assembly"

        [database]
        url = "sqlite://data/trader.sqlite"

        [data]
        source = "csv"
        path = "datasets/sample/aapl_1d.csv"

        [strategy]
        name = "moving_average_cross"
        universe = "static"
        alpha = "moving_average_cross"
        symbols = ["US:NASDAQ:AAPL:EQUITY", "US:NASDAQ:MSFT:EQUITY"]
        fast_window = 2
        slow_window = 3

        [portfolio]
        initial_cash = "100000"
        base_currency = "USD"
        order_qty = "1"
        max_abs_qty = "100"

        [risk]
        max_order_notional = "1000000"
        min_cash_after_order = "0"
        max_exposure = "1000000"
        max_drawdown = "1"
        max_leverage = "10"
        max_margin_used = "0"
        trading_halted = false

        [broker]
        kind = "simulated"
        mode = "paper"

        [paper]
        account_id = "paper"
        slippage_bps = "25"
        fee_bps = "10"

        [live]
        enabled = false
        "#,
    )
    .unwrap();

    assert_eq!(config.strategy.universe, "static");
    assert_eq!(config.strategy.alpha, "moving_average_cross");
    assert_eq!(
        config.strategy.symbols,
        vec!["US:NASDAQ:AAPL:EQUITY", "US:NASDAQ:MSFT:EQUITY"]
    );
}

#[test]
fn parses_filtered_universe_rules() {
    let config = AppConfig::from_toml_str(
        r#"
        [runtime]
        mode = "backtest"
        run_id = "filtered-universe"

        [database]
        url = "sqlite://data/trader.sqlite"

        [data]
        source = "csv"
        path = "datasets/sample/aapl_1d.csv"

        [strategy]
        name = "moving_average_cross"
        universe = "filtered"
        alpha = "moving_average_cross"
        symbols = [
            "US:NASDAQ:AAPL:EQUITY",
            "US:NASDAQ:MSFT:EQUITY",
            "US:NYSE:IBM:EQUITY"
        ]
        fast_window = 2
        slow_window = 3

        [strategy.universe_filter]
        exclude_symbols = ["US:NASDAQ:MSFT:EQUITY"]
        symbol_prefixes = ["US:NASDAQ:"]
        require_current_data = true

        [portfolio]
        initial_cash = "100000"
        base_currency = "USD"
        order_qty = "1"
        max_abs_qty = "100"

        [risk]
        max_order_notional = "1000000"
        min_cash_after_order = "0"
        max_exposure = "1000000"
        max_drawdown = "1"
        max_leverage = "10"
        max_margin_used = "0"
        trading_halted = false

        [broker]
        kind = "simulated"
        mode = "paper"

        [paper]
        account_id = "paper"
        slippage_bps = "25"
        fee_bps = "10"

        [live]
        enabled = false
        "#,
    )
    .unwrap();

    assert_eq!(config.strategy.universe, "filtered");
    assert_eq!(
        config.strategy.universe_filter.exclude_symbols,
        vec!["US:NASDAQ:MSFT:EQUITY"]
    );
    assert_eq!(
        config.strategy.universe_filter.symbol_prefixes,
        vec!["US:NASDAQ:"]
    );
    assert!(config.strategy.universe_filter.require_current_data);
}

#[test]
fn parses_ranked_universe_limit() {
    let config = AppConfig::from_toml_str(
        r#"
        [runtime]
        mode = "backtest"
        run_id = "ranked-universe"

        [database]
        url = "sqlite::memory:"

        [data]
        source = "csv"
        path = "datasets/sample/aapl_1d.csv"

        [strategy]
        name = "moving_average_cross"
        universe = "ranked"
        symbols = ["US:NASDAQ:AAPL:EQUITY", "US:NASDAQ:MSFT:EQUITY"]
        fast_window = 2
        slow_window = 3

        [strategy.universe_filter]
        max_symbols = 1
        require_current_data = true

        [portfolio]
        initial_cash = "100000"
        base_currency = "USD"
        order_qty = "1"
        max_abs_qty = "100"

        [risk]
        max_order_notional = "1000000"
        min_cash_after_order = "0"
        max_exposure = "1000000"
        max_drawdown = "1"
        max_leverage = "10"
        max_margin_used = "0"
        trading_halted = false

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

    assert_eq!(config.strategy.universe, "ranked");
    assert_eq!(config.strategy.universe_filter.max_symbols, Some(1));
    assert!(config.strategy.universe_filter.require_current_data);
}

#[test]
fn parses_feature_ranked_universe_config() {
    let config = AppConfig::from_toml_str(
        r#"
        [runtime]
        mode = "backtest"
        run_id = "feature-ranked-universe"

        [database]
        url = "sqlite::memory:"

        [data]
        source = "csv"
        path = "datasets/sample/aapl_1d.csv"

        [strategy]
        name = "moving_average_cross"
        universe = "feature_ranked"
        symbols = ["US:NASDAQ:AAPL:EQUITY", "US:NASDAQ:MSFT:EQUITY"]
        fast_window = 2
        slow_window = 3

        [strategy.universe_filter]
        max_symbols = 1
        require_current_data = true

        [strategy.universe_rank]
        source = "parquet"
        path = "datasets/features/universe_quality.parquet"
        manifest_path = "datasets/features/universe_quality.manifest.json"
        run_id = "research-2026-06-11"
        feature_name = "quality_score"
        version = "v1"
        build_indicator = "sma"
        build_period = 1
        build_value_column = "close"
        descending = true

        [portfolio]
        initial_cash = "100000"
        base_currency = "USD"
        order_qty = "1"
        max_abs_qty = "100"

        [risk]
        max_order_notional = "1000000"
        min_cash_after_order = "0"
        max_exposure = "1000000"
        max_drawdown = "1"
        max_leverage = "10"
        max_margin_used = "0"
        trading_halted = false

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

    let rank = config.strategy.universe_rank.unwrap();
    assert_eq!(config.strategy.universe, "feature_ranked");
    assert_eq!(config.strategy.universe_filter.max_symbols, Some(1));
    assert_eq!(rank.source, "parquet");
    assert_eq!(rank.path, "datasets/features/universe_quality.parquet");
    assert_eq!(
        rank.manifest_path.as_deref(),
        Some("datasets/features/universe_quality.manifest.json")
    );
    assert_eq!(rank.run_id, "research-2026-06-11");
    assert_eq!(rank.feature_name, "quality_score");
    assert_eq!(rank.version.as_deref(), Some("v1"));
    assert_eq!(rank.build_indicator.as_deref(), Some("sma"));
    assert_eq!(rank.build_period, Some(1));
    assert_eq!(rank.build_value_column.as_deref(), Some("close"));
    assert!(rank.descending);
}

#[test]
fn parses_weighted_alpha_components() {
    let config = AppConfig::from_toml_str(
        r#"
        [runtime]
        mode = "backtest"
        run_id = "weighted-alpha"

        [database]
        url = "sqlite://data/trader.sqlite"

        [data]
        source = "csv"
        path = "datasets/sample/aapl_1d.csv"

        [strategy]
        name = "moving_average_cross"
        alpha = "moving_average_cross"
        alpha_conflict_resolution = "highest_confidence"
        symbols = ["US:NASDAQ:AAPL:EQUITY"]
        fast_window = 2
        slow_window = 3

        [[strategy.alpha_components]]
        name = "moving_average_cross"
        fast_window = 2
        slow_window = 3
        weight = 0.25

        [[strategy.alpha_components]]
        name = "moving_average_cross"
        fast_window = 2
        slow_window = 3
        weight = 0.5

        [portfolio]
        initial_cash = "100000"
        base_currency = "USD"
        order_qty = "1"
        max_abs_qty = "100"

        [risk]
        max_order_notional = "1000000"
        min_cash_after_order = "0"
        max_exposure = "1000000"
        max_drawdown = "1"
        max_leverage = "10"
        max_margin_used = "0"
        trading_halted = false

        [broker]
        kind = "simulated"
        mode = "paper"

        [paper]
        account_id = "paper"
        slippage_bps = "25"
        fee_bps = "10"

        [live]
        enabled = false
        "#,
    )
    .unwrap();

    assert_eq!(
        config.strategy.alpha_conflict_resolution,
        "highest_confidence"
    );
    assert_eq!(config.strategy.alpha_components.len(), 2);
    assert_eq!(
        config.strategy.alpha_components[0].name,
        "moving_average_cross"
    );
    assert_eq!(config.strategy.alpha_components[0].fast_window, Some(2));
    assert_eq!(config.strategy.alpha_components[0].slow_window, Some(3));
    assert_eq!(config.strategy.alpha_components[0].weight, 0.25);
    assert_eq!(config.strategy.alpha_components[1].weight, 0.5);
}

#[test]
fn parses_alpha_feature_gate() {
    let config = AppConfig::from_toml_str(
        r#"
        [runtime]
        mode = "backtest"
        run_id = "feature-gated-alpha"

        [database]
        url = "sqlite://data/trader.sqlite"

        [data]
        source = "csv"
        path = "datasets/sample/aapl_1d.csv"

        [strategy]
        name = "moving_average_cross"
        alpha = "moving_average_cross"
        symbols = ["US:NASDAQ:AAPL:EQUITY"]
        fast_window = 2
        slow_window = 3

        [strategy.alpha_gate]
        source = "parquet"
        path = "datasets/features/quality.parquet"
        manifest_path = "datasets/features/quality.manifest.json"
        run_id = "research-2026-06-11"
        feature_name = "quality_score"
        version = "v2"
        build_indicator = "sma"
        build_period = 20
        build_value_column = "close"
        min_value = "0.7"
        max_value = "1.0"

        [portfolio]
        initial_cash = "100000"
        base_currency = "USD"
        order_qty = "1"
        max_abs_qty = "100"

        [risk]
        max_order_notional = "1000000"
        min_cash_after_order = "0"
        max_exposure = "1000000"
        max_drawdown = "1"
        max_leverage = "10"
        max_margin_used = "0"
        trading_halted = false

        [broker]
        kind = "simulated"
        mode = "paper"

        [paper]
        account_id = "paper"
        slippage_bps = "25"
        fee_bps = "10"

        [live]
        enabled = false
        "#,
    )
    .unwrap();

    let gate = config.strategy.alpha_gate.unwrap();
    assert_eq!(gate.source, "parquet");
    assert_eq!(gate.path, "datasets/features/quality.parquet");
    assert_eq!(
        gate.manifest_path.as_deref(),
        Some("datasets/features/quality.manifest.json")
    );
    assert_eq!(gate.run_id, "research-2026-06-11");
    assert_eq!(gate.feature_name, "quality_score");
    assert_eq!(gate.version.as_deref(), Some("v2"));
    assert_eq!(gate.build_indicator.as_deref(), Some("sma"));
    assert_eq!(gate.build_period, Some(20));
    assert_eq!(gate.build_value_column.as_deref(), Some("close"));
    assert_eq!(gate.min_value.as_deref(), Some("0.7"));
    assert_eq!(gate.max_value.as_deref(), Some("1.0"));
}

#[test]
fn parses_multi_symbol_data_inputs_without_legacy_single_file() {
    let config = AppConfig::from_toml_str(
        r#"
        [runtime]
        mode = "backtest"
        run_id = "multi-symbol-data"

        [database]
        url = "sqlite://data/trader.sqlite"

        [data]
        [[data.inputs]]
        symbol = "US:NASDAQ:AAPL:EQUITY"
        source = "csv"
        path = "datasets/sample/aapl_1d.csv"

        [[data.inputs]]
        symbol = "US:NASDAQ:MSFT:EQUITY"
        source = "parquet"
        path = "datasets/sample/msft_1d.parquet"

        [strategy]
        name = "moving_average_cross"
        symbols = ["US:NASDAQ:AAPL:EQUITY", "US:NASDAQ:MSFT:EQUITY"]
        fast_window = 2
        slow_window = 3

        [portfolio]
        initial_cash = "100000"
        base_currency = "USD"
        order_qty = "1"
        max_abs_qty = "100"

        [risk]
        max_order_notional = "1000000"
        min_cash_after_order = "0"
        max_exposure = "1000000"
        max_drawdown = "1"
        max_leverage = "10"
        max_margin_used = "0"
        trading_halted = false

        [broker]
        kind = "simulated"
        mode = "paper"

        [paper]
        account_id = "paper"
        slippage_bps = "25"
        fee_bps = "10"

        [live]
        enabled = false
        "#,
    )
    .unwrap();

    assert_eq!(config.data.source, "");
    assert_eq!(config.data.path, "");
    assert_eq!(config.data.inputs.len(), 2);
    assert_eq!(config.data.inputs[0].symbol, "US:NASDAQ:AAPL:EQUITY");
    assert_eq!(config.data.inputs[0].source, "csv");
    assert_eq!(config.data.inputs[1].symbol, "US:NASDAQ:MSFT:EQUITY");
    assert_eq!(config.data.inputs[1].source, "parquet");
}

#[test]
fn parses_optional_paper_bar_delay() {
    let config = AppConfig::from_toml_str(
        r#"
        [runtime]
        mode = "paper"
        run_id = "slow-paper"

        [database]
        url = "sqlite://data/trader.sqlite"

        [data]
        source = "csv"
        path = "datasets/sample/aapl_1d.csv"

        [strategy]
        name = "moving_average_cross"
        symbols = ["US:NASDAQ:AAPL:EQUITY"]
        fast_window = 2
        slow_window = 3

        [portfolio]
        initial_cash = "100000"
        base_currency = "USD"
        order_qty = "1"
        max_abs_qty = "100"

        [risk]
        max_order_notional = "1000000"
        min_cash_after_order = "0"
        max_exposure = "1000000"
        max_drawdown = "1"
        max_leverage = "10"
        max_margin_used = "0"
        trading_halted = false

        [broker]
        kind = "simulated"
        mode = "paper"

        [paper]
        account_id = "paper"
        slippage_bps = "25"
        fee_bps = "10"
        bar_delay_ms = 50

        [live]
        enabled = false
        "#,
    )
    .unwrap();

    assert_eq!(config.paper.bar_delay_ms, Some(50));
}

#[test]
fn parses_production_paper_controls() {
    let config = AppConfig::from_toml_str(
        r#"
        [runtime]
        mode = "paper"
        run_id = "paper-production-prep"

        [database]
        url = "sqlite://data/paper.sqlite"

        [data]
        source = "csv"
        path = "datasets/sample/aapl_1d.csv"

        [strategy]
        name = "moving_average_cross"
        symbols = ["US:NASDAQ:AAPL:EQUITY"]
        fast_window = 5
        slow_window = 20

        [portfolio]
        initial_cash = "250000"
        base_currency = "USD"
        order_qty = "10"
        max_abs_qty = "500"

        [risk]
        max_order_notional = "25000"
        min_cash_after_order = "10000"
        max_exposure = "150000"
        max_drawdown = "0.2"
        max_leverage = "2"
        max_margin_used = "0.5"
        trading_halted = true
        allow_short = true

        [broker]
        kind = "futu"
        mode = "paper"

        [paper]
        account_id = "paper-futu"
        slippage_bps = "5"
        fee_bps = "2"
        bar_delay_ms = 25

        [live]
        enabled = false
        heartbeat_ms = 500
        "#,
    )
    .unwrap();

    assert_eq!(config.risk.max_order_notional, "25000");
    assert_eq!(config.risk.min_cash_after_order, "10000");
    assert_eq!(config.risk.max_exposure, "150000");
    assert_eq!(config.risk.max_drawdown, "0.2");
    assert_eq!(config.risk.max_leverage, "2");
    assert_eq!(config.risk.max_margin_used, "0.5");
    assert!(config.risk.trading_halted);
    assert_eq!(config.risk.allow_short, Some(true));
    assert!(config.effective_allow_short());
    assert_eq!(config.broker.kind, config::BrokerKind::Futu);
    assert_eq!(config.broker.mode, config::BrokerMode::Paper);
    assert!(!config.live.enabled);
    assert_eq!(config.live.heartbeat_ms, Some(500));
}

#[test]
fn parses_binance_paper_connection_config_without_secrets() {
    let config = AppConfig::from_toml_str(
        r#"
        [runtime]
        mode = "paper"
        run_id = "binance-paper-readonly"

        [database]
        url = "sqlite://data/binance-paper.sqlite"

        [data]
        source = "csv"
        path = "datasets/sample/aapl_1d.csv"

        [strategy]
        name = "moving_average_cross"
        symbols = ["CRYPTO:BINANCE:BTCUSDT:CRYPTO_SPOT"]
        fast_window = 5
        slow_window = 20

        [portfolio]
        initial_cash = "100000"
        base_currency = "USDT"
        order_qty = "0.001"
        max_abs_qty = "1"

        [risk]
        max_order_notional = "50"
        min_cash_after_order = "10"
        max_exposure = "1000"
        max_drawdown = "0.2"
        max_leverage = "1"
        max_margin_used = "0"
        trading_halted = false

        [broker]
        kind = "binance"
        mode = "paper"
        base_url = "https://testnet.binance.vision/api"
        api_key_env = "BINANCE_TESTNET_API_KEY"
        secret_key_env = "BINANCE_TESTNET_SECRET_KEY"
        recv_window_ms = 5000
        order_submit_enabled = true

        [paper]
        account_id = "binance-testnet"
        slippage_bps = "5"
        fee_bps = "10"

        [live]
        enabled = false
        "#,
    )
    .unwrap();

    assert_eq!(config.broker.kind, config::BrokerKind::Binance);
    assert_eq!(config.broker.mode, config::BrokerMode::Paper);
    assert_eq!(
        config.broker.base_url.as_deref(),
        Some("https://testnet.binance.vision/api")
    );
    assert_eq!(
        config.broker.api_key_env.as_deref(),
        Some("BINANCE_TESTNET_API_KEY")
    );
    assert_eq!(
        config.broker.secret_key_env.as_deref(),
        Some("BINANCE_TESTNET_SECRET_KEY")
    );
    assert_eq!(config.broker.recv_window_ms, Some(5000));
    assert!(config.broker.order_submit_enabled);
}

#[test]
fn parses_ibkr_alias_as_interactive_brokers() {
    let config = AppConfig::from_toml_str(
        r#"
        [runtime]
        mode = "paper"
        run_id = "ibkr-aapl-paper"

        [database]
        url = "sqlite://data/ibkr-aapl-paper.sqlite"

        [data]
        source = "parquet"
        path = "datasets/ibkr/aapl_1d.parquet"

        [strategy]
        name = "moving_average_cross"
        symbols = ["US:NASDAQ:AAPL:EQUITY"]
        fast_window = 2
        slow_window = 3

        [portfolio]
        initial_cash = "100000"
        base_currency = "USD"
        order_qty = "1"
        max_abs_qty = "100"

        [risk]
        max_order_notional = "1000"
        min_cash_after_order = "1000"
        max_exposure = "10000"
        max_drawdown = "0.2"
        max_leverage = "1"
        max_margin_used = "0"
        trading_halted = false

        [broker]
        kind = "ibkr"
        mode = "paper"
        host = "127.0.0.1"
        port = 7497
        client_id = 1

        [paper]
        account_id = "ibkr-paper"
        slippage_bps = "5"
        fee_bps = "2"

        [live]
        enabled = false
        "#,
    )
    .unwrap();

    assert_eq!(config.broker.kind, config::BrokerKind::InteractiveBrokers);
    assert_eq!(config.broker.mode, config::BrokerMode::Paper);
    assert_eq!(config.broker.host.as_deref(), Some("127.0.0.1"));
    assert_eq!(config.broker.port, Some(7497));
    assert_eq!(config.broker.client_id, Some(1));
    assert_eq!(config.data.source, "parquet");
}

#[test]
fn derives_short_permission_from_crypto_derivative_symbols_when_unset() {
    let config =
        config_with_symbol_and_allow_short("CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP", None);

    assert_eq!(config.risk.allow_short, None);
    assert!(config.effective_allow_short());
    assert_eq!(
        config.shortable_symbols(),
        std::collections::BTreeSet::from(["CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP".to_string()])
    );
}

#[test]
fn keeps_equity_and_crypto_spot_short_permission_disabled_when_unset() {
    let equity_config = config_with_symbol_and_allow_short("US:NASDAQ:AAPL:EQUITY", None);
    let spot_config =
        config_with_symbol_and_allow_short("CRYPTO:BINANCE:BTCUSDT:CRYPTO_SPOT", None);

    assert!(!equity_config.effective_allow_short());
    assert!(!spot_config.effective_allow_short());
}

#[test]
fn explicit_short_permission_overrides_symbol_default() {
    let disabled_perp_config =
        config_with_symbol_and_allow_short("CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP", Some(false));
    let enabled_equity_config =
        config_with_symbol_and_allow_short("US:NASDAQ:AAPL:EQUITY", Some(true));

    assert!(!disabled_perp_config.effective_allow_short());
    assert!(enabled_equity_config.effective_allow_short());
}

#[test]
fn derives_short_permission_per_symbol_for_mixed_universe_when_unset() {
    let config = config_with_symbols_and_allow_short(
        &[
            "US:NASDAQ:AAPL:EQUITY",
            "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP",
            "CRYPTO:BINANCE:ETHUSDT:CRYPTO_SPOT",
        ],
        None,
    );

    assert!(!config.effective_allow_short());
    assert_eq!(
        config.shortable_symbols(),
        std::collections::BTreeSet::from(["CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP".to_string()])
    );
}

fn config_with_symbol_and_allow_short(symbol: &str, allow_short: Option<bool>) -> AppConfig {
    config_with_symbols_and_allow_short(&[symbol], allow_short)
}

fn config_with_symbols_and_allow_short(symbols: &[&str], allow_short: Option<bool>) -> AppConfig {
    let allow_short_line = allow_short
        .map(|value| format!("allow_short = {value}"))
        .unwrap_or_default();
    let symbols = symbols
        .iter()
        .map(|symbol| format!(r#""{symbol}""#))
        .collect::<Vec<_>>()
        .join(", ");
    AppConfig::from_toml_str(&format!(
        r#"
        [runtime]
        mode = "backtest"
        run_id = "short-permission"

        [database]
        url = "sqlite://data/trader.sqlite"

        [data]
        source = "csv"
        path = "datasets/sample/aapl_1d.csv"

        [strategy]
        name = "moving_average_cross"
        symbols = [{symbols}]
        fast_window = 2
        slow_window = 3

        [portfolio]
        initial_cash = "100000"
        base_currency = "USD"
        order_qty = "1"
        max_abs_qty = "100"

        [risk]
        max_order_notional = "1000000"
        min_cash_after_order = "0"
        max_exposure = "1000000"
        max_drawdown = "1"
        max_leverage = "10"
        max_margin_used = "0"
        trading_halted = false
        {allow_short_line}

        [broker]
        kind = "simulated"
        mode = "paper"

        [paper]
        account_id = "paper"
        slippage_bps = "25"
        fee_bps = "10"

        [live]
        enabled = false
        "#
    ))
    .unwrap()
}
