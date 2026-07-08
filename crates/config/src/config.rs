#![forbid(unsafe_code)]

use std::{collections::BTreeSet, path::Path};

use serde::{Deserialize, Serialize};
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeMode {
    Backtest,
    Replay,
    Paper,
    Live,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrokerKind {
    Simulated,
    Futu,
    Binance,
    Okx,
    #[serde(alias = "ibkr")]
    InteractiveBrokers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrokerMode {
    Paper,
    Live,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    /// Run launch template. Server startup must not treat this as deployment identity.
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
    #[serde(default)]
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    /// Deployment-level config for the control plane process.
    pub database: DatabaseConfig,
    #[serde(default)]
    pub server: ServerSettings,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub run_defaults: ServerRunDefaults,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerSettings {
    #[serde(default = "default_server_bind")]
    pub bind: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ServerRunDefaults {
    pub config_path: Option<String>,
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
    /// Strategy template selected per run and copied into RunSpec.
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
    #[serde(default)]
    pub daily_loss_limit: Option<String>,
    #[serde(default)]
    pub max_order_attempts_per_day: Option<u32>,
    #[serde(default)]
    pub max_order_failures_per_day: Option<u32>,
    #[serde(default)]
    pub max_price_deviation_bps: Option<String>,
    #[serde(default)]
    pub max_market_data_age_ms: Option<u64>,
    #[serde(default)]
    pub max_consecutive_strategy_losses: Option<u32>,
    #[serde(default)]
    pub max_consecutive_strategy_errors: Option<u32>,
    #[serde(default)]
    pub trading_session: Option<TradingSessionConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TradingSessionConfig {
    pub mode: String,
    pub timezone: String,
    pub start: String,
    pub end: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BrokerConfig {
    pub kind: BrokerKind,
    pub mode: BrokerMode,
    pub base_url: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub client_id: Option<u32>,
    pub connect_timeout_ms: Option<u64>,
    pub api_key_env: Option<String>,
    pub secret_key_env: Option<String>,
    pub recv_window_ms: Option<u64>,
    #[serde(default)]
    pub order_submit_enabled: bool,
    #[serde(default)]
    pub fake_startup_unmatched_open_order: bool,
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
    pub reconciliation_gate: LiveReconciliationGateConfig,
    #[serde(default)]
    pub startup_recovery: LiveStartupRecoveryConfig,
    #[serde(default)]
    pub alerts: LiveAlertsConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LiveReconciliationGateConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_reconciliation_gate_min_successful_audits")]
    pub min_successful_audits: usize,
    #[serde(default = "default_reconciliation_gate_max_audit_age_ms")]
    pub max_audit_age_ms: i64,
    #[serde(default)]
    pub required_accounts: Vec<String>,
    #[serde(default)]
    pub missing_required_accounts: LiveReconciliationGateFailurePolicy,
    #[serde(default)]
    pub missing_required_audit: LiveReconciliationGateFailurePolicy,
    #[serde(default)]
    pub insufficient_clean_recent_audits: LiveReconciliationGateFailurePolicy,
    #[serde(default)]
    pub audit_too_old: LiveReconciliationGateFailurePolicy,
    #[serde(default)]
    pub audit_has_drift: LiveReconciliationGateFailurePolicy,
    #[serde(default)]
    pub audit_has_stale_inputs: LiveReconciliationGateFailurePolicy,
    #[serde(default)]
    pub log_write_failure: LiveReconciliationGateFailurePolicy,
}

impl Default for LiveReconciliationGateConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            min_successful_audits: default_reconciliation_gate_min_successful_audits(),
            max_audit_age_ms: default_reconciliation_gate_max_audit_age_ms(),
            required_accounts: Vec::new(),
            missing_required_accounts: LiveReconciliationGateFailurePolicy::default(),
            missing_required_audit: LiveReconciliationGateFailurePolicy::default(),
            insufficient_clean_recent_audits: LiveReconciliationGateFailurePolicy::default(),
            audit_too_old: LiveReconciliationGateFailurePolicy::default(),
            audit_has_drift: LiveReconciliationGateFailurePolicy::default(),
            audit_has_stale_inputs: LiveReconciliationGateFailurePolicy::default(),
            log_write_failure: LiveReconciliationGateFailurePolicy::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveReconciliationGateFailurePolicy {
    #[default]
    Block,
    WarnOnly,
}

fn default_reconciliation_gate_min_successful_audits() -> usize {
    1
}

fn default_reconciliation_gate_max_audit_age_ms() -> i64 {
    300_000
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct LiveStartupRecoveryConfig {
    #[serde(default)]
    pub unmatched_open_orders: LiveStartupRecoveryUnmatchedOpenOrders,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveStartupRecoveryUnmatchedOpenOrders {
    #[default]
    Fail,
    WarnOnly,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct LiveAlertsConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub sinks: Vec<LiveAlertSinkConfig>,
    pub sink: Option<String>,
    pub file_path: Option<String>,
    pub webhook_url: Option<String>,
    pub cooldown_ms: Option<u64>,
    pub webhook_timeout_ms: Option<u64>,
    pub webhook_max_retries: Option<u32>,
    pub webhook_auth_token: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct LiveAlertSinkConfig {
    pub sink: String,
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

#[derive(Debug, Clone, Deserialize)]
pub struct LoggingConfig {
    #[serde(default = "default_logging_enabled")]
    pub enabled: bool,
    #[serde(default = "default_logging_level")]
    pub level: String,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default = "default_logging_buffer_size")]
    pub buffer_size: usize,
    #[serde(default = "default_logging_flush_interval_ms")]
    pub flush_interval_ms: u64,
    #[serde(default = "default_logging_retention_days")]
    pub retention_days: u32,
    #[serde(default = "default_logging_console_output")]
    pub console_output: bool,
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

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            enabled: default_logging_enabled(),
            level: default_logging_level(),
            categories: Vec::new(),
            buffer_size: default_logging_buffer_size(),
            flush_interval_ms: default_logging_flush_interval_ms(),
            retention_days: default_logging_retention_days(),
            console_output: default_logging_console_output(),
        }
    }
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            bind: default_server_bind(),
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            database: DatabaseConfig {
                url: "sqlite::memory:".to_string(),
            },
            server: ServerSettings::default(),
            logging: LoggingConfig::default(),
            run_defaults: ServerRunDefaults::default(),
        }
    }
}

fn default_ingestion_fetch_interval_minutes() -> u64 {
    60
}

fn default_server_bind() -> String {
    "127.0.0.1:8080".to_string()
}

fn default_logging_enabled() -> bool {
    true
}

fn default_logging_level() -> String {
    "info".to_string()
}

fn default_logging_buffer_size() -> usize {
    1000
}

fn default_logging_flush_interval_ms() -> u64 {
    5000
}

fn default_logging_retention_days() -> u32 {
    30
}

fn default_logging_console_output() -> bool {
    true
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

impl ServerConfig {
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

    pub fn with_default_run_config_path(config_path: String) -> Self {
        Self {
            run_defaults: ServerRunDefaults {
                config_path: Some(config_path),
            },
            ..Self::default()
        }
    }

    pub fn default_run_config_path(&self) -> Option<&str> {
        self.run_defaults.config_path.as_deref()
    }
}

impl From<String> for ServerConfig {
    fn from(config_path: String) -> Self {
        Self::with_default_run_config_path(config_path)
    }
}

impl From<&str> for ServerConfig {
    fn from(config_path: &str) -> Self {
        Self::with_default_run_config_path(config_path.to_string())
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
