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
}
