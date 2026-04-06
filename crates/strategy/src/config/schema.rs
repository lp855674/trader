// Configuration schema — JSON/env parsing, validation, merging.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ─── ConfigError ─────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Env error: {0}")]
    EnvError(String),
}

// ─── StrategyConfig ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyConfig {
    pub id: String,
    pub strategy_type: String,
    pub params: HashMap<String, serde_json::Value>,
    pub enabled: bool,
    pub version: u32,
}

// ─── BacktestConfigSchema ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestConfigSchema {
    pub start_date: String,
    pub end_date: String,
    pub initial_capital: f64,
    pub commission_rate: f64,
    pub instruments: Vec<String>,
}

// ─── RiskConfig ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskConfig {
    pub max_drawdown_pct: f64,
    pub max_position_size: f64,
    pub daily_loss_limit: f64,
    pub var_confidence: f64,
}

// ─── PaperTradingConfig ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperTradingConfig {
    pub enabled: bool,
    pub initial_capital: f64,
    pub commission_rate: f64,
    pub slippage_bps: f64,
}

// ─── AppConfig ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub strategies: Vec<StrategyConfig>,
    pub backtest: Option<BacktestConfigSchema>,
    pub risk: RiskConfig,
    pub paper_trading: PaperTradingConfig,
    pub log_level: String,
    pub data_dir: String,
}

// ─── ConfigLoader ─────────────────────────────────────────────────────────────

pub struct ConfigLoader;

impl ConfigLoader {
    /// Parse AppConfig from a JSON string.
    pub fn from_json(json: &str) -> Result<AppConfig, ConfigError> {
        serde_json::from_str(json).map_err(|e| ConfigError::ParseError(e.to_string()))
    }

    /// Apply environment variable overrides to the base config.
    pub fn from_env_overrides(mut base: AppConfig) -> AppConfig {
        if let Ok(v) = std::env::var("LOG_LEVEL") {
            base.log_level = v;
        }
        if let Ok(v) = std::env::var("DATA_DIR") {
            base.data_dir = v;
        }
        if let Ok(v) = std::env::var("INITIAL_CAPITAL") {
            if let Ok(n) = v.parse::<f64>() {
                base.paper_trading.initial_capital = n;
            }
        }
        if let Ok(v) = std::env::var("COMMISSION_RATE") {
            if let Ok(n) = v.parse::<f64>() {
                base.paper_trading.commission_rate = n;
            }
        }
        base
    }

    /// Validate an AppConfig.  Returns Err if any invariant is violated.
    pub fn validate(config: &AppConfig) -> Result<(), ConfigError> {
        if config.paper_trading.initial_capital <= 0.0 {
            return Err(ConfigError::ValidationError(
                "initial_capital must be > 0".into(),
            ));
        }
        let cr = config.paper_trading.commission_rate;
        if !(0.0..=0.1).contains(&cr) {
            return Err(ConfigError::ValidationError(format!(
                "commission_rate {cr} not in [0, 0.1]"
            )));
        }
        let md = config.risk.max_drawdown_pct;
        if !(0.0 < md && md <= 1.0) {
            return Err(ConfigError::ValidationError(format!(
                "max_drawdown_pct {md} not in (0, 1]"
            )));
        }
        let vc = config.risk.var_confidence;
        if !(0.9 < vc && vc < 1.0) {
            return Err(ConfigError::ValidationError(format!(
                "var_confidence {vc} not in (0.9, 1.0)"
            )));
        }
        for s in &config.strategies {
            if s.id.is_empty() {
                return Err(ConfigError::ValidationError(
                    "strategy id must be non-empty".into(),
                ));
            }
        }
        Ok(())
    }

    /// Merge an overlay (partial JSON) onto the base config.  Non-null overlay
    /// fields overwrite base fields.
    pub fn merge(
        base: AppConfig,
        overlay: serde_json::Value,
    ) -> Result<AppConfig, ConfigError> {
        let mut base_val =
            serde_json::to_value(&base).map_err(|e| ConfigError::ParseError(e.to_string()))?;
        Self::merge_json(&mut base_val, overlay);
        serde_json::from_value(base_val).map_err(|e| ConfigError::ParseError(e.to_string()))
    }

    fn merge_json(base: &mut serde_json::Value, overlay: serde_json::Value) {
        match (base, overlay) {
            (serde_json::Value::Object(b), serde_json::Value::Object(o)) => {
                for (k, v) in o {
                    if !v.is_null() {
                        let entry = b.entry(k).or_insert(serde_json::Value::Null);
                        Self::merge_json(entry, v);
                    }
                }
            }
            (base, overlay) => {
                if !overlay.is_null() {
                    *base = overlay;
                }
            }
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_json() -> &'static str {
        r#"{
            "strategies": [
                {
                    "id": "s1",
                    "strategy_type": "always_long",
                    "params": {},
                    "enabled": true,
                    "version": 1
                }
            ],
            "backtest": null,
            "risk": {
                "max_drawdown_pct": 0.2,
                "max_position_size": 1000.0,
                "daily_loss_limit": 500.0,
                "var_confidence": 0.95
            },
            "paper_trading": {
                "enabled": true,
                "initial_capital": 10000.0,
                "commission_rate": 0.001,
                "slippage_bps": 5.0
            },
            "log_level": "info",
            "data_dir": "/tmp/data"
        }"#
    }

    #[test]
    fn from_json_parses() {
        let cfg = ConfigLoader::from_json(valid_json()).unwrap();
        assert_eq!(cfg.strategies.len(), 1);
        assert_eq!(cfg.strategies[0].id, "s1");
        assert_eq!(cfg.log_level, "info");
    }

    #[test]
    fn validate_accepts_valid_config() {
        let cfg = ConfigLoader::from_json(valid_json()).unwrap();
        assert!(ConfigLoader::validate(&cfg).is_ok());
    }

    #[test]
    fn validate_rejects_zero_capital() {
        let mut cfg = ConfigLoader::from_json(valid_json()).unwrap();
        cfg.paper_trading.initial_capital = 0.0;
        let err = ConfigLoader::validate(&cfg);
        assert!(matches!(err, Err(ConfigError::ValidationError(_))));
    }

    #[test]
    fn validate_rejects_bad_commission() {
        let mut cfg = ConfigLoader::from_json(valid_json()).unwrap();
        cfg.paper_trading.commission_rate = 0.5; // > 0.1
        let err = ConfigLoader::validate(&cfg);
        assert!(matches!(err, Err(ConfigError::ValidationError(_))));
    }

    #[test]
    fn validate_rejects_bad_drawdown() {
        let mut cfg = ConfigLoader::from_json(valid_json()).unwrap();
        cfg.risk.max_drawdown_pct = 0.0;
        let err = ConfigLoader::validate(&cfg);
        assert!(matches!(err, Err(ConfigError::ValidationError(_))));
    }

    #[test]
    fn validate_rejects_bad_var_confidence() {
        let mut cfg = ConfigLoader::from_json(valid_json()).unwrap();
        cfg.risk.var_confidence = 0.5;
        let err = ConfigLoader::validate(&cfg);
        assert!(matches!(err, Err(ConfigError::ValidationError(_))));
    }

    #[test]
    fn validate_rejects_empty_strategy_id() {
        let mut cfg = ConfigLoader::from_json(valid_json()).unwrap();
        cfg.strategies[0].id = "".into();
        let err = ConfigLoader::validate(&cfg);
        assert!(matches!(err, Err(ConfigError::ValidationError(_))));
    }

    #[test]
    fn merge_overlays_field() {
        let base = ConfigLoader::from_json(valid_json()).unwrap();
        let overlay = serde_json::json!({ "log_level": "debug" });
        let merged = ConfigLoader::merge(base, overlay).unwrap();
        assert_eq!(merged.log_level, "debug");
    }

    #[test]
    fn merge_null_not_overwritten() {
        let base = ConfigLoader::from_json(valid_json()).unwrap();
        let original_level = base.log_level.clone();
        let overlay = serde_json::json!({ "log_level": null });
        let merged = ConfigLoader::merge(base, overlay).unwrap();
        assert_eq!(merged.log_level, original_level);
    }

    #[test]
    fn env_override_log_level() {
        // Ensure LOG_LEVEL is unset at start of test, verify override works
        // by checking that after removal, the value reverts.
        let cfg = ConfigLoader::from_json(valid_json()).unwrap();
        // When no LOG_LEVEL env var is set, log_level is not changed.
        // This test just verifies the function doesn't panic.
        // The env_override_applied test covers actual override behavior.
        let _ = ConfigLoader::from_env_overrides(cfg);
    }

    #[test]
    fn env_override_applied() {
        let cfg = ConfigLoader::from_json(valid_json()).unwrap();
        // SAFETY: single-threaded test
        unsafe {
            std::env::set_var("LOG_LEVEL", "warn");
            std::env::set_var("INITIAL_CAPITAL", "50000");
        }
        let cfg2 = ConfigLoader::from_env_overrides(cfg);
        unsafe {
            std::env::remove_var("LOG_LEVEL");
            std::env::remove_var("INITIAL_CAPITAL");
        }
        assert_eq!(cfg2.log_level, "warn");
        assert!((cfg2.paper_trading.initial_capital - 50_000.0).abs() < 1e-9);
    }
}
