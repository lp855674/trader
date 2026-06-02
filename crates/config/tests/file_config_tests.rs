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
}
