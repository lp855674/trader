use std::collections::HashMap;
use marketdata::monitor::{DataMetricsCollector, DataAlertManager, DataTracer};
use marketdata::api::health::DataHealthChecker;
use marketdata::lifecycle::GracefulShutdown;

// 1. MetricsCollector snapshot reflects recorded data
#[test]
fn metrics_collector_snapshot_reflects_data() {
    let mut collector = DataMetricsCollector::new();
    collector.record_query(500, true);
    collector.record_query(300, true);
    collector.record_query(200, false);
    collector.record_ingestion(1000);
    collector.record_quality(0.92);

    let snap = collector.snapshot(3000);
    // 2 hits out of 3 = 0.667
    assert!((snap.cache_hit_rate - 2.0 / 3.0).abs() < 1e-9);
    // avg latency = (500+300+200)/3 = 333.33
    assert!((snap.avg_query_latency_us - 1000.0 / 3.0).abs() < 1e-6);
    assert_eq!(snap.items_ingested_total, 1000);
    assert!((snap.quality_score - 0.92).abs() < 1e-9);
}

// 2. AlertManager triggers on low cache hit rate
#[test]
fn alert_manager_triggers_on_low_cache_hit() {
    let mut thresholds = HashMap::new();
    thresholds.insert("min_cache_hit_rate".to_string(), 0.8);
    thresholds.insert("min_quality_score".to_string(), 0.7);
    let mut manager = DataAlertManager::new(thresholds);

    let mut collector = DataMetricsCollector::new();
    // All misses → cache hit rate = 0
    collector.record_query(100, false);
    collector.record_query(100, false);
    collector.record_quality(0.95);

    let snap = collector.snapshot(1000);
    let alerts = manager.check(&snap, 1000);
    assert!(!alerts.is_empty(), "Should have at least one alert");
    assert!(alerts.iter().any(|a| matches!(a.alert_type, marketdata::monitor::DataAlertType::LowCacheHitRate)));
    assert!(manager.active_count() > 0);
}

// 3. Tracer avg duration computed correctly
#[test]
fn tracer_avg_duration_correct() {
    let mut tracer = DataTracer::new(100);
    let id1 = tracer.start("fetch", 0);
    tracer.finish(&id1, 1000);
    let id2 = tracer.start("fetch", 2000);
    tracer.finish(&id2, 2600);
    let id3 = tracer.start("fetch", 5000);
    tracer.finish(&id3, 5200);

    let avg = tracer.avg_duration_for("fetch").unwrap();
    // durations: 1000, 600, 200 → avg = 1800/3 = 600
    assert!((avg - 600.0).abs() < 1e-9, "Expected 600, got {}", avg);
}

// 4. HealthChecker returns Degraded on low quality
#[test]
fn health_checker_degraded_on_low_quality() {
    let mut collector = DataMetricsCollector::new();
    collector.record_query(100, true);
    collector.record_quality(0.5); // low quality

    let snap = collector.snapshot(0);
    let report = DataHealthChecker::check(&snap, 0.8, 0.5);
    assert!(!report.quality_ok);
    assert!(matches!(report.overall, marketdata::api::health::DataHealthStatus::Degraded(_)));
}

// 5. GracefulShutdown registers and signals correctly
#[test]
fn graceful_shutdown_registers_and_signals() {
    let mut gs = GracefulShutdown::new(10_000);
    gs.register("pipeline");
    gs.register("cache");
    gs.register("source");

    assert_eq!(gs.registered_components.len(), 3);
    assert!(!gs.signal.is_triggered());

    gs.initiate();
    assert!(gs.signal.is_triggered());
    assert!(gs.await_completion(0));
}
