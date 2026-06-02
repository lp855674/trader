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

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub runtime: RuntimeConfig,
    pub database: DatabaseConfig,
    pub data: DataConfig,
    pub strategy: StrategyConfig,
    pub portfolio: PortfolioConfig,
    pub paper: PaperConfig,
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
pub struct PaperConfig {
    pub account_id: String,
    pub slippage_bps: String,
    pub fee_bps: String,
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
