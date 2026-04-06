use exec::config::ExecConfig;
use exec::system::SystemIntegration;

#[test]
fn config_default_values_are_valid() {
    let cfg = ExecConfig::default();
    let sys = SystemIntegration::new(cfg);
    assert!(sys.validate_config().is_ok());
}

#[test]
fn config_json_roundtrip() {
    let cfg = ExecConfig::default();
    let json = cfg.to_json();
    let parsed = ExecConfig::from_json(&json).expect("json parse");
    assert_eq!(parsed.execution.max_order_size, cfg.execution.max_order_size);
    assert_eq!(parsed.broker.venue, cfg.broker.venue);
    assert_eq!(parsed.risk_limits.max_drawdown_pct, cfg.risk_limits.max_drawdown_pct);
    assert_eq!(parsed.monitoring.metrics_interval_secs, cfg.monitoring.metrics_interval_secs);
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
fn invalid_config_fails_validation() {
    let mut cfg = ExecConfig::default();
    cfg.execution.max_order_size = -500.0;
    let sys = SystemIntegration::new(cfg);
    assert!(sys.validate_config().is_err());
}
