use feature_store::{FeatureKey, FeatureRecord, FeatureStore, InMemoryFeatureStore};
use rust_decimal_macros::dec;

#[test]
fn in_memory_feature_store_round_trips_latest_feature_value() {
    let mut store = InMemoryFeatureStore::default();
    store.insert(FeatureRecord::new(
        "run-1",
        "US:NASDAQ:AAPL:EQUITY",
        1,
        "sma_20",
        dec!(101.25),
        "v1",
    ));
    store.insert(FeatureRecord::new(
        "run-1",
        "US:NASDAQ:AAPL:EQUITY",
        2,
        "sma_20",
        dec!(102.50),
        "v1",
    ));

    let key = FeatureKey::new("run-1", "US:NASDAQ:AAPL:EQUITY", "sma_20");
    let latest = store.latest(&key).unwrap();

    assert_eq!(latest.ts_ms, 2);
    assert_eq!(latest.value, dec!(102.50));
}

#[test]
fn in_memory_feature_store_queries_time_range_in_order() {
    let mut store = InMemoryFeatureStore::default();
    for (ts_ms, value) in [(3, dec!(103)), (1, dec!(101)), (2, dec!(102))] {
        store.insert(FeatureRecord::new(
            "run-1",
            "US:NASDAQ:AAPL:EQUITY",
            ts_ms,
            "ema_20",
            value,
            "v1",
        ));
    }

    let key = FeatureKey::new("run-1", "US:NASDAQ:AAPL:EQUITY", "ema_20");
    let values = store.range(&key, 2, 3);

    assert_eq!(
        values.iter().map(|record| record.ts_ms).collect::<Vec<_>>(),
        vec![2, 3]
    );
}
