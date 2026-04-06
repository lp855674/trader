use marketdata::core::{DataItem, DataQuery, DataSource, Granularity, InMemoryDataSource};
use marketdata::parser::{FileParser, CsvConfig, ApiParser};
use marketdata::clean::{DataCleaner, CleaningRule};
use marketdata::align::{TimeAligner, FillStrategy};
use domain::NormalizedBar;

fn bar(ts_ms: i64, close: f64) -> NormalizedBar {
    NormalizedBar {
        ts_ms,
        open: close,
        high: close,
        low: close,
        close,
        volume: 10.0,
    }
}

fn bar_item(ts_ms: i64) -> DataItem {
    DataItem::Bar(bar(ts_ms, 1.0))
}

// ── Test 1: CSV parse → DataCleaner → TimeAligner pipeline ────────────────────

#[test]
fn csv_to_cleaner_to_aligner_pipeline() {
    let csv = "ts_ms,open,high,low,close,volume
0,10.0,10.0,10.0,10.0,100.0
1000,10.0,10.0,10.0,10.0,0.0
2000,10.0,10.0,10.0,10.0,100.0
5000,11.0,11.0,11.0,11.0,200.0";

    let config = CsvConfig::default();
    let bars = FileParser::csv_to_bars(csv, &config).expect("parse should succeed");
    assert_eq!(bars.len(), 4);

    // Clean: remove zero-volume bars
    let cleaner = DataCleaner::new(vec![CleaningRule::RequirePositiveVolume]);
    let (cleaned, report) = cleaner.clean(bars);
    assert_eq!(report.zero_volume_removed, 1);
    assert_eq!(cleaned.len(), 3);

    // Align: forward fill gaps at 1000ms intervals
    let aligned = TimeAligner::align(&cleaned, 1000, FillStrategy::ForwardFill);
    assert!(aligned.len() >= 3);
    assert!(aligned.iter().all(|b| b.ts_ms >= 0));
}

// ── Test 2: DataQuery with InMemoryDataSource ─────────────────────────────────

#[test]
fn data_query_with_in_memory_source() {
    let items: Vec<DataItem> = (0..20).map(|i| bar_item(i * 1000)).collect();
    let source = InMemoryDataSource::new("test", items);

    let q = DataQuery::new("", 5000, 10000);
    let result = source.query(&q).unwrap();
    assert_eq!(result.len(), 6); // ts 5000, 6000, 7000, 8000, 9000, 10000

    let q_limited = DataQuery::new("", 0, 19000).with_limit(5);
    let limited = source.query(&q_limited).unwrap();
    assert_eq!(limited.len(), 5);
}

// ── Test 3: Rate limiter rejects when depleted ────────────────────────────────

#[test]
fn rate_limiter_rejects_when_depleted() {
    use marketdata::parser::RateLimiter;
    let mut rl = RateLimiter::new(3.0);
    // 3 requests should succeed
    assert!(rl.try_acquire(0));
    assert!(rl.try_acquire(0));
    assert!(rl.try_acquire(0));
    // 4th should fail (depleted)
    assert!(!rl.try_acquire(0));
    // After 1 second, should have tokens again
    assert!(rl.try_acquire(1000));
}

// ── Test 4: Gap detection on synthetic series with known gap ──────────────────

#[test]
fn gap_detection_on_known_series() {
    let bars = vec![
        bar(0, 1.0),
        bar(1000, 1.0),
        bar(2000, 1.0),
        // gap from 2000 to 7000 (5000ms gap, expected 1000ms)
        bar(7000, 1.0),
        bar(8000, 1.0),
    ];
    let gaps = TimeAligner::detect_gaps(&bars, 1000);
    assert_eq!(gaps.len(), 1);
    assert_eq!(gaps[0].start_ts_ms, 2000);
    assert_eq!(gaps[0].end_ts_ms, 7000);
    assert_eq!(gaps[0].gap_ms, 5000);
}
