#![forbid(unsafe_code)]

use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
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

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub runtime: RuntimeConfig,
    pub data: DataConfig,
    pub strategy: StrategyConfig,
    pub portfolio: PortfolioConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeConfig {
    pub mode: RuntimeMode,
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
}

impl AppConfig {
    pub fn from_toml_str(input: &str) -> Result<Self, ConfigError> {
        Ok(toml::from_str(input)?)
    }
}
