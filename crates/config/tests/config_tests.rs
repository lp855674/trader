use config::{AppConfig, RuntimeMode};

#[test]
fn parses_backtest_config() {
    let input = r#"
        [runtime]
        mode = "backtest"

        [data]
        source = "parquet"
        path = "datasets/sample/aapl_1d.csv"

        [strategy]
        name = "moving_average_cross"
        symbols = ["US:NASDAQ:AAPL:EQUITY"]
        fast_window = 20
        slow_window = 60

        [portfolio]
        initial_cash = "100000"
        base_currency = "USD"
    "#;

    let config = AppConfig::from_toml_str(input).unwrap();

    assert_eq!(config.runtime.mode, RuntimeMode::Backtest);
    assert_eq!(config.strategy.name, "moving_average_cross");
    assert_eq!(config.data.path, "datasets/sample/aapl_1d.csv");
    assert_eq!(config.portfolio.base_currency, "USD");
}
