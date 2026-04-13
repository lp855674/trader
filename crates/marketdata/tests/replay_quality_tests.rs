use domain::NormalizedBar;
use marketdata::align::GapSpec;
use marketdata::core::{DataItem, Granularity, InMemoryDataSource};
use marketdata::quality::{DataGapDetector, QualityChecker, QualityRule};
use marketdata::replay::{
    CallbackEvent, CallbackManager, GranularityReplayer, ReplayCallback, ReplayConfig,
    ReplayController,
};
use std::sync::{Arc, Mutex};

fn make_bars(count: usize, interval_ms: i64) -> Vec<DataItem> {
    (0..count)
        .map(|i| {
            DataItem::Bar(NormalizedBar {
                ts_ms: (i as i64) * interval_ms,
                open: 1.0,
                high: (i + 1) as f64,
                low: 1.0,
                close: (i + 1) as f64,
                volume: 10.0,
            })
        })
        .collect()
}

fn bar(ts_ms: i64, price: f64) -> NormalizedBar {
    NormalizedBar {
        ts_ms,
        open: price,
        high: price,
        low: price,
        close: price,
        volume: 10.0,
    }
}

// ── Test 1: ReplayController runs to completion ───────────────────────────────

#[test]
fn replay_controller_runs_to_completion() {
    let items = make_bars(10, 60_000); // 10 bars at 1-min intervals
    let source = Box::new(InMemoryDataSource::new("test", items));
    let config = ReplayConfig::new(0, 9 * 60_000);
    let mut ctrl = ReplayController::new(source, config).with_step_ms(60_000);

    let mut count = 0u64;
    let total = ctrl.run_to_completion(|_item| {
        count += 1;
    });

    assert_eq!(total, count);
    assert_eq!(total, 10);
}

// ── Test 2: GranularityReplayer aggregates 5x1min to 1x5min ──────────────────

#[test]
fn granularity_replayer_5min_from_1min() {
    // 5 one-minute bars should aggregate to 1 five-minute bar
    let items: Vec<DataItem> = (0..5)
        .map(|i| {
            DataItem::Bar(NormalizedBar {
                ts_ms: (i as i64) * 60_000,
                open: 1.0,
                high: (i + 1) as f64,
                low: 1.0,
                close: (i + 1) as f64,
                volume: 10.0,
            })
        })
        .collect();

    let source = Box::new(InMemoryDataSource::new("test", items));
    let config = ReplayConfig::new(0, 5 * 60_000 - 1);
    let ctrl = ReplayController::new(source, config).with_step_ms(5 * 60_000);
    let mut replayer = GranularityReplayer::new(ctrl, Granularity::Minutes(5));

    let bars = replayer.step().unwrap();
    assert!(!bars.is_empty());
    let total_volume: f64 = bars.iter().map(|b| b.volume).sum();
    assert!((total_volume - 50.0).abs() < 1e-9);
}

// ── Test 3: QualityChecker detects price-out-of-range ────────────────────────

#[test]
fn quality_checker_detects_price_out_of_range() {
    let bars = vec![
        bar(1000, 50.0),  // OK
        bar(2000, 5.0),   // Out of range (< 10)
        bar(3000, 200.0), // Out of range (> 100)
    ];

    let checker = QualityChecker::new(vec![QualityRule::PriceInRange {
        min: 10.0,
        max: 100.0,
    }]);
    let violations = checker.check_bars(&bars);
    assert!(!violations.is_empty());
    // Should have violations for ts=2000 and ts=3000
    let ts_ms_set: Vec<i64> = violations.iter().map(|v| v.ts_ms).collect();
    assert!(ts_ms_set.contains(&2000));
    assert!(ts_ms_set.contains(&3000));
    assert!(!ts_ms_set.contains(&1000));
}

// ── Test 4: DataGapDetector finds known gap ───────────────────────────────────

#[test]
fn gap_detector_finds_known_gap() {
    let bars = vec![
        bar(0, 1.0),
        bar(1_000, 1.0),
        bar(2_000, 1.0),
        // gap here from 2000 to 8000
        bar(8_000, 1.0),
        bar(9_000, 1.0),
    ];

    let report = DataGapDetector::detect("BTC", &bars, 1_000);
    assert_eq!(report.total_bars, 5);
    assert_eq!(report.gaps.len(), 1);
    assert_eq!(report.gaps[0].start_ts_ms, 2_000);
    assert_eq!(report.gaps[0].end_ts_ms, 8_000);
    assert_eq!(report.gaps[0].gap_ms, 6_000);
}

// ── Test 5: CallbackManager fires events to multiple callbacks ────────────────

#[test]
fn callback_manager_fires_to_multiple_callbacks() {
    struct Counter {
        count: Arc<Mutex<u32>>,
    }

    impl ReplayCallback for Counter {
        fn on_event(&mut self, _event: CallbackEvent) {
            *self.count.lock().unwrap() += 1;
        }
    }

    let c1 = Arc::new(Mutex::new(0u32));
    let c2 = Arc::new(Mutex::new(0u32));
    let c3 = Arc::new(Mutex::new(0u32));

    let mut mgr = CallbackManager::new();
    mgr.add(Box::new(Counter { count: c1.clone() }));
    mgr.add(Box::new(Counter { count: c2.clone() }));
    mgr.add(Box::new(Counter { count: c3.clone() }));

    let b = NormalizedBar {
        ts_ms: 1000,
        open: 1.0,
        high: 1.0,
        low: 1.0,
        close: 1.0,
        volume: 1.0,
    };

    mgr.fire(CallbackEvent::OnBar(b.clone()));
    mgr.fire(CallbackEvent::OnBar(b.clone()));
    mgr.fire(CallbackEvent::OnComplete { total_items: 10 });

    assert_eq!(*c1.lock().unwrap(), 3);
    assert_eq!(*c2.lock().unwrap(), 3);
    assert_eq!(*c3.lock().unwrap(), 3);
}
