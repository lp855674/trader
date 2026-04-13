use infra::lifecycle::shutdown::GracefulShutdown;
use infra::lifecycle::watchdog::Watchdog;
use infra::otel::metrics::MetricsCollector;
use infra::services::circuit::CircuitBreaker;
use infra::services::discovery::{ServiceInfo, ServiceRegistry};

#[test]
fn metrics_counter_accumulates() {
    let mut mc = MetricsCollector::new();
    mc.record_counter("fills", 10);
    mc.record_counter("fills", 25);
    let snap = mc.snapshot();
    match snap["fills"] {
        infra::otel::metrics::MetricValue::Counter(c) => assert_eq!(c, 35),
        _ => panic!("expected counter"),
    }
}

#[test]
fn shutdown_lifecycle_completes() {
    let mut sd = GracefulShutdown::new(5000);
    sd.advance_phase(); // Running -> Initiated
    sd.complete_step("flush_orders");
    sd.advance_phase(); // Initiated -> DrainConnections
    sd.advance_phase(); // DrainConnections -> SaveState
    sd.advance_phase(); // SaveState -> Complete
    assert!(sd.is_complete());
    assert_eq!(sd.steps_completed().len(), 1);
}

#[test]
fn watchdog_detects_timeout() {
    let mut wd = Watchdog::new();
    wd.register("market_data_feed", 500);
    wd.tick(600);
    let unhealthy = wd.unhealthy();
    assert!(unhealthy.contains(&"market_data_feed".to_string()));
}

#[test]
fn circuit_breaker_opens_on_failures() {
    let mut cb = CircuitBreaker::new(3, 2);
    cb.call_failure();
    cb.call_failure();
    cb.call_failure();
    assert!(cb.is_open());
}

#[test]
fn service_registry_finds_healthy() {
    let mut reg = ServiceRegistry::new();
    reg.register(ServiceInfo {
        name: "strategy_svc".to_string(),
        address: "127.0.0.1:9001".to_string(),
        healthy: true,
        version: "1.0".to_string(),
    });
    reg.register(ServiceInfo {
        name: "risk_svc".to_string(),
        address: "127.0.0.1:9002".to_string(),
        healthy: true,
        version: "1.0".to_string(),
    });
    reg.heartbeat("risk_svc", false);
    assert_eq!(reg.healthy_services().len(), 1);
    assert_eq!(reg.healthy_services()[0].name, "strategy_svc");
}
