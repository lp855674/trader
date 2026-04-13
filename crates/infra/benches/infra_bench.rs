// Infrastructure performance benchmarks.
// Run with: cargo bench -p infra

use std::hint::black_box;
use std::time::Instant;

fn bench_metrics_throughput(n: usize) {
    use infra::otel::metrics::MetricsCollector;
    let mut mc = MetricsCollector::new();
    let start = Instant::now();
    for i in 0..n {
        mc.record_counter(black_box("orders"), black_box(1));
        mc.record_gauge(black_box("cpu"), black_box(i as f64 / n as f64));
    }
    let elapsed = start.elapsed().as_nanos() as f64;
    println!(
        "metrics_throughput: {n} ops in {:.2}ms, avg {:.1}ns/op",
        elapsed / 1_000_000.0,
        elapsed / n as f64
    );
}

fn bench_circuit_breaker(n: usize) {
    use infra::services::circuit::CircuitBreaker;
    let mut cb = CircuitBreaker::new(100, 10);
    let start = Instant::now();
    for i in 0..n {
        if i % 3 == 0 {
            cb.call_failure();
        } else {
            cb.call_success();
        }
        black_box(cb.is_open());
    }
    let elapsed = start.elapsed().as_nanos() as f64;
    println!(
        "circuit_breaker: {n} ops in {:.2}ms, avg {:.1}ns/op",
        elapsed / 1_000_000.0,
        elapsed / n as f64
    );
}

fn bench_watchdog_tick(n: usize) {
    use infra::lifecycle::watchdog::Watchdog;
    let mut wd = Watchdog::new();
    for i in 0..100 {
        wd.register(&format!("svc_{}", i), 5000);
    }
    let start = Instant::now();
    for _ in 0..n {
        wd.tick(black_box(1));
        black_box(wd.unhealthy());
    }
    let elapsed = start.elapsed().as_nanos() as f64;
    println!(
        "watchdog_tick (100 services): {n} ticks in {:.2}ms, avg {:.1}ns/tick",
        elapsed / 1_000_000.0,
        elapsed / n as f64
    );
}

fn bench_load_balancer(n: usize) {
    use infra::services::balance::{BalanceStrategy, LoadBalancer};
    let mut lb = LoadBalancer::new(BalanceStrategy::RoundRobin);
    for i in 0..10 {
        lb.add_node(&format!("n{}", i), &format!("10.0.0.{}:9090", i), 1);
    }
    let start = Instant::now();
    for _ in 0..n {
        black_box(lb.select(None));
    }
    let elapsed = start.elapsed().as_nanos() as f64;
    println!(
        "load_balancer_rr (10 nodes): {n} selects in {:.2}ms, avg {:.1}ns/select",
        elapsed / 1_000_000.0,
        elapsed / n as f64
    );
}

fn bench_shutdown_latency() {
    use infra::lifecycle::shutdown::GracefulShutdown;
    let n = 10_000;
    let start = Instant::now();
    for _ in 0..n {
        let mut sd = GracefulShutdown::new(5000);
        sd.advance_phase();
        sd.advance_phase();
        sd.advance_phase();
        sd.advance_phase();
        black_box(sd.is_complete());
    }
    let elapsed = start.elapsed().as_nanos() as f64;
    println!(
        "shutdown_latency: {n} full cycles in {:.2}ms, avg {:.1}ns/cycle",
        elapsed / 1_000_000.0,
        elapsed / n as f64
    );
}

fn main() {
    println!("=== Infrastructure Performance Benchmarks ===\n");

    let n = 100_000;
    bench_metrics_throughput(n);
    bench_circuit_breaker(n);
    bench_watchdog_tick(10_000);
    bench_load_balancer(n);
    bench_shutdown_latency();

    println!("\n=== Done ===");
}
