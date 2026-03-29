//! Application configuration from environment variables.

use std::net::SocketAddr;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub database_url: String,
    pub http_bind: SocketAddr,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("invalid QUANTD_HTTP_BIND: {0}")]
    InvalidBind(String),
}

impl AppConfig {
    /// Loads config. `QUANTD_DATABASE_URL` defaults to `sqlite:quantd.db`.
    /// `QUANTD_HTTP_BIND` defaults to `127.0.0.1:8080`.
    pub fn from_env() -> Result<Self, ConfigError> {
        let database_url = std::env::var("QUANTD_DATABASE_URL")
            .unwrap_or_else(|_| "sqlite:quantd.db".to_string());
        let bind_str =
            std::env::var("QUANTD_HTTP_BIND").unwrap_or_else(|_| "127.0.0.1:8080".to_string());
        let http_bind = bind_str
            .parse::<SocketAddr>()
            .map_err(|_| ConfigError::InvalidBind(bind_str))?;
        Ok(Self {
            database_url,
            http_bind,
        })
    }
}
