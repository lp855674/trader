use feature_store::{
    FeatureBuildContract, FeatureBuildContractExpectation, FeatureKey, FeatureManifestInput,
    FeatureRecord, FeatureStore, InMemoryFeatureStore, ParquetFeatureStore, build_feature_manifest,
    build_feature_manifest_with_contract, load_feature_manifest, load_feature_records_from_parquet,
    validate_feature_manifest_for_build_contract, validate_feature_manifest_for_gate,
    validate_feature_manifest_for_input_contract, write_feature_manifest,
    write_feature_records_to_parquet,
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

#[test]
fn feature_manifest_summarizes_feature_records_for_reproducibility() {
    let records = vec![
        FeatureRecord::new(
            "research-run-1",
            "US:NASDAQ:AAPL:EQUITY",
            10,
            "quality_score",
            dec!(0.8),
            "v2",
        ),
        FeatureRecord::new(
            "research-run-1",
            "US:NASDAQ:MSFT:EQUITY",
            10,
            "quality_score",
            dec!(0.7),
            "v2",
        ),
        FeatureRecord::new(
            "research-run-2",
            "US:NASDAQ:AAPL:EQUITY",
            11,
            "momentum_20d",
            dec!(0.1),
            "v1",
        ),
    ];

    let manifest = build_feature_manifest("datasets/features/research.parquet", &records);

    assert_eq!(manifest.schema_version, 1);
    assert_eq!(manifest.parquet_path, "datasets/features/research.parquet");
    assert_eq!(manifest.record_count, 3);
    assert_eq!(
        manifest.run_ids,
        vec!["research-run-1".to_string(), "research-run-2".to_string()]
    );
    assert_eq!(
        manifest.feature_names,
        vec!["momentum_20d".to_string(), "quality_score".to_string()]
    );
    assert_eq!(
        manifest.symbols,
        vec![
            "US:NASDAQ:AAPL:EQUITY".to_string(),
            "US:NASDAQ:MSFT:EQUITY".to_string()
        ]
    );
    assert_eq!(manifest.versions, vec!["v1".to_string(), "v2".to_string()]);
}

#[test]
fn feature_manifest_json_round_trip_preserves_summary() {
    let path = temp_json_path("feature-manifest-round-trip");
    let manifest = build_feature_manifest(
        "datasets/features/research.parquet",
        &[FeatureRecord::new(
            "research-run-1",
            "US:NASDAQ:AAPL:EQUITY",
            10,
            "quality_score",
            dec!(0.8),
            "v2",
        )],
    );

    write_feature_manifest(&path, &manifest).unwrap();
    let loaded = load_feature_manifest(&path).unwrap();

    std::fs::remove_file(path).unwrap();
    assert_eq!(loaded, manifest);
}

#[test]
fn feature_manifest_json_round_trip_preserves_build_contract() {
    let path = temp_json_path("feature-manifest-build-contract-round-trip");
    let build_contract = FeatureBuildContract {
        builder: "feature-build-indicator".to_string(),
        indicator: "sma".to_string(),
        value_column: "close".to_string(),
        period: 2,
        run_id: "research-run-1".to_string(),
        feature_name: "sma_close_2".to_string(),
        version: "v1".to_string(),
        inputs: vec![FeatureManifestInput {
            symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
            source: "csv".to_string(),
            path: "datasets/sample/aapl_1d.csv".to_string(),
            content_hash: None,
            bar_count: None,
            first_ts_ms: None,
            last_ts_ms: None,
        }],
    };
    let manifest = build_feature_manifest_with_contract(
        "datasets/features/aapl_sma_2.parquet",
        &[FeatureRecord::new(
            "research-run-1",
            "US:NASDAQ:AAPL:EQUITY",
            10,
            "sma_close_2",
            dec!(101),
            "v1",
        )],
        build_contract.clone(),
    );

    write_feature_manifest(&path, &manifest).unwrap();
    let loaded = load_feature_manifest(&path).unwrap();

    std::fs::remove_file(path).unwrap();
    assert_eq!(loaded, manifest);
    assert_eq!(loaded.build_contract.as_ref(), Some(&build_contract));
}

#[test]
fn feature_manifest_input_contract_rejects_mismatched_source_bars() {
    let manifest = build_feature_manifest_with_contract(
        "datasets/features/aapl_sma_2.parquet",
        &[FeatureRecord::new(
            "research-run-1",
            "US:NASDAQ:AAPL:EQUITY",
            10,
            "sma_close_2",
            dec!(101),
            "v1",
        )],
        FeatureBuildContract {
            builder: "feature-build-indicator".to_string(),
            indicator: "sma".to_string(),
            value_column: "close".to_string(),
            period: 2,
            run_id: "research-run-1".to_string(),
            feature_name: "sma_close_2".to_string(),
            version: "v1".to_string(),
            inputs: vec![FeatureManifestInput {
                symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
                source: "csv".to_string(),
                path: "datasets/sample/research_aapl_1d.csv".to_string(),
                content_hash: Some("fnv1a64:old".to_string()),
                bar_count: Some(3),
                first_ts_ms: Some(1),
                last_ts_ms: Some(3),
            }],
        },
    );

    let error = validate_feature_manifest_for_input_contract(
        &manifest,
        &[FeatureManifestInput {
            symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
            source: "csv".to_string(),
            path: "datasets/sample/aapl_1d.csv".to_string(),
            content_hash: Some("fnv1a64:new".to_string()),
            bar_count: Some(3),
            first_ts_ms: Some(1),
            last_ts_ms: Some(3),
        }],
    )
    .unwrap_err();

    assert!(error.to_string().contains("build inputs"));
    assert!(error.to_string().contains("aapl_1d.csv"));
}

#[test]
fn feature_manifest_input_contract_rejects_same_path_with_changed_content_snapshot() {
    let manifest = build_feature_manifest_with_contract(
        "datasets/features/aapl_sma_2.parquet",
        &[FeatureRecord::new(
            "research-run-1",
            "US:NASDAQ:AAPL:EQUITY",
            10,
            "sma_close_2",
            dec!(101),
            "v1",
        )],
        FeatureBuildContract {
            builder: "feature-build-indicator".to_string(),
            indicator: "sma".to_string(),
            value_column: "close".to_string(),
            period: 2,
            run_id: "research-run-1".to_string(),
            feature_name: "sma_close_2".to_string(),
            version: "v1".to_string(),
            inputs: vec![FeatureManifestInput {
                symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
                source: "csv".to_string(),
                path: "datasets/sample/aapl_1d.csv".to_string(),
                content_hash: Some("fnv1a64:old".to_string()),
                bar_count: Some(3),
                first_ts_ms: Some(1),
                last_ts_ms: Some(3),
            }],
        },
    );

    let error = validate_feature_manifest_for_input_contract(
        &manifest,
        &[FeatureManifestInput {
            symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
            source: "csv".to_string(),
            path: "datasets/sample/aapl_1d.csv".to_string(),
            content_hash: Some("fnv1a64:new".to_string()),
            bar_count: Some(3),
            first_ts_ms: Some(1),
            last_ts_ms: Some(3),
        }],
    )
    .unwrap_err();

    assert!(error.to_string().contains("content_hash"));
    assert!(error.to_string().contains("fnv1a64:old"));
}

#[test]
fn feature_manifest_build_contract_rejects_mismatched_indicator_period_or_value_column() {
    let manifest = build_feature_manifest_with_contract(
        "datasets/features/aapl_sma_2.parquet",
        &[FeatureRecord::new(
            "research-run-1",
            "US:NASDAQ:AAPL:EQUITY",
            10,
            "sma_close_2",
            dec!(101),
            "v1",
        )],
        FeatureBuildContract {
            builder: "feature-build-indicator".to_string(),
            indicator: "sma".to_string(),
            value_column: "close".to_string(),
            period: 2,
            run_id: "research-run-1".to_string(),
            feature_name: "sma_close_2".to_string(),
            version: "v1".to_string(),
            inputs: vec![FeatureManifestInput {
                symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
                source: "csv".to_string(),
                path: "datasets/sample/aapl_1d.csv".to_string(),
                content_hash: None,
                bar_count: None,
                first_ts_ms: None,
                last_ts_ms: None,
            }],
        },
    );

    let error = validate_feature_manifest_for_build_contract(
        &manifest,
        &FeatureBuildContractExpectation {
            indicator: Some("ema".to_string()),
            value_column: Some("open".to_string()),
            period: Some(3),
        },
    )
    .unwrap_err();

    assert!(error.to_string().contains("build contract"));
    assert!(error.to_string().contains("indicator"));
    assert!(error.to_string().contains("ema"));
}

#[test]
fn feature_manifest_validation_accepts_matching_gate_contract() {
    let manifest = build_feature_manifest(
        "datasets/features/research.parquet",
        &[
            FeatureRecord::new(
                "research-run-1",
                "US:NASDAQ:AAPL:EQUITY",
                10,
                "quality_score",
                dec!(0.8),
                "v2",
            ),
            FeatureRecord::new(
                "research-run-1",
                "US:NASDAQ:MSFT:EQUITY",
                10,
                "quality_score",
                dec!(0.7),
                "v2",
            ),
        ],
    );

    validate_feature_manifest_for_gate(
        &manifest,
        "datasets/features/research.parquet",
        "research-run-1",
        &[
            "US:NASDAQ:AAPL:EQUITY".to_string(),
            "US:NASDAQ:MSFT:EQUITY".to_string(),
        ],
        "quality_score",
        Some("v2"),
    )
    .unwrap();
}

#[test]
fn feature_manifest_validation_rejects_parquet_path_mismatch() {
    let manifest = build_feature_manifest(
        "datasets/features/research.parquet",
        &[FeatureRecord::new(
            "research-run-1",
            "US:NASDAQ:AAPL:EQUITY",
            10,
            "quality_score",
            dec!(0.8),
            "v1",
        )],
    );

    let error = validate_feature_manifest_for_gate(
        &manifest,
        "datasets/features/other.parquet",
        "research-run-1",
        &["US:NASDAQ:AAPL:EQUITY".to_string()],
        "quality_score",
        Some("v1"),
    )
    .unwrap_err();

    assert!(error.to_string().contains("parquet_path"));
    assert!(error.to_string().contains("other.parquet"));
}

#[test]
fn feature_manifest_validation_rejects_missing_version() {
    let manifest = build_feature_manifest(
        "datasets/features/research.parquet",
        &[FeatureRecord::new(
            "research-run-1",
            "US:NASDAQ:AAPL:EQUITY",
            10,
            "quality_score",
            dec!(0.8),
            "v1",
        )],
    );

    let error = validate_feature_manifest_for_gate(
        &manifest,
        "datasets/features/research.parquet",
        "research-run-1",
        &["US:NASDAQ:AAPL:EQUITY".to_string()],
        "quality_score",
        Some("v2"),
    )
    .unwrap_err();

    assert!(error.to_string().contains("version v2"));
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

fn temp_json_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "trader-feature-store-{name}-{}.json",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}
