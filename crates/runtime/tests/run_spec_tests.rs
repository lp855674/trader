use config::{AppConfig, BrokerKind, BrokerMode, RuntimeMode};
use runtime::RunSpec;

#[test]
fn run_spec_preserves_core_fields_from_app_config() {
    let config = AppConfig::from_toml_str(
        r#"
        [runtime]
        mode = "paper"
        run_id = "run-spec-paper"

        [database]
        url = "sqlite://data/run-spec.sqlite"

        [data]
        source = "csv"
        path = "datasets/sample/aapl_1d.csv"

        [[data.inputs]]
        symbol = "US:NASDAQ:AAPL:EQUITY"
        source = "csv"
        path = "datasets/sample/aapl_1d.csv"

        [strategy]
        name = "moving_average_cross"
        universe = "static"
        alpha = "moving_average_cross"
        alpha_conflict_resolution = "highest_confidence"
        symbols = ["US:NASDAQ:AAPL:EQUITY"]
        fast_window = 5
        slow_window = 20

        [portfolio]
        initial_cash = "100000"
        base_currency = "USD"
        order_qty = "10"
        max_abs_qty = "500"

        [risk]
        max_order_notional = "25000"
        min_cash_after_order = "1000"
        max_exposure = "150000"
        max_drawdown = "0.2"
        max_leverage = "2"
        max_margin_used = "0.5"
        trading_halted = false
        allow_short = true

        [broker]
        kind = "binance"
        mode = "paper"
        base_url = "https://testnet.binance.vision/api"
        api_key_env = "BINANCE_TESTNET_API_KEY"
        secret_key_env = "BINANCE_TESTNET_SECRET_KEY"
        recv_window_ms = 5000
        order_submit_enabled = true

        [paper]
        account_id = "paper-binance"
        slippage_bps = "5"
        fee_bps = "10"
        bar_delay_ms = 25

        [live]
        enabled = false
        "#,
    )
    .unwrap();

    let spec = RunSpec::from(&config);

    assert_eq!(spec.run_id, "run-spec-paper");
    assert_eq!(spec.mode, RuntimeMode::Paper);
    assert_eq!(spec.strategy.name, "moving_average_cross");
    assert_eq!(spec.strategy.symbols, vec!["US:NASDAQ:AAPL:EQUITY"]);
    assert_eq!(spec.strategy.fast_window, 5);
    assert_eq!(spec.strategy.slow_window, 20);
    assert_eq!(spec.data.source, "csv");
    assert_eq!(spec.data.path, "datasets/sample/aapl_1d.csv");
    assert_eq!(spec.data.inputs.len(), 1);
    assert_eq!(spec.data.inputs[0].symbol, "US:NASDAQ:AAPL:EQUITY");
    assert_eq!(spec.portfolio.base_currency, "USD");
    assert_eq!(spec.portfolio.order_qty, "10");
    assert_eq!(spec.risk.max_order_notional, "25000");
    assert_eq!(spec.risk.allow_short, Some(true));
    assert_eq!(spec.broker.kind, BrokerKind::Binance);
    assert_eq!(spec.broker.mode, BrokerMode::Paper);
    assert_eq!(
        spec.broker.base_url.as_deref(),
        Some("https://testnet.binance.vision/api")
    );
    assert!(spec.broker.order_submit_enabled);
    assert_eq!(spec.paper.account_id, "paper-binance");
    assert_eq!(spec.paper.bar_delay_ms, Some(25));
    assert!(!spec.live_enabled);
}
