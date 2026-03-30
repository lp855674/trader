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
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("invalid QUANTD_HTTP_BIND: {0}")]
    InvalidBind(String),
}

impl AppConfig {
    /// Loads config. `QUANTD_DATABASE_URL` defaults to `sqlite:quantd.db`.
    /// `QUANTD_HTTP_BIND` defaults to `127.0.0.1:8080`.
    /// Log filter: `RUST_LOG` if set, else `QUANTD_LOG`, else `info`.
    pub fn from_env() -> Result<Self, ConfigError> {
        let database_url = std::env::var("QUANTD_DATABASE_URL")
            .unwrap_or_else(|_| "sqlite:quantd.db".to_string());
        let bind_str =
            std::env::var("QUANTD_HTTP_BIND").unwrap_or_else(|_| "127.0.0.1:8080".to_string());
        let http_bind = bind_str
            .parse::<SocketAddr>()
            .map_err(|_| ConfigError::InvalidBind(bind_str))?;
        let log_filter = std::env::var("RUST_LOG")
            .or_else(|_| std::env::var("QUANTD_LOG"))
            .unwrap_or_else(|_| "info".to_string());
        let api_key = std::env::var("QUANTD_API_KEY").ok().filter(|v| !v.is_empty());
        let env = std::env::var("QUANTD_ENV")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "dev".to_string());
        let allow_seed = std::env::var("QUANTD_ALLOW_SEED")
            .ok()
            .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
            .unwrap_or(false);
        Ok(Self {
            database_url,
            http_bind,
            log_filter,
            api_key,
            env,
            allow_seed,
        })
    }
}
