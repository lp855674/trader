// Chaos engineering tests: inject failures and verify resilience.

use infra::lifecycle::cleanup::{ResourceCleanup, ResourceType};
use infra::lifecycle::shutdown::GracefulShutdown;
use infra::lifecycle::watchdog::Watchdog;
use infra::services::circuit::{CircuitBreaker, CircuitState};
use infra::services::discovery::{ServiceInfo, ServiceRegistry};

/// Simulate a random failure pattern using a deterministic LCG.
struct LcgRng {
    state: u64,
}
impl LcgRng {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }
    fn next(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.state >> 33
    }
    fn next_bool(&mut self, probability_pct: u64) -> bool {
        self.next() % 100 < probability_pct
    }
}

#[test]
fn random_failures_circuit_breaker_recovers() {
    // Inject random call failures; circuit should open then allow recovery.
    let mut cb = CircuitBreaker::new(3, 2);
    let mut rng = LcgRng::new(0xDEADBEEF);
    let mut opened = false;

    for _ in 0..50 {
        if rng.next_bool(40) {
            cb.call_failure();
        } else {
            cb.call_success();
        }
        if cb.is_open() {
            opened = true;
        }
    }
    // After many successes, circuit should have opened at some point
    // (40% failure rate over 50 calls will definitely exceed threshold of 3)
    assert!(opened, "circuit should have opened under 40% failure rate");
}

#[test]
fn resource_exhaustion_cleanup_handles_many_resources() {
    let mut rc = ResourceCleanup::new();
    // Register a large number of resources (simulating exhaustion)
    for i in 0..1000 {
        let kind = match i % 4 {
            0 => ResourceType::Memory,
            1 => ResourceType::FileHandle,
            2 => ResourceType::NetworkConnection,
            _ => ResourceType::DatabaseConn,
        };
        rc.register(kind, &format!("resource_{}", i));
    }
    assert_eq!(rc.pending_count(), 1000);
    let cleaned = rc.cleanup_all();
    assert_eq!(cleaned.len(), 1000);
    assert_eq!(rc.pending_count(), 0);
}

#[test]
fn concurrent_shutdown_phases_complete_safely() {
    // Simulate multiple concurrent shutdown sequences (single-threaded deterministic)
    let mut shutdowns: Vec<GracefulShutdown> =
        (0..10).map(|_| GracefulShutdown::new(5000)).collect();

    // Advance each through all phases
    for sd in &mut shutdowns {
        sd.advance_phase();
        sd.complete_step("flush");
        sd.advance_phase();
        sd.advance_phase();
        sd.advance_phase();
    }

    assert!(shutdowns.iter().all(|sd| sd.is_complete()));
}

#[test]
fn watchdog_multiple_services_partial_failure() {
    let mut wd = Watchdog::new();
    // Register 5 services with different timeouts
    for i in 0..5 {
        wd.register(&format!("svc_{}", i), 1000 + i as u64 * 100);
    }
    // Tick past shortest timeout only
    wd.tick(1050);
    // svc_0 (timeout 1000) is the only one that timed out
    let unhealthy = wd.unhealthy();
    assert!(
        unhealthy.contains(&"svc_0".to_string()),
        "svc_0 should be unhealthy"
    );
    // Heartbeat svc_0 and tick again — it should recover
    wd.heartbeat("svc_0");
    wd.tick(50);
    let unhealthy2 = wd.unhealthy();
    assert!(
        !unhealthy2.contains(&"svc_0".to_string()),
        "svc_0 should recover after heartbeat"
    );
}

#[test]
fn service_registry_flapping_health() {
    // Simulate a flapping service (oscillates between healthy/unhealthy)
    let mut reg = ServiceRegistry::new();
    reg.register(ServiceInfo {
        name: "flapping_svc".to_string(),
        address: "127.0.0.1:9999".to_string(),
        healthy: true,
        version: "1.0".to_string(),
    });

    let mut rng = LcgRng::new(12345);
    let mut healthy_snapshots = 0usize;
    let mut total = 0usize;
    for _ in 0..100 {
        let healthy = rng.next_bool(60); // 60% healthy
        reg.heartbeat("flapping_svc", healthy);
        if reg.healthy_services().len() == 1 {
            healthy_snapshots += 1;
        }
        total += 1;
    }
    // Roughly 60% of snapshots should show healthy
    let ratio = healthy_snapshots as f64 / total as f64;
    assert!(
        ratio > 0.4 && ratio < 0.8,
        "health ratio {} out of expected range",
        ratio
    );
}
