use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionSettings {
    pub max_order_size: f64,
    pub max_position_pct: f64,
    pub default_slippage_bps: f64,
}

impl Default for ExecutionSettings {
    fn default() -> Self {
        Self {
            max_order_size: 100_000.0,
            max_position_pct: 0.10,
            default_slippage_bps: 5.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerConfig {
    pub venue: String,
    pub api_url: String,
    pub timeout_ms: u64,
    pub max_connections: u8,
}

impl Default for BrokerConfig {
    fn default() -> Self {
        Self {
            venue: "paper".to_string(),
            api_url: "http://localhost:8080".to_string(),
            timeout_ms: 5000,
            max_connections: 10,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskLimits {
    pub max_drawdown_pct: f64,
    pub max_daily_loss: f64,
    pub max_leverage: f64,
}

impl Default for RiskLimits {
    fn default() -> Self {
        Self {
            max_drawdown_pct: 0.20,
            max_daily_loss: 50_000.0,
            max_leverage: 2.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    pub metrics_interval_secs: u64,
    pub alert_threshold_ms: u64,
    pub enable_tracing: bool,
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            metrics_interval_secs: 30,
            alert_threshold_ms: 500,
            enable_tracing: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExecConfig {
    pub execution: ExecutionSettings,
    pub broker: BrokerConfig,
    pub risk_limits: RiskLimits,
    pub monitoring: MonitoringConfig,
}

impl ExecConfig {
    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_values_are_sensible() {
        let cfg = ExecConfig::default();
        assert!(cfg.execution.max_order_size > 0.0);
        assert!(cfg.execution.max_position_pct > 0.0 && cfg.execution.max_position_pct <= 1.0);
        assert!(!cfg.broker.venue.is_empty());
        assert!(!cfg.broker.api_url.is_empty());
        assert!(cfg.risk_limits.max_leverage >= 1.0);
        assert!(cfg.monitoring.metrics_interval_secs > 0);
    }

    #[test]
    fn json_roundtrip() {
        let cfg = ExecConfig::default();
        let json = cfg.to_json();
        let parsed: ExecConfig = ExecConfig::from_json(&json).expect("parse");
        assert_eq!(parsed.execution.max_order_size, cfg.execution.max_order_size);
        assert_eq!(parsed.broker.venue, cfg.broker.venue);
        assert_eq!(parsed.risk_limits.max_leverage, cfg.risk_limits.max_leverage);
    }
}
