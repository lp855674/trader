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
