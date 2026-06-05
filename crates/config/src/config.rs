#![forbid(unsafe_code)]

use std::path::Path;

use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config file {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("failed to parse config: {0}")]
    Parse(#[from] toml::de::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeMode {
    Backtest,
    Replay,
    Paper,
    Live,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrokerKind {
    Simulated,
    Futu,
    Binance,
    Okx,
    #[serde(alias = "ibkr")]
    InteractiveBrokers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrokerMode {
    Paper,
    Live,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub runtime: RuntimeConfig,
    pub database: DatabaseConfig,
    pub data: DataConfig,
    pub strategy: StrategyConfig,
    pub portfolio: PortfolioConfig,
    pub risk: RiskConfig,
    pub broker: BrokerConfig,
    pub paper: PaperConfig,
    pub live: LiveConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeConfig {
    pub mode: RuntimeMode,
    pub run_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DataConfig {
    pub source: String,
    pub path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StrategyConfig {
    pub name: String,
    pub symbols: Vec<String>,
    pub fast_window: usize,
    pub slow_window: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PortfolioConfig {
    pub initial_cash: String,
    pub base_currency: String,
    pub order_qty: String,
    pub max_abs_qty: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RiskConfig {
    pub max_order_notional: String,
    pub min_cash_after_order: String,
    pub max_exposure: String,
    pub max_drawdown: String,
    pub max_leverage: String,
    pub max_margin_used: String,
    pub trading_halted: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BrokerConfig {
    pub kind: BrokerKind,
    pub mode: BrokerMode,
    pub base_url: Option<String>,
    pub api_key_env: Option<String>,
    pub secret_key_env: Option<String>,
    pub recv_window_ms: Option<u64>,
    #[serde(default)]
    pub order_submit_enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PaperConfig {
    pub account_id: String,
    pub slippage_bps: String,
    pub fee_bps: String,
    pub bar_delay_ms: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LiveConfig {
    pub enabled: bool,
    pub heartbeat_ms: Option<u64>,
}

impl AppConfig {
    pub fn from_toml_file(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path_ref = path.as_ref();
        let input = std::fs::read_to_string(path_ref).map_err(|source| ConfigError::Read {
            path: path_ref.display().to_string(),
            source,
        })?;
        Self::from_toml_str(&input)
    }

    pub fn from_toml_str(input: &str) -> Result<Self, ConfigError> {
        Ok(toml::from_str(input)?)
    }
}
