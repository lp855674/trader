#![forbid(unsafe_code)]

use std::{collections::BTreeSet, path::Path};

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
    #[serde(default)]
    pub ingestion: IngestionConfig,
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
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub inputs: Vec<DataInputConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DataInputConfig {
    pub symbol: String,
    pub source: String,
    pub path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StrategyConfig {
    pub name: String,
    #[serde(default = "default_universe_name")]
    pub universe: String,
    #[serde(default = "default_alpha_name")]
    pub alpha: String,
    #[serde(default = "default_alpha_conflict_resolution")]
    pub alpha_conflict_resolution: String,
    #[serde(default)]
    pub alpha_components: Vec<StrategyAlphaComponentConfig>,
    pub symbols: Vec<String>,
    #[serde(default)]
    pub universe_filter: StrategyUniverseFilterConfig,
    pub universe_rank: Option<StrategyUniverseRankConfig>,
    pub alpha_gate: Option<StrategyAlphaGateConfig>,
    pub fast_window: usize,
    pub slow_window: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StrategyAlphaComponentConfig {
    pub name: String,
    #[serde(default)]
    pub category: Option<String>,
    pub fast_window: Option<usize>,
    pub slow_window: Option<usize>,
    #[serde(default = "default_alpha_component_weight")]
    pub weight: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StrategyAlphaGateConfig {
    pub source: String,
    pub path: String,
    pub manifest_path: Option<String>,
    pub run_id: String,
    pub feature_name: String,
    pub version: Option<String>,
    pub build_indicator: Option<String>,
    pub build_period: Option<usize>,
    pub build_value_column: Option<String>,
    pub min_value: Option<String>,
    pub max_value: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StrategyUniverseRankConfig {
    pub source: String,
    pub path: String,
    pub manifest_path: Option<String>,
    pub run_id: String,
    pub feature_name: String,
    pub version: Option<String>,
    pub build_indicator: Option<String>,
    pub build_period: Option<usize>,
    pub build_value_column: Option<String>,
    #[serde(default = "default_true")]
    pub descending: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct StrategyUniverseFilterConfig {
    #[serde(default)]
    pub include_symbols: Vec<String>,
    #[serde(default)]
    pub exclude_symbols: Vec<String>,
    #[serde(default)]
    pub symbol_prefixes: Vec<String>,
    #[serde(default)]
    pub require_current_data: bool,
    pub max_symbols: Option<usize>,
}

fn default_universe_name() -> String {
    "static".to_string()
}

fn default_alpha_name() -> String {
    "moving_average_cross".to_string()
}

fn default_alpha_conflict_resolution() -> String {
    "highest_confidence".to_string()
}

fn default_alpha_component_weight() -> f64 {
    1.0
}

fn default_true() -> bool {
    true
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
    #[serde(default)]
    pub allow_short: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BrokerConfig {
    pub kind: BrokerKind,
    pub mode: BrokerMode,
    pub base_url: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub client_id: Option<u32>,
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
    pub broker_snapshot_interval_ms: Option<u64>,
    #[serde(default)]
    pub alerts: LiveAlertsConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct LiveAlertsConfig {
    #[serde(default)]
    pub enabled: bool,
    pub sink: Option<String>,
    pub file_path: Option<String>,
    pub webhook_url: Option<String>,
    pub cooldown_ms: Option<u64>,
    pub webhook_timeout_ms: Option<u64>,
    pub webhook_max_retries: Option<u32>,
    pub webhook_auth_token: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IngestionConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default = "default_ingestion_fetch_interval_minutes")]
    pub fetch_interval_minutes: u64,
    #[serde(default)]
    pub symbols: Vec<String>,
}

impl Default for IngestionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            sources: Vec::new(),
            fetch_interval_minutes: default_ingestion_fetch_interval_minutes(),
            symbols: Vec::new(),
        }
    }
}

fn default_ingestion_fetch_interval_minutes() -> u64 {
    60
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

    pub fn effective_allow_short(&self) -> bool {
        self.risk.effective_allow_short(&self.strategy.symbols)
    }

    pub fn shortable_symbols(&self) -> BTreeSet<String> {
        self.risk.shortable_symbols(&self.strategy.symbols)
    }
}

impl RiskConfig {
    pub fn effective_allow_short(&self, symbols: &[String]) -> bool {
        self.allow_short
            .unwrap_or_else(|| symbols_default_allow_short(symbols))
    }

    pub fn shortable_symbols(&self, symbols: &[String]) -> BTreeSet<String> {
        match self.allow_short {
            Some(true) => symbols.iter().cloned().collect(),
            Some(false) => BTreeSet::new(),
            None => symbols
                .iter()
                .filter(|symbol| symbol_default_allow_short(symbol))
                .cloned()
                .collect(),
        }
    }
}

fn symbols_default_allow_short(symbols: &[String]) -> bool {
    !symbols.is_empty()
        && symbols
            .iter()
            .all(|symbol| symbol_default_allow_short(symbol))
}

fn symbol_default_allow_short(symbol: &str) -> bool {
    let mut parts = symbol.split(':');
    let market = parts.next();
    let _exchange = parts.next();
    let _code = parts.next();
    let asset_class = parts.next();
    if parts.next().is_some() {
        return false;
    }

    matches!(
        (market, asset_class),
        (Some("CRYPTO"), Some("CRYPTO_PERP")) | (Some("CRYPTO"), Some("CRYPTO_FUTURE"))
    )
}
