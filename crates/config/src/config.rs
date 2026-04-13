//! Application configuration from environment variables.

use std::net::SocketAddr;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub database_url: String,
    pub http_bind: SocketAddr,
    /// `tracing` filter directive (e.g. `info`, `quantd=debug`).  
    /// From `RUST_LOG`, else `QUANTD_LOG`, else `info`.
    pub log_filter: String,
    /// Optional API key for HTTP/WS. If unset, auth is disabled.
    pub api_key: Option<String>,
    /// Deployment environment (`dev` | `paper` | `prod`).
    pub env: String,
    /// Allow writing MVP seed rows in `prod` (default: false).
    pub allow_seed: bool,
    /// Enable background universe-cycle loop (default: false).
    pub universe_loop_enabled: bool,
    /// Interval seconds for background universe-cycle loop.
    pub universe_loop_interval_secs: u64,
    /// Venue for background universe-cycle loop.
    pub universe_loop_venue: String,
    /// Account for background universe-cycle loop.
    pub universe_loop_account_id: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("invalid QUANTD_HTTP_BIND: {0}")]
    InvalidBind(String),
}

impl AppConfig {
    /// Loads config. `QUANTD_DATABASE_URL` defaults to `sqlite:quantd.db?mode=rwc` (create file if missing).
    /// `QUANTD_HTTP_BIND` defaults to `127.0.0.1:8080`.
    /// `QUANTD_STRATEGY`（仅 `quantd` 进程）：未设置或 `noop` 为不下单；`always_long_one` 为演示管线（有 bar 则 paper 买）。
    /// Log filter: `RUST_LOG` if set, else `QUANTD_LOG`, else `info`.
    pub fn from_env() -> Result<Self, ConfigError> {
        let database_url = std::env::var("QUANTD_DATABASE_URL")
            .unwrap_or_else(|_| "sqlite:quantd.db?mode=rwc".to_string());
        let bind_str =
            std::env::var("QUANTD_HTTP_BIND").unwrap_or_else(|_| "127.0.0.1:8080".to_string());
        let http_bind = bind_str
            .parse::<SocketAddr>()
            .map_err(|_| ConfigError::InvalidBind(bind_str))?;
        let log_filter = std::env::var("RUST_LOG")
            .or_else(|_| std::env::var("QUANTD_LOG"))
            .unwrap_or_else(|_| "info".to_string());
        let api_key = std::env::var("QUANTD_API_KEY")
            .ok()
            .filter(|v| !v.is_empty());
        let env = std::env::var("QUANTD_ENV")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "dev".to_string());
        let allow_seed = std::env::var("QUANTD_ALLOW_SEED")
            .ok()
            .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
            .unwrap_or(false);
        let universe_loop_enabled = std::env::var("QUANTD_UNIVERSE_LOOP_ENABLED")
            .ok()
            .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
            .unwrap_or(false);
        let universe_loop_interval_secs = std::env::var("QUANTD_UNIVERSE_LOOP_INTERVAL_SECS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(60);
        let universe_loop_venue = std::env::var("QUANTD_UNIVERSE_LOOP_VENUE")
            .ok()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "US_EQUITY".to_string());
        let universe_loop_account_id = std::env::var("QUANTD_UNIVERSE_LOOP_ACCOUNT_ID")
            .ok()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "acc_mvp_paper".to_string());
        Ok(Self {
            database_url,
            http_bind,
            log_filter,
            api_key,
            env,
            allow_seed,
            universe_loop_enabled,
            universe_loop_interval_secs,
            universe_loop_venue,
            universe_loop_account_id,
        })
    }
}
