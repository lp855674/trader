// Risk system configuration schema

use serde::{Deserialize, Serialize};
use crate::risk::order::OrderRiskConfig;
use crate::risk::portfolio::PortfolioRiskConfig;
use crate::risk::position::PnLLimits;

// ── DataQualityConfig ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataQualityConfig {
    pub z_score_threshold: f64,
    pub max_stale_ms: u64,
    pub expected_interval_ms: u64,
}

impl Default for DataQualityConfig {
    fn default() -> Self {
        Self {
            z_score_threshold: 3.0,
            max_stale_ms: 30_000,
            expected_interval_ms: 1_000,
        }
    }
}

// ── RiskSystemConfig ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskSystemConfig {
    pub order_risk: OrderRiskConfig,
    pub portfolio: PortfolioRiskConfig,
    pub pnl_limits: PnLLimits,
    /// e.g. ["log", "webhook:https://example.com/hook"]
    pub alert_channels: Vec<String>,
    pub metrics_retention_ms: u64,
    pub data_quality: DataQualityConfig,
}

impl Default for RiskSystemConfig {
    fn default() -> Self {
        Self {
            order_risk: OrderRiskConfig::default(),
            portfolio: PortfolioRiskConfig::default(),
            pnl_limits: PnLLimits {
                daily_loss_limit: -10_000.0,
                position_loss_limit: -2_000.0,
                max_drawdown_pct: 0.15,
            },
            alert_channels: vec!["log".to_string()],
            metrics_retention_ms: 86_400_000,
            data_quality: DataQualityConfig::default(),
        }
    }
}

// ── RiskConfigLoader ──────────────────────────────────────────────────────────

pub struct RiskConfigLoader;

impl RiskConfigLoader {
    pub fn from_json(json: &str) -> Result<RiskSystemConfig, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn validate(config: &RiskSystemConfig) -> Result<(), String> {
        // var_confidence must be in (0.9, 1.0)
        let vc = config.portfolio.var_confidence;
        if vc <= 0.9 || vc >= 1.0 {
            return Err(format!(
                "var_confidence {:.4} must be in (0.9, 1.0)",
                vc
            ));
        }

        // All thresholds > 0
        if config.order_risk.max_quantity <= 0.0 {
            return Err("max_quantity must be > 0".into());
        }
        if config.order_risk.max_notional <= 0.0 {
            return Err("max_notional must be > 0".into());
        }
        if config.portfolio.max_var_pct <= 0.0 {
            return Err("max_var_pct must be > 0".into());
        }

        // max_drawdown_pct in (0, 1]
        let md = config.pnl_limits.max_drawdown_pct;
        if md <= 0.0 || md > 1.0 {
            return Err(format!(
                "max_drawdown_pct {:.4} must be in (0, 1]",
                md
            ));
        }

        // data quality thresholds
        if config.data_quality.z_score_threshold <= 0.0 {
            return Err("z_score_threshold must be > 0".into());
        }
        if config.data_quality.max_stale_ms == 0 {
            return Err("max_stale_ms must be > 0".into());
        }

        Ok(())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn default_json() -> String {
        serde_json::to_string(&RiskSystemConfig::default()).unwrap()
    }

    #[test]
    fn from_json_parses_default() {
        let json = default_json();
        let config = RiskConfigLoader::from_json(&json).expect("Should parse default config");
        assert!((config.portfolio.var_confidence - 0.95).abs() < 1e-9);
    }

    #[test]
    fn validate_accepts_default_config() {
        let config = RiskSystemConfig::default();
        assert!(RiskConfigLoader::validate(&config).is_ok());
    }

    #[test]
    fn validate_rejects_invalid_var_confidence() {
        let mut config = RiskSystemConfig::default();
        config.portfolio.var_confidence = 0.50; // too low
        let result = RiskConfigLoader::validate(&config);
        assert!(result.is_err(), "Should reject var_confidence outside (0.9, 1.0)");
    }

    #[test]
    fn validate_rejects_invalid_drawdown() {
        let mut config = RiskSystemConfig::default();
        config.pnl_limits.max_drawdown_pct = 1.5; // > 1.0
        let result = RiskConfigLoader::validate(&config);
        assert!(result.is_err(), "Should reject max_drawdown_pct > 1.0");
    }

    #[test]
    fn validate_rejects_zero_max_quantity() {
        let mut config = RiskSystemConfig::default();
        config.order_risk.max_quantity = 0.0;
        let result = RiskConfigLoader::validate(&config);
        assert!(result.is_err(), "Should reject max_quantity = 0");
    }
}
