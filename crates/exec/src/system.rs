use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::config::ExecConfig;

pub struct SystemIntegration {
    pub config: ExecConfig,
    shutdown_flag: Arc<AtomicBool>,
}

impl SystemIntegration {
    pub fn new(config: ExecConfig) -> Self {
        Self {
            config,
            shutdown_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn request_shutdown(&self) {
        self.shutdown_flag.store(true, Ordering::SeqCst);
    }

    pub fn is_shutdown_requested(&self) -> bool {
        self.shutdown_flag.load(Ordering::SeqCst)
    }

    pub fn validate_config(&self) -> Result<(), String> {
        let exec = &self.config.execution;
        if exec.max_order_size <= 0.0 {
            return Err("max_order_size must be positive".to_string());
        }
        if !(0.0..=1.0).contains(&exec.max_position_pct) {
            return Err("max_position_pct must be in [0, 1]".to_string());
        }
        if exec.default_slippage_bps < 0.0 {
            return Err("default_slippage_bps must be non-negative".to_string());
        }
        let broker = &self.config.broker;
        if broker.venue.is_empty() {
            return Err("broker.venue must not be empty".to_string());
        }
        if broker.api_url.is_empty() {
            return Err("broker.api_url must not be empty".to_string());
        }
        let risk = &self.config.risk_limits;
        if risk.max_drawdown_pct <= 0.0 || risk.max_drawdown_pct > 1.0 {
            return Err("max_drawdown_pct must be in (0, 1]".to_string());
        }
        if risk.max_daily_loss <= 0.0 {
            return Err("max_daily_loss must be positive".to_string());
        }
        if risk.max_leverage < 1.0 {
            return Err("max_leverage must be >= 1.0".to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_validates() {
        let sys = SystemIntegration::new(ExecConfig::default());
        assert!(sys.validate_config().is_ok());
    }

    #[test]
    fn shutdown_flag_starts_false() {
        let sys = SystemIntegration::new(ExecConfig::default());
        assert!(!sys.is_shutdown_requested());
    }

    #[test]
    fn shutdown_request_sets_flag() {
        let sys = SystemIntegration::new(ExecConfig::default());
        sys.request_shutdown();
        assert!(sys.is_shutdown_requested());
    }

    #[test]
    fn negative_order_size_invalid() {
        let mut cfg = ExecConfig::default();
        cfg.execution.max_order_size = -1.0;
        let sys = SystemIntegration::new(cfg);
        assert!(sys.validate_config().is_err());
    }
}
