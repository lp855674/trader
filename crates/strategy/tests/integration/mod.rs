//! Integration tests for Strategy System
//!
//! Tests for end-to-end data flow, scheduler timing, and component interactions.

use crate::data::kline::{Granularity, Kline, KlineAggregator, Resampler};
use crate::event::EventBusEvent;
use crate::scheduler::{BackpressureHandler, HybridScheduler, SchedulerConfig, SchedulerEvent};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::time::sleep;

/// Test pure function determinism
#[test]
fn test_kline_determinism() {
    let mut aggregator = KlineAggregator::new("BTC/USDT".to_string(), Granularity::Minute(1));

    // First run
    aggregator.update(&Kline::new(
        "BTC/USDT".to_string(),
        1712345600000,
        100.0, 105.0, 99.0, 102.0, 100.0,
    ));
    aggregator.update(&Kline::new(
        "BTC/USDT".to_string(),
        1712345610000,
        102.5, 103.0, 101.0, 103.0, 50.0,
    ));

    let result1 = aggregator.get_current().unwrap();

    // Second run with same input
    let mut aggregator2 = KlineAggregator::new("BTC/USDT".to_string(), Granularity::Minute(1));
    aggregator2.update(&Kline::new(
        "BTC/USDT".to_string(),
        1712345600000,
        100.0, 105.0, 99.0, 102.0, 100.0,
    ));
    aggregator2.update(&Kline::new(
        "BTC/USDT".to_string(),
        1712345610000,
        102.5, 103.0, 101.0, 103.0, 50.0,
    ));

    let result2 = aggregator2.get_current().unwrap();

    assert_eq!(result1.open, result2.open);
    assert_eq!(result1.high, result2.high);
    assert_eq!(result1.low, result2.low);
    assert_eq!(result1.close, result2.close);
}

/// Test scheduler timing behavior
#[tokio::test]
async fn test_hybrid_scheduler_timer_ticks() {
    let (tx, _) = broadcast::channel(100);
    let mut scheduler = HybridScheduler::new(50, Arc::new(tx), SchedulerConfig::default());

    let handle = tokio::spawn(async move {
        scheduler.run().await;
    });

    // Wait for some ticks
    sleep(Duration::from_millis(150)).await;

    // Should have received at least 3 ticks (150ms / 50ms = 3)
    let count = tx.receiver_count();
    assert!(count >= 3, "Should have received timer ticks");

    handle.abort();
}

/// Test event bus sequence numbers
#[tokio::test]
async fn test_event_bus_sequence() {
    let (bus, subscriber) = crate::event::EventBus::new(100);

    let seq1 = subscriber.get_sequence();
    assert_eq!(seq1, 0);

    // Send 5 events
    for i in 0..5 {
        subscriber.send(EventBusEvent::DataUpdate {
            instrument_id: "BTC/USDT".to_string(),
            granularity: "1m".to_string(),
            timestamp_ms: 1712345678000 + i as i64,
        }).unwrap();
    }

    let seq2 = subscriber.get_sequence();
    assert_eq!(seq2, 5, "Sequence should increment by 5");
}

/// Test backpressure handler rate limiting
#[tokio::test]
async fn test_backpressure_rate_limiting() {
    let handler = BackpressureHandler::new(5); // 5 events per second

    // First 5 should pass immediately
    for i in 0..5 {
        assert!(handler.allow().await, "Event {} should pass", i);
    }

    // 6th should be blocked (rate limited)
    let start = std::time::Instant::now();
    let result = handler.allow().await;
    let elapsed = start.elapsed();

    assert!(!result, "6th event should be blocked");
    assert!(elapsed >= Duration::from_millis(1), "Should have waited at least 1ms");
}

/// Test resampler with gap detection
#[test]
fn test_resampler_gap_detection() {
    let mut resampler = Resampler::new("BTC/USDT".to_string(), Granularity::Minute(1));

    // Add ticks with gaps
    resampler.update(&Kline::new(
        "BTC/USDT".to_string(),
        1712345600000,
        100.0, 105.0, 99.0, 102.0, 100.0,
    ));

    // Normal tick
    resampler.update(&Kline::new(
        "BTC/USDT".to_string(),
        1712345610000,
        102.5, 103.0, 101.0, 103.0, 50.0,
    ));

    // Gap of 10 seconds (should be detected but still aggregate)
    resampler.update(&Kline::new(
        "BTC/USDT".to_string(),
        1712345620000,
        103.5, 104.0, 102.5, 104.0, 30.0,
    ));

    let current = resampler.get_current_kline().unwrap();
    assert_eq!(current.close, 104.0);
}

/// Test end-to-end data flow
#[tokio::test]
async fn test_end_to_end_flow() {
    let (tx, _) = broadcast::channel(100);
    let (bus, _) = crate::event::EventBus::new(100);

    let mut scheduler = HybridScheduler::new(100, Arc::new(tx), SchedulerConfig::default());

    // Start event bus
    let bus_handle = tokio::spawn(async move {
        let mut event_bus = crate::event::EventBus::new(100);
        loop {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => break,
                event = event_bus.channel.recv() => {
                    if event.is_err() {
                        break;
                    }
                }
            }
        }
    });

    // Run scheduler briefly
    let scheduler_handle = tokio::spawn(async move {
        scheduler.run().await;
    });

    // Give time to start
    sleep(Duration::from_millis(50)).await;

    // Send events through both channels
    let _ = tx.send(SchedulerEvent::DataUpdate {
        instrument_id: "BTC/USDT".to_string(),
        timestamp_ms: 1712345678000,
    });

    let _ = bus.send(EventBusEvent::DataUpdate {
        instrument_id: "BTC/USDT".to_string(),
        granularity: "1m".to_string(),
        timestamp_ms: 1712345678000,
    });

    sleep(Duration::from_millis(100)).await;

    bus_handle.abort();
    scheduler_handle.abort();
}

/// Test kline aggregation with volume calculation
#[test]
fn test_volume_accumulation() {
    let mut aggregator = KlineAggregator::new("BTC/USDT".to_string(), Granularity::Minute(1));

    aggregator.update(&Kline::new(
        "BTC/USDT".to_string(),
        1712345600000,
        100.0, 101.0, 99.0, 100.5, 1000.0,
    ));

    aggregator.update(&Kline::new(
        "BTC/USDT".to_string(),
        1712345610000,
        100.5, 102.0, 100.0, 101.5, 500.0,
    ));

    let current = aggregator.get_current().unwrap();
    assert_eq!(current.volume, 1500.0, "Volume should accumulate");
}

/// Test high frequency tick aggregation
#[test]
fn test_high_frequency_aggregation() {
    let mut aggregator = KlineAggregator::new("BTC/USDT".to_string(), Granularity::Minute(1));

    // Simulate 100 ticks in one minute
    for i in 0..100 {
        aggregator.update(&Kline::new(
            "BTC/USDT".to_string(),
            1712345600000 + (i * 1000) as i64, // every 1 second
            100.0 + (i as f64 * 0.01),
            100.0 + (i as f64 * 0.01) + 0.5,
            100.0 + (i as f64 * 0.01) - 0.5,
            100.0 + (i as f64 * 0.01) + 0.2,
            10.0,
        ));
    }

    let current = aggregator.get_current().unwrap();
    assert_eq!(current.volume, 1000.0, "Total volume should be 1000");
    assert_eq!(current.high, 100.5, "High should be 100.5");
    assert_eq!(current.low, 99.5, "Low should be 99.5");
}
