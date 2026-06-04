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
    "#;

    let config = AppConfig::from_toml_str(input).unwrap();

    assert_eq!(config.runtime.mode, RuntimeMode::Backtest);
    assert_eq!(config.runtime.run_id, "sample-ma-cross");
    assert_eq!(config.database.url, "sqlite://data/trader.sqlite");
    assert_eq!(config.strategy.name, "moving_average_cross");
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
    assert_eq!(config.broker.kind, config::BrokerKind::Simulated);
    assert_eq!(config.broker.mode, config::BrokerMode::Paper);
    assert!(!config.live.enabled);
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
    assert_eq!(config.broker.kind, config::BrokerKind::Futu);
    assert_eq!(config.broker.mode, config::BrokerMode::Paper);
    assert!(!config.live.enabled);
    assert_eq!(config.live.heartbeat_ms, Some(500));
}
