use feature_store::{
    FeatureKey, FeatureRecord, FeatureStore, InMemoryFeatureStore, ParquetFeatureStore,
    load_feature_records_from_parquet, write_feature_records_to_parquet,
};
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

#[test]
fn parquet_feature_records_round_trip_preserves_decimal_values() {
    let path = temp_parquet_path("feature-records-round-trip");
    let records = vec![
        FeatureRecord::new(
            "research-run-1",
            "US:NASDAQ:AAPL:EQUITY",
            1704067200000,
            "momentum_20d",
            dec!(0.123456789012345678),
            "v1",
        ),
        FeatureRecord::new(
            "research-run-1",
            "US:NASDAQ:AAPL:EQUITY",
            1704153600000,
            "momentum_20d",
            dec!(-0.000000000000000001),
            "v1",
        ),
    ];

    write_feature_records_to_parquet(&path, &records).unwrap();
    let loaded = load_feature_records_from_parquet(&path).unwrap();

    std::fs::remove_file(path).unwrap();
    assert_eq!(loaded, records);
}

#[test]
fn parquet_feature_store_reopens_as_queryable_store() {
    let path = temp_parquet_path("feature-store-reopen");
    let mut store = ParquetFeatureStore::create(&path);
    for (ts_ms, value) in [(3, dec!(103.33)), (1, dec!(101.11)), (2, dec!(102.22))] {
        store.insert(FeatureRecord::new(
            "research-run-2",
            "US:NASDAQ:MSFT:EQUITY",
            ts_ms,
            "sma_10",
            value,
            "v2",
        ));
    }
    store.flush().unwrap();

    let loaded = ParquetFeatureStore::open(&path).unwrap();
    let key = FeatureKey::new("research-run-2", "US:NASDAQ:MSFT:EQUITY", "sma_10");
    let values = loaded.range(&key, 2, 3);
    let latest = loaded.latest(&key).unwrap();

    std::fs::remove_file(path).unwrap();
    assert_eq!(
        values
            .iter()
            .map(|record| (record.ts_ms, record.value))
            .collect::<Vec<_>>(),
        vec![(2, dec!(102.22)), (3, dec!(103.33))]
    );
    assert_eq!(latest.ts_ms, 3);
    assert_eq!(latest.value, dec!(103.33));
}

fn temp_parquet_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "trader-feature-store-{name}-{}.parquet",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}
