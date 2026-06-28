use api::{AppState, router_with_state};
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use events::TraderEvent;
use feature_store::{
    FeatureBuildContract, FeatureManifestInput, FeatureRecord, build_feature_manifest,
    build_feature_manifest_with_contract, write_feature_manifest, write_feature_records_to_parquet,
};
use replay::{ReplayController, ReplayStatus};
use runtime::RuntimeRunMetadata;
use rust_decimal::Decimal;
use std::path::PathBuf;
use std::sync::Arc;
use storage::Db;
use tokio::sync::{Mutex, Notify};
use tower::ServiceExt;

#[tokio::test]
async fn post_backtest_returns_created() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = router_with_state(AppState::new(db, "configs/backtest/ma_cross.toml".into()));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/backtests")
                .header("content-type", "application/json")
                .body(launch_request_body("backtest"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn post_backtest_publishes_algorithm_events_to_event_bus() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let state = AppState::new(db, "configs/backtest/ma_cross.toml".into());
    let mut receiver = state.event_bus.subscribe();
    let app = router_with_state(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/backtests")
                .header("content-type", "application/json")
                .body(launch_request_body("backtest"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let mut categories = Vec::new();
    while categories.len() < 12 {
        let event = tokio::time::timeout(std::time::Duration::from_secs(2), receiver.recv())
            .await
            .unwrap()
            .unwrap();
        if let TraderEvent::Runtime(runtime_event) = event.payload {
            categories.push(runtime_event.category);
        }
    }
    assert!(categories.contains(&"algorithm.universe.selected".to_string()));
    assert!(categories.contains(&"algorithm.alpha.generated".to_string()));
    assert!(categories.contains(&"algorithm.oms.accepted".to_string()));
}

#[tokio::test]
async fn post_backtest_persists_lifecycle_events() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = router_with_state(AppState::new(db, "configs/backtest/ma_cross.toml".into()));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/backtests")
                .header("content-type", "application/json")
                .body(launch_request_body("backtest"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/runs/sample-ma-cross/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(
        bytes
            .as_ref()
            .windows("backtest.completed".len())
            .any(|window| window == b"backtest.completed")
    );
}

#[tokio::test]
async fn post_backtest_runs_multi_symbol_data_inputs() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let config_path = write_multi_symbol_config("api-backtest-multi-symbol", "backtest");
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = router_with_state(AppState::new(
        db.clone(),
        config_path.to_string_lossy().into_owned(),
    ));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/backtests")
                .header("content-type", "application/json")
                .body(launch_request_body_for_config(&config_path, "backtest"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let summary: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(summary["orders"], 2);
    let positions = db
        .list_positions("api-backtest-multi-symbol")
        .await
        .unwrap();
    assert_eq!(positions.len(), 2);
}

#[tokio::test]
async fn post_backtest_applies_filtered_universe_config() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let config_path = write_filtered_multi_symbol_config("api-backtest-filtered-universe");
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = router_with_state(AppState::new(
        db.clone(),
        config_path.to_string_lossy().into_owned(),
    ));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/backtests")
                .header("content-type", "application/json")
                .body(launch_request_body_for_config(&config_path, "backtest"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let summary: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(summary["orders"], 1);
    let positions = db
        .list_positions("api-backtest-filtered-universe")
        .await
        .unwrap();
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].symbol, "US:NASDAQ:AAPL:EQUITY");
}

#[tokio::test]
async fn post_backtest_applies_ranked_universe_config() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = router_with_state(AppState::new(
        db.clone(),
        "configs/backtest/ranked_universe_ma_cross.toml".to_string(),
    ));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/backtests")
                .header("content-type", "application/json")
                .body(launch_request_body_for_config(
                    "configs/backtest/ranked_universe_ma_cross.toml",
                    "backtest",
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let summary: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(summary["orders"], 1);
    let positions = db
        .list_positions("sample-ranked-universe-ma-cross")
        .await
        .unwrap();
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].symbol, "US:NASDAQ:AAPL:EQUITY");
}

#[tokio::test]
async fn post_backtest_applies_feature_ranked_universe_config() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let config_path = write_feature_ranked_multi_symbol_config("api-backtest-feature-ranked");
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = router_with_state(AppState::new(
        db.clone(),
        config_path.to_string_lossy().into_owned(),
    ));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/backtests")
                .header("content-type", "application/json")
                .body(launch_request_body_for_config(&config_path, "backtest"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let summary: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(summary["orders"], 1);
    let positions = db
        .list_positions("api-backtest-feature-ranked")
        .await
        .unwrap();
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].symbol, "US:NASDAQ:MSFT:EQUITY");
}

#[tokio::test]
async fn post_backtest_rejects_alpha_gate_manifest_source_bars_mismatch() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let configured_bars = temp_path("api-alpha-gate-configured-bars", "csv");
    let research_bars = temp_path("api-alpha-gate-research-bars", "csv");
    let feature_path = temp_path("api-alpha-gate-source-features", "parquet");
    let manifest_path = temp_path("api-alpha-gate-source-manifest", "json");
    let config_path = temp_path("api-alpha-gate-source-config", "toml");
    std::fs::write(
        &configured_bars,
        "ts_ms,open,high,low,close,volume\n1,10,10,10,10,1\n2,11,11,11,11,1\n3,20,20,20,20,1\n",
    )
    .unwrap();
    std::fs::write(
        &research_bars,
        "ts_ms,open,high,low,close,volume\n1,9,9,9,9,1\n2,10,10,10,10,1\n3,19,19,19,19,1\n",
    )
    .unwrap();
    let records = vec![FeatureRecord::new(
        "research-run-1",
        "US:NASDAQ:AAPL:EQUITY",
        1,
        "quality_score",
        Decimal::new(8, 1),
        "v1",
    )];
    write_feature_records_to_parquet(&feature_path, &records).unwrap();
    let manifest = build_feature_manifest_with_contract(
        &feature_path,
        &records,
        FeatureBuildContract {
            builder: "feature-build-indicator".to_string(),
            indicator: "sma".to_string(),
            value_column: "close".to_string(),
            period: 2,
            run_id: "research-run-1".to_string(),
            feature_name: "quality_score".to_string(),
            version: "v1".to_string(),
            inputs: vec![FeatureManifestInput {
                symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
                source: "csv".to_string(),
                path: toml_path(&research_bars),
                content_hash: None,
                bar_count: None,
                first_ts_ms: None,
                last_ts_ms: None,
            }],
        },
    );
    write_feature_manifest(&manifest_path, &manifest).unwrap();
    std::fs::write(
        &config_path,
        format!(
            r#"
            [runtime]
            mode = "backtest"
            run_id = "api-alpha-gate-manifest-source"

            [database]
            url = "sqlite::memory:"

            [data]
            source = "csv"
            path = "{}"

            [strategy]
            name = "moving_average_cross"
            symbols = ["US:NASDAQ:AAPL:EQUITY"]
            fast_window = 2
            slow_window = 3

            [strategy.alpha_gate]
            source = "parquet"
            path = "{}"
            manifest_path = "{}"
            run_id = "research-run-1"
            feature_name = "quality_score"
            version = "v1"

            [portfolio]
            initial_cash = "100000"
            base_currency = "USD"
            order_qty = "1"
            max_abs_qty = "100"

            [risk]
            max_order_notional = "1000000"
            min_cash_after_order = "0"
            max_exposure = "1000000"
            max_drawdown = "1"
            max_leverage = "10"
            max_margin_used = "0"
            trading_halted = false

            [broker]
            kind = "simulated"
            mode = "paper"

            [paper]
            account_id = "paper"
            slippage_bps = "0"
            fee_bps = "0"

            [live]
            enabled = false
            "#,
            toml_path(&configured_bars),
            toml_path(&feature_path),
            toml_path(&manifest_path)
        ),
    )
    .unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = router_with_state(AppState::new(
        db,
        config_path.to_string_lossy().into_owned(),
    ));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/backtests")
                .header("content-type", "application/json")
                .body(launch_request_body_for_config(&config_path, "backtest"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("build inputs"));

    std::fs::remove_file(configured_bars).unwrap();
    std::fs::remove_file(research_bars).unwrap();
    std::fs::remove_file(feature_path).unwrap();
    std::fs::remove_file(manifest_path).unwrap();
    std::fs::remove_file(config_path).unwrap();
}

#[tokio::test]
async fn post_backtest_rejects_alpha_gate_manifest_build_contract_mismatch() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let bars = temp_path("api-alpha-gate-build-bars", "csv");
    let feature_path = temp_path("api-alpha-gate-build-features", "parquet");
    let manifest_path = temp_path("api-alpha-gate-build-manifest", "json");
    let config_path = temp_path("api-alpha-gate-build-config", "toml");
    std::fs::write(
        &bars,
        "ts_ms,open,high,low,close,volume\n1,10,10,10,10,1\n2,11,11,11,11,1\n3,20,20,20,20,1\n",
    )
    .unwrap();
    let records = vec![FeatureRecord::new(
        "research-run-1",
        "US:NASDAQ:AAPL:EQUITY",
        2,
        "quality_score",
        Decimal::new(8, 1),
        "v1",
    )];
    write_feature_records_to_parquet(&feature_path, &records).unwrap();
    let manifest = build_feature_manifest_with_contract(
        &feature_path,
        &records,
        FeatureBuildContract {
            builder: "feature-build-indicator".to_string(),
            indicator: "sma".to_string(),
            value_column: "close".to_string(),
            period: 2,
            run_id: "research-run-1".to_string(),
            feature_name: "quality_score".to_string(),
            version: "v1".to_string(),
            inputs: vec![FeatureManifestInput {
                symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
                source: "csv".to_string(),
                path: toml_path(&bars),
                content_hash: None,
                bar_count: None,
                first_ts_ms: None,
                last_ts_ms: None,
            }],
        },
    );
    write_feature_manifest(&manifest_path, &manifest).unwrap();
    std::fs::write(
        &config_path,
        format!(
            r#"
            [runtime]
            mode = "backtest"
            run_id = "api-alpha-gate-manifest-build"

            [database]
            url = "sqlite::memory:"

            [data]
            source = "csv"
            path = "{}"

            [strategy]
            name = "moving_average_cross"
            symbols = ["US:NASDAQ:AAPL:EQUITY"]
            fast_window = 2
            slow_window = 3

            [strategy.alpha_gate]
            source = "parquet"
            path = "{}"
            manifest_path = "{}"
            run_id = "research-run-1"
            feature_name = "quality_score"
            version = "v1"
            build_indicator = "ema"
            build_period = 2
            build_value_column = "close"

            [portfolio]
            initial_cash = "100000"
            base_currency = "USD"
            order_qty = "1"
            max_abs_qty = "100"

            [risk]
            max_order_notional = "1000000"
            min_cash_after_order = "0"
            max_exposure = "1000000"
            max_drawdown = "1"
            max_leverage = "10"
            max_margin_used = "0"
            trading_halted = false

            [broker]
            kind = "simulated"
            mode = "paper"

            [paper]
            account_id = "paper"
            slippage_bps = "0"
            fee_bps = "0"

            [live]
            enabled = false
            "#,
            toml_path(&bars),
            toml_path(&feature_path),
            toml_path(&manifest_path)
        ),
    )
    .unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = router_with_state(AppState::new(
        db,
        config_path.to_string_lossy().into_owned(),
    ));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/backtests")
                .header("content-type", "application/json")
                .body(launch_request_body_for_config(&config_path, "backtest"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("build contract"));
    assert!(body.contains("indicator"));

    std::fs::remove_file(bars).unwrap();
    std::fs::remove_file(feature_path).unwrap();
    std::fs::remove_file(manifest_path).unwrap();
    std::fs::remove_file(config_path).unwrap();
}

#[tokio::test]
async fn post_backtest_rejects_alpha_gate_manifest_when_source_bars_content_changes() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let bars = temp_path("api-alpha-gate-content-bars", "csv");
    let feature_path = temp_path("api-alpha-gate-content-features", "parquet");
    let manifest_path = temp_path("api-alpha-gate-content-manifest", "json");
    let config_path = temp_path("api-alpha-gate-content-config", "toml");
    std::fs::write(
        &bars,
        "ts_ms,open,high,low,close,volume\n1,10,10,10,10,1\n2,12,12,12,12,1\n3,14,14,14,14,1\n",
    )
    .unwrap();

    let records = vec![FeatureRecord::new(
        "research-run-1",
        "US:NASDAQ:AAPL:EQUITY",
        2,
        "quality_score",
        Decimal::new(8, 1),
        "v1",
    )];
    write_feature_records_to_parquet(&feature_path, &records).unwrap();
    let manifest = build_feature_manifest_with_contract(
        &feature_path,
        &records,
        FeatureBuildContract {
            builder: "feature-build-indicator".to_string(),
            indicator: "sma".to_string(),
            value_column: "close".to_string(),
            period: 2,
            run_id: "research-run-1".to_string(),
            feature_name: "quality_score".to_string(),
            version: "v1".to_string(),
            inputs: vec![FeatureManifestInput {
                symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
                source: "csv".to_string(),
                path: toml_path(&bars),
                content_hash: Some(stable_test_file_content_hash(&bars)),
                bar_count: Some(3),
                first_ts_ms: Some(1),
                last_ts_ms: Some(3),
            }],
        },
    );
    write_feature_manifest(&manifest_path, &manifest).unwrap();

    std::fs::write(
        &bars,
        "ts_ms,open,high,low,close,volume\n1,100,100,100,100,1\n2,120,120,120,120,1\n3,140,140,140,140,1\n",
    )
    .unwrap();
    std::fs::write(
        &config_path,
        format!(
            r#"
            [runtime]
            mode = "backtest"
            run_id = "api-alpha-gate-manifest-content"

            [database]
            url = "sqlite::memory:"

            [data]
            source = "csv"
            path = "{}"

            [strategy]
            name = "moving_average_cross"
            symbols = ["US:NASDAQ:AAPL:EQUITY"]
            fast_window = 2
            slow_window = 3

            [strategy.alpha_gate]
            source = "parquet"
            path = "{}"
            manifest_path = "{}"
            run_id = "research-run-1"
            feature_name = "quality_score"
            version = "v1"
            build_indicator = "sma"
            build_period = 2
            build_value_column = "close"

            [portfolio]
            initial_cash = "100000"
            base_currency = "USD"
            order_qty = "1"
            max_abs_qty = "100"

            [risk]
            max_order_notional = "1000000"
            min_cash_after_order = "0"
            max_exposure = "1000000"
            max_drawdown = "1"
            max_leverage = "10"
            max_margin_used = "0"
            trading_halted = false

            [broker]
            kind = "simulated"
            mode = "paper"

            [paper]
            account_id = "paper"
            slippage_bps = "0"
            fee_bps = "0"

            [live]
            enabled = false
            "#,
            toml_path(&bars),
            toml_path(&feature_path),
            toml_path(&manifest_path)
        ),
    )
    .unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = router_with_state(AppState::new(
        db,
        config_path.to_string_lossy().into_owned(),
    ));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/backtests")
                .header("content-type", "application/json")
                .body(launch_request_body_for_config(&config_path, "backtest"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("content_hash"));

    std::fs::remove_file(bars).unwrap();
    std::fs::remove_file(feature_path).unwrap();
    std::fs::remove_file(manifest_path).unwrap();
    std::fs::remove_file(config_path).unwrap();
}

#[tokio::test]
async fn post_backtest_applies_weighted_alpha_components() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let config_path = write_weighted_alpha_config("api-backtest-weighted-alpha");
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = router_with_state(AppState::new(
        db.clone(),
        config_path.to_string_lossy().into_owned(),
    ));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/backtests")
                .header("content-type", "application/json")
                .body(launch_request_body_for_config(&config_path, "backtest"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let events = db
        .list_events_by_source("api-backtest-weighted-alpha")
        .await
        .unwrap();
    let alpha_event = events
        .iter()
        .find(|event| event.category == "algorithm.alpha.generated")
        .unwrap();
    assert!(alpha_event.payload_json.contains("\"confidence\":0.4"));
}

#[tokio::test]
async fn post_backtest_applies_net_signal_alpha_components() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let config_path = write_net_signal_alpha_config("api-backtest-net-signal-alpha");
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = router_with_state(AppState::new(
        db.clone(),
        config_path.to_string_lossy().into_owned(),
    ));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/backtests")
                .header("content-type", "application/json")
                .body(launch_request_body_for_config(&config_path, "backtest"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let events = db
        .list_events_by_source("api-backtest-net-signal-alpha")
        .await
        .unwrap();
    let alpha_event = events
        .iter()
        .find(|event| event.category == "algorithm.alpha.generated")
        .unwrap();
    let payload: serde_json::Value = serde_json::from_str(&alpha_event.payload_json).unwrap();
    assert_eq!(payload["side"], "Buy");
    let confidence = payload["confidence"].as_f64().unwrap();
    assert!((confidence - 0.6).abs() < 1e-9);
}

#[tokio::test]
async fn post_backtest_applies_majority_vote_alpha_components() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let config_path = write_majority_vote_alpha_config("api-backtest-majority-vote-alpha");
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = router_with_state(AppState::new(
        db.clone(),
        config_path.to_string_lossy().into_owned(),
    ));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/backtests")
                .header("content-type", "application/json")
                .body(launch_request_body_for_config(&config_path, "backtest"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let events = db
        .list_events_by_source("api-backtest-majority-vote-alpha")
        .await
        .unwrap();
    let alpha_event = events
        .iter()
        .find(|event| event.category == "algorithm.alpha.generated")
        .unwrap();
    let payload: serde_json::Value = serde_json::from_str(&alpha_event.payload_json).unwrap();
    assert_eq!(payload["side"], "Buy");
    let confidence = payload["confidence"].as_f64().unwrap();
    assert!((confidence - 0.3).abs() < 1e-9);
}

#[tokio::test]
async fn post_backtest_applies_category_majority_alpha_components() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let config_path = write_category_majority_alpha_config("api-backtest-category-majority-alpha");
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = router_with_state(AppState::new(
        db.clone(),
        config_path.to_string_lossy().into_owned(),
    ));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/backtests")
                .header("content-type", "application/json")
                .body(launch_request_body_for_config(&config_path, "backtest"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let events = db
        .list_events_by_source("api-backtest-category-majority-alpha")
        .await
        .unwrap();
    let alpha_event = events
        .iter()
        .find(|event| event.category == "algorithm.alpha.generated")
        .unwrap();
    let payload: serde_json::Value = serde_json::from_str(&alpha_event.payload_json).unwrap();
    assert_eq!(payload["side"], "Buy");
    let confidence = payload["confidence"].as_f64().unwrap();
    assert!((confidence - 0.6).abs() < 1e-9);
}

#[tokio::test]
async fn post_backtest_applies_ema_cross_alpha() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = router_with_state(AppState::new(
        db.clone(),
        "configs/backtest/ema_cross.toml".to_string(),
    ));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/backtests")
                .header("content-type", "application/json")
                .body(launch_request_body_for_config(
                    "configs/backtest/ema_cross.toml",
                    "backtest",
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let events = db.list_events_by_source("sample-ema-cross").await.unwrap();
    let alpha_payloads = events
        .iter()
        .filter(|event| event.category == "algorithm.alpha.generated")
        .map(|event| serde_json::from_str::<serde_json::Value>(&event.payload_json).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(alpha_payloads.len(), 2);
    assert_eq!(alpha_payloads[0]["side"], "Sell");
    assert_eq!(alpha_payloads[1]["side"], "Buy");
    for payload in alpha_payloads {
        let confidence = payload["confidence"].as_f64().unwrap();
        assert!((confidence - 0.8).abs() < 1e-9);
    }
}

#[tokio::test]
async fn post_backtest_applies_price_momentum_alpha() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = router_with_state(AppState::new(
        db.clone(),
        "configs/backtest/price_momentum.toml".to_string(),
    ));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/backtests")
                .header("content-type", "application/json")
                .body(launch_request_body_for_config(
                    "configs/backtest/price_momentum.toml",
                    "backtest",
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let events = db
        .list_events_by_source("sample-price-momentum")
        .await
        .unwrap();
    let alpha_event = events
        .iter()
        .find(|event| event.category == "algorithm.alpha.generated")
        .unwrap();
    let payload: serde_json::Value = serde_json::from_str(&alpha_event.payload_json).unwrap();
    assert_eq!(payload["side"], "Buy");
    let confidence = payload["confidence"].as_f64().unwrap();
    assert!((confidence - 0.8).abs() < 1e-9);
}

#[tokio::test]
async fn post_backtest_applies_price_channel_breakout_alpha() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = router_with_state(AppState::new(
        db.clone(),
        "configs/backtest/price_channel_breakout.toml".to_string(),
    ));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/backtests")
                .header("content-type", "application/json")
                .body(launch_request_body_for_config(
                    "configs/backtest/price_channel_breakout.toml",
                    "backtest",
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let events = db
        .list_events_by_source("sample-price-channel-breakout")
        .await
        .unwrap();
    let alpha_event = events
        .iter()
        .find(|event| event.category == "algorithm.alpha.generated")
        .unwrap();
    let payload: serde_json::Value = serde_json::from_str(&alpha_event.payload_json).unwrap();
    assert_eq!(payload["side"], "Buy");
    let confidence = payload["confidence"].as_f64().unwrap();
    assert!((confidence - 0.8).abs() < 1e-9);
}

#[tokio::test]
async fn post_backtest_applies_price_channel_reversion_alpha() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = router_with_state(AppState::new(
        db.clone(),
        "configs/backtest/price_channel_reversion.toml".to_string(),
    ));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/backtests")
                .header("content-type", "application/json")
                .body(launch_request_body_for_config(
                    "configs/backtest/price_channel_reversion.toml",
                    "backtest",
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let events = db
        .list_events_by_source("sample-price-channel-reversion")
        .await
        .unwrap();
    let alpha_event = events
        .iter()
        .find(|event| event.category == "algorithm.alpha.generated")
        .unwrap();
    let payload: serde_json::Value = serde_json::from_str(&alpha_event.payload_json).unwrap();
    assert_eq!(payload["side"], "Buy");
    let confidence = payload["confidence"].as_f64().unwrap();
    assert!((confidence - 0.8).abs() < 1e-9);
}

#[tokio::test]
async fn post_backtest_applies_rsi_reversion_alpha() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = router_with_state(AppState::new(
        db.clone(),
        "configs/backtest/rsi_reversion.toml".to_string(),
    ));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/backtests")
                .header("content-type", "application/json")
                .body(launch_request_body_for_config(
                    "configs/backtest/rsi_reversion.toml",
                    "backtest",
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let events = db
        .list_events_by_source("sample-rsi-reversion")
        .await
        .unwrap();
    let alpha_event = events
        .iter()
        .find(|event| event.category == "algorithm.alpha.generated")
        .unwrap();
    let payload: serde_json::Value = serde_json::from_str(&alpha_event.payload_json).unwrap();
    assert_eq!(payload["side"], "Buy");
    let confidence = payload["confidence"].as_f64().unwrap();
    assert!((confidence - 0.8).abs() < 1e-9);
}

#[tokio::test]
async fn post_backtest_applies_rsi_feature_gate_config() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = router_with_state(AppState::new(
        db.clone(),
        "configs/backtest/rsi_feature_gate.toml".to_string(),
    ));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/backtests")
                .header("content-type", "application/json")
                .body(launch_request_body_for_config(
                    "configs/backtest/rsi_feature_gate.toml",
                    "backtest",
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let summary: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(summary["signals"], 1);
    assert_eq!(summary["orders"], 1);
}

#[tokio::test]
async fn post_paper_run_runs_multi_symbol_data_inputs() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let config_path = write_multi_symbol_config("api-paper-multi-symbol", "paper");
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = router_with_state(AppState::new(
        db.clone(),
        config_path.to_string_lossy().into_owned(),
    ));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/paper-runs")
                .header("content-type", "application/json")
                .body(launch_request_body_for_config(&config_path, "paper"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    wait_for_status(app, "api-paper-multi-symbol", "completed").await;
    let orders = db.list_orders("api-paper-multi-symbol").await.unwrap();
    assert_eq!(orders.len(), 2);
    let positions = db.list_positions("api-paper-multi-symbol").await.unwrap();
    assert_eq!(positions.len(), 2);
}

#[tokio::test]
async fn post_replay_returns_created_and_persists_events() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = router_with_state(AppState::new(db, "configs/backtest/ma_cross.toml".into()));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/replays")
                .header("content-type", "application/json")
                .body(launch_request_body("replay"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(
        bytes
            .as_ref()
            .windows("\"bars\"".len())
            .any(|window| window == b"\"bars\"")
    );

    for uri in ["/api/v1/events", "/api/v1/runs/sample-ma-cross/events"] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert!(
            bytes
                .as_ref()
                .windows("replay.completed".len())
                .any(|window| window == b"replay.completed")
        );
    }
}

#[tokio::test]
async fn event_routes_return_structured_payload_objects() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = router_with_state(AppState::new(db, "configs/backtest/ma_cross.toml".into()));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/backtests")
                .header("content-type", "application/json")
                .body(launch_request_body("backtest"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/runs/sample-ma-cross/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let events: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let first = events.as_array().unwrap().first().unwrap();
    assert!(first.get("payload_json").is_none());
    assert!(first.get("payload").unwrap().is_object());
}

#[tokio::test]
async fn run_routes_return_structured_config_objects() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = router_with_state(AppState::new(db, "configs/backtest/ma_cross.toml".into()));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/backtests")
                .header("content-type", "application/json")
                .body(launch_request_body("backtest"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/runs/sample-ma-cross")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let run: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(run.get("config_json").is_none());
    assert!(run.get("config").unwrap().is_object());
    assert_eq!(run["config"]["runtime"]["run_id"], "sample-ma-cross");
}

#[tokio::test]
async fn post_replay_publishes_market_events_to_event_bus() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let state = AppState::new(db, "configs/backtest/ma_cross.toml".into());
    let mut receiver = state.event_bus.subscribe();
    let app = router_with_state(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/replays")
                .header("content-type", "application/json")
                .body(launch_request_body("replay"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let mut categories = Vec::new();
    while categories.len() < 3 {
        let event = tokio::time::timeout(std::time::Duration::from_secs(2), receiver.recv())
            .await
            .unwrap()
            .unwrap();
        if let TraderEvent::Runtime(runtime_event) = event.payload {
            categories.push(runtime_event.category);
        }
    }
    assert_eq!(categories, vec!["market.bar", "market.bar", "market.bar"]);
}

#[tokio::test]
async fn replay_control_routes_update_replay_state() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let state = AppState::new(db, "configs/backtest/ma_cross.toml".into());
    let released = spawn_active_replay_runtime(&state, "sample-ma-cross").await;
    register_replay_controller(&state, "sample-ma-cross").await;
    let app = router_with_state(state.clone());

    for (uri, expected_status, expected_fragment) in [
        (
            "/api/v1/replay/sample-ma-cross/pause",
            "paused",
            "\"status\":\"paused\"",
        ),
        (
            "/api/v1/replay/sample-ma-cross/seek/2",
            "paused",
            "\"offset\":2",
        ),
        (
            "/api/v1/replay/sample-ma-cross/speed/25",
            "paused",
            "\"speed\":25",
        ),
        (
            "/api/v1/replay/sample-ma-cross/resume",
            "running",
            "\"status\":\"running\"",
        ),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert!(
            bytes
                .as_ref()
                .windows(expected_fragment.len())
                .any(|window| window == expected_fragment.as_bytes()),
            "missing {expected_fragment} in {uri}"
        );
        assert!(
            bytes
                .as_ref()
                .windows(expected_status.len())
                .any(|window| window == expected_status.as_bytes())
        );
    }

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/runs/sample-ma-cross/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(
        bytes
            .as_ref()
            .windows("replay.speed".len())
            .any(|window| window == b"replay.speed")
    );
    released.notify_one();
    state.runtime_manager.wait_for_idle("sample-ma-cross").await;
}

#[tokio::test]
async fn replay_control_routes_reject_unknown_run() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let state = AppState::new(db, "configs/backtest/ma_cross.toml".into());
    register_replay_controller(&state, "sample-ma-cross").await;
    let app = router_with_state(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/replay/missing-run/pause")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn replay_control_routes_reject_stale_controller_for_inactive_run() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let state = AppState::new(db, "configs/backtest/ma_cross.toml".into());
    let released = spawn_active_replay_runtime(&state, "run-active").await;
    register_replay_controller(&state, "run-active").await;
    let stale_controller = register_replay_controller(&state, "run-stale").await;
    let app = router_with_state(state.clone());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/replay/run-stale/pause")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("inactive replay run"), "{body}");
    assert_eq!(
        stale_controller.lock().await.state().status,
        ReplayStatus::Running
    );

    released.notify_one();
    state.runtime_manager.wait_for_idle("run-active").await;
}

#[tokio::test]
async fn post_paper_run_returns_accepted_run_start() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = router_with_state(AppState::new(db, "configs/backtest/ma_cross.toml".into()));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/paper-runs")
                .header("content-type", "application/json")
                .body(launch_request_body("paper"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(
        bytes
            .as_ref()
            .windows("\"status\":\"running\"".len())
            .any(|window| window == b"\"status\":\"running\"")
    );
}

#[tokio::test]
async fn post_paper_run_publishes_algorithm_events_to_event_bus() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let state = AppState::new(db, "configs/backtest/ma_cross.toml".into());
    let mut receiver = state.event_bus.subscribe();
    let app = router_with_state(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/paper-runs")
                .header("content-type", "application/json")
                .body(launch_request_body("paper"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let mut categories = Vec::new();
    while categories.len() < 12 {
        let event = tokio::time::timeout(std::time::Duration::from_secs(2), receiver.recv())
            .await
            .unwrap()
            .unwrap();
        if let TraderEvent::Runtime(runtime_event) = event.payload {
            categories.push(runtime_event.category);
        }
    }
    assert!(categories.contains(&"algorithm.universe.selected".to_string()));
    assert!(categories.contains(&"algorithm.alpha.generated".to_string()));
    assert!(categories.contains(&"algorithm.oms.accepted".to_string()));
}

#[tokio::test]
async fn post_paper_run_requires_credentials_for_enabled_binance_submit() {
    std::env::set_current_dir(workspace_root()).unwrap();
    unsafe {
        std::env::remove_var("BINANCE_TESTNET_API_KEY");
        std::env::remove_var("BINANCE_TESTNET_SECRET_KEY");
    }
    let config_path = temp_config_with_enabled_broker_submit();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = router_with_state(AppState::new(
        db,
        config_path.to_string_lossy().into_owned(),
    ));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/paper-runs")
                .header("content-type", "application/json")
                .body(launch_request_body_for_config(&config_path, "paper"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("BINANCE_TESTNET_API_KEY"));

    std::fs::remove_file(config_path).unwrap();
}

#[tokio::test]
async fn post_paper_run_populates_query_routes() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = router_with_state(AppState::new(db, "configs/backtest/ma_cross.toml".into()));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/paper-runs")
                .header("content-type", "application/json")
                .body(launch_request_body("paper"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    wait_for_status(app.clone(), "sample-ma-cross", "completed").await;

    for uri in [
        "/api/v1/fills?run_id=sample-ma-cross",
        "/api/v1/account-balances?run_id=sample-ma-cross",
        "/api/v1/portfolio/snapshots?run_id=sample-ma-cross",
        "/api/v1/cash/snapshots?run_id=sample-ma-cross",
        "/api/v1/positions/snapshots?run_id=sample-ma-cross",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_ne!(bytes.as_ref(), b"[]");
    }

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/metrics?run_id=sample-ma-cross")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(
        bytes
            .as_ref()
            .windows("total_return".len())
            .any(|window| window == b"total_return")
    );

    for uri in ["/api/v1/runs", "/api/v1/runs/sample-ma-cross"] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_ne!(bytes.as_ref(), b"[]");
    }

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/configs/sample-ma-cross")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let config: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(config["name"], "sample-ma-cross");
    assert_eq!(config["config_type"], "RUN");
    assert_eq!(config["format"], "JSON");
    assert!(
        config["content"]
            .as_str()
            .unwrap()
            .contains("\"run_id\": \"sample-ma-cross\"")
    );
    assert!(config["checksum"].as_str().unwrap().starts_with("fnv1a64:"));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/configs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let configs: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let configs = configs.as_array().unwrap();
    assert!(configs.iter().any(|config| {
        config["name"] == "sample-ma-cross"
            && config["config_type"] == "RUN"
            && config["format"] == "JSON"
            && config["content"]
                .as_str()
                .unwrap()
                .contains("\"mode\": \"paper\"")
            && config["checksum"].as_str().unwrap().starts_with("fnv1a64:")
    }));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/runs/sample-ma-cross/system-logs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let logs: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let logs = logs.as_array().unwrap();
    assert!(
        logs.iter()
            .any(|log| log["message"] == "paper run accepted")
    );
    let completed = logs
        .iter()
        .find(|log| log["message"] == "paper run completed" && log["target"] == "api.run")
        .unwrap();
    assert_eq!(completed["level"], "INFO");
    assert_eq!(completed["target"], "api.run");
    assert_eq!(completed["fields"]["orders"], 1);
}

#[tokio::test]
async fn completed_paper_run_status_is_preserved_after_late_cancel() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = router_with_state(AppState::new(db, "configs/backtest/ma_cross.toml".into()));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/paper-runs")
                .header("content-type", "application/json")
                .body(launch_request_body("paper"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    wait_for_status(app.clone(), "sample-ma-cross", "completed").await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/runs/sample-ma-cross/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(
        bytes
            .as_ref()
            .windows("completed".len())
            .any(|window| window == b"completed")
    );

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/runs/sample-ma-cross/cancel")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/runs/sample-ma-cross/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(
        bytes
            .as_ref()
            .windows("completed".len())
            .any(|window| window == b"completed")
    );
}

#[tokio::test]
async fn cancel_does_not_overwrite_terminal_run_status_while_manager_is_still_active() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    db.start_strategy_run(storage::StrategyRunStartCommand {
        run_id: "terminal-run".to_string(),
        name: "sample".to_string(),
        mode: "paper".to_string(),
        started_at_ms: 1,
        config: serde_json::json!({}),
    })
    .await
    .unwrap();
    db.update_strategy_run_status("terminal-run", "completed", Some(2), None)
        .await
        .unwrap();

    let state = AppState::new(db, "configs/backtest/ma_cross.toml".into());
    state
        .runtime_manager
        .spawn("terminal-run".to_string(), |cancel| async move {
            while !cancel.is_cancelled() {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    let app = router_with_state(state.clone());

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/runs/terminal-run/cancel")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(
        bytes
            .as_ref()
            .windows("completed".len())
            .any(|window| window == b"completed")
    );

    state.runtime_manager.cancel("terminal-run").await;
    state.runtime_manager.wait_for_idle("terminal-run").await;
}

#[tokio::test]
async fn failed_paper_run_records_failed_status_and_error() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = router_with_state(AppState::new(
        db,
        "configs/backtest/missing-bars.toml".into(),
    ));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/paper-runs")
                .header("content-type", "application/json")
                .body(launch_request_body_for_config(
                    "configs/backtest/missing-bars.toml",
                    "paper",
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    wait_for_status(app.clone(), "sample-missing-bars", "failed").await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/runs/sample-missing-bars/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(
        bytes
            .as_ref()
            .windows("failed".len())
            .any(|window| window == b"failed")
    );
    assert!(
        bytes
            .as_ref()
            .windows("\"error\":null".len())
            .all(|window| window != b"\"error\":null")
    );
}

#[tokio::test]
async fn active_paper_run_can_be_cancelled() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = router_with_state(AppState::new(db, "configs/backtest/slow-paper.toml".into()));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/paper-runs")
                .header("content-type", "application/json")
                .body(launch_request_body_for_config(
                    "configs/backtest/slow-paper.toml",
                    "paper",
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/runs/sample-slow-paper/cancel")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    assert_status(app.clone(), "sample-slow-paper", "cancelled").await;
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .unwrap()
        .to_path_buf()
}

fn temp_config_with_enabled_broker_submit() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "trader-api-order-submit-gate-{}.toml",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let input = std::fs::read_to_string("configs/paper/binance_testnet.toml").unwrap();
    let content = input.replace(
        "order_submit_enabled = false",
        "order_submit_enabled = true",
    );
    std::fs::write(&path, content).unwrap();
    path
}

fn launch_request_body(mode: &str) -> Body {
    launch_request_body_for_config("configs/backtest/ma_cross.toml", mode)
}

fn launch_request_body_for_config(path: impl AsRef<std::path::Path>, mode: &str) -> Body {
    let config_toml = std::fs::read_to_string(path).unwrap();
    Body::from(serde_json::json!({ "config_toml": config_toml, "mode": mode }).to_string())
}

async fn register_replay_controller(
    state: &AppState,
    run_id: &str,
) -> Arc<Mutex<ReplayController>> {
    let controller = Arc::new(Mutex::new(ReplayController::new(run_id.to_string(), 1)));
    state
        .replay_controllers
        .lock()
        .await
        .insert(run_id.to_string(), controller.clone());
    controller
}

async fn spawn_active_replay_runtime(state: &AppState, run_id: &str) -> Arc<Notify> {
    let released = Arc::new(Notify::new());
    let released_for_task = released.clone();
    state
        .runtime_manager
        .spawn_with_metadata(
            run_id.to_string(),
            RuntimeRunMetadata {
                mode: Some("replay".to_string()),
            },
            move |_cancel| async move {
                released_for_task.notified().await;
            },
        )
        .await
        .unwrap();
    released
}

fn write_multi_symbol_config(run_id: &str, runtime_mode: &str) -> PathBuf {
    let aapl_path = temp_path("api-aapl", "csv");
    let msft_path = temp_path("api-msft", "csv");
    std::fs::write(
        &aapl_path,
        "ts_ms,open,high,low,close,volume\n1,10,10,10,10,1\n2,11,11,11,11,1\n3,20,20,20,20,1\n",
    )
    .unwrap();
    std::fs::write(
        &msft_path,
        "ts_ms,open,high,low,close,volume\n1,30,30,30,30,1\n2,31,31,31,31,1\n3,40,40,40,40,1\n",
    )
    .unwrap();

    let config_path = temp_path("api-config", "toml");
    std::fs::write(
        &config_path,
        format!(
            r#"
            [runtime]
            mode = "{runtime_mode}"
            run_id = "{run_id}"

            [database]
            url = "sqlite::memory:"

            [data]
            [[data.inputs]]
            symbol = "US:NASDAQ:AAPL:EQUITY"
            source = "csv"
            path = "{}"

            [[data.inputs]]
            symbol = "US:NASDAQ:MSFT:EQUITY"
            source = "csv"
            path = "{}"

            [strategy]
            name = "moving_average_cross"
            symbols = ["US:NASDAQ:AAPL:EQUITY", "US:NASDAQ:MSFT:EQUITY"]
            fast_window = 2
            slow_window = 3

            [portfolio]
            initial_cash = "100000"
            base_currency = "USD"
            order_qty = "1"
            max_abs_qty = "100"

            [risk]
            max_order_notional = "1000000"
            min_cash_after_order = "0"
            max_exposure = "1000000"
            max_drawdown = "1"
            max_leverage = "10"
            max_margin_used = "0"
            trading_halted = false

            [broker]
            kind = "simulated"
            mode = "paper"

            [paper]
            account_id = "paper"
            slippage_bps = "0"
            fee_bps = "0"

            [live]
            enabled = false
            "#,
            toml_path(&aapl_path),
            toml_path(&msft_path)
        ),
    )
    .unwrap();
    config_path
}

fn write_filtered_multi_symbol_config(run_id: &str) -> PathBuf {
    let aapl_path = temp_path("api-filtered-aapl", "csv");
    let msft_path = temp_path("api-filtered-msft", "csv");
    std::fs::write(
        &aapl_path,
        "ts_ms,open,high,low,close,volume\n1,10,10,10,10,1\n2,11,11,11,11,1\n3,20,20,20,20,1\n",
    )
    .unwrap();
    std::fs::write(
        &msft_path,
        "ts_ms,open,high,low,close,volume\n1,30,30,30,30,1\n2,31,31,31,31,1\n3,40,40,40,40,1\n",
    )
    .unwrap();

    let config_path = temp_path("api-filtered-config", "toml");
    std::fs::write(
        &config_path,
        format!(
            r#"
            [runtime]
            mode = "backtest"
            run_id = "{run_id}"

            [database]
            url = "sqlite::memory:"

            [data]
            [[data.inputs]]
            symbol = "US:NASDAQ:AAPL:EQUITY"
            source = "csv"
            path = "{}"

            [[data.inputs]]
            symbol = "US:NASDAQ:MSFT:EQUITY"
            source = "csv"
            path = "{}"

            [strategy]
            name = "moving_average_cross"
            universe = "filtered"
            symbols = ["US:NASDAQ:AAPL:EQUITY", "US:NASDAQ:MSFT:EQUITY"]
            fast_window = 2
            slow_window = 3

            [strategy.universe_filter]
            exclude_symbols = ["US:NASDAQ:MSFT:EQUITY"]

            [portfolio]
            initial_cash = "100000"
            base_currency = "USD"
            order_qty = "1"
            max_abs_qty = "100"

            [risk]
            max_order_notional = "1000000"
            min_cash_after_order = "0"
            max_exposure = "1000000"
            max_drawdown = "1"
            max_leverage = "10"
            max_margin_used = "0"
            trading_halted = false

            [broker]
            kind = "simulated"
            mode = "paper"

            [paper]
            account_id = "paper"
            slippage_bps = "0"
            fee_bps = "0"

            [live]
            enabled = false
            "#,
            toml_path(&aapl_path),
            toml_path(&msft_path)
        ),
    )
    .unwrap();
    config_path
}

fn write_feature_ranked_multi_symbol_config(run_id: &str) -> PathBuf {
    let aapl_path = temp_path("api-feature-ranked-aapl", "csv");
    let msft_path = temp_path("api-feature-ranked-msft", "csv");
    let feature_path = temp_path("api-feature-ranked", "parquet");
    let manifest_path = temp_path("api-feature-ranked", "json");
    std::fs::write(
        &aapl_path,
        "ts_ms,open,high,low,close,volume\n1,10,10,10,10,1\n2,11,11,11,11,1\n3,20,20,20,20,1\n",
    )
    .unwrap();
    std::fs::write(
        &msft_path,
        "ts_ms,open,high,low,close,volume\n1,30,30,30,30,1\n2,31,31,31,31,1\n3,40,40,40,40,1\n",
    )
    .unwrap();
    let records = vec![
        FeatureRecord::new(
            "research-rank",
            "US:NASDAQ:AAPL:EQUITY",
            1,
            "quality_score",
            Decimal::new(1, 1),
            "v1",
        ),
        FeatureRecord::new(
            "research-rank",
            "US:NASDAQ:MSFT:EQUITY",
            1,
            "quality_score",
            Decimal::new(9, 1),
            "v1",
        ),
    ];
    write_feature_records_to_parquet(&feature_path, &records).unwrap();
    let manifest = build_feature_manifest(&feature_path, &records);
    write_feature_manifest(&manifest_path, &manifest).unwrap();

    let config_path = temp_path("api-feature-ranked-config", "toml");
    std::fs::write(
        &config_path,
        format!(
            r#"
            [runtime]
            mode = "backtest"
            run_id = "{run_id}"

            [database]
            url = "sqlite::memory:"

            [data]
            [[data.inputs]]
            symbol = "US:NASDAQ:AAPL:EQUITY"
            source = "csv"
            path = "{}"

            [[data.inputs]]
            symbol = "US:NASDAQ:MSFT:EQUITY"
            source = "csv"
            path = "{}"

            [strategy]
            name = "moving_average_cross"
            universe = "feature_ranked"
            symbols = ["US:NASDAQ:AAPL:EQUITY", "US:NASDAQ:MSFT:EQUITY"]
            fast_window = 2
            slow_window = 3

            [strategy.universe_filter]
            max_symbols = 1
            require_current_data = true

            [strategy.universe_rank]
            source = "parquet"
            path = "{}"
            manifest_path = "{}"
            run_id = "research-rank"
            feature_name = "quality_score"
            version = "v1"

            [portfolio]
            initial_cash = "100000"
            base_currency = "USD"
            order_qty = "1"
            max_abs_qty = "100"

            [risk]
            max_order_notional = "1000000"
            min_cash_after_order = "0"
            max_exposure = "1000000"
            max_drawdown = "1"
            max_leverage = "10"
            max_margin_used = "0"
            trading_halted = false

            [broker]
            kind = "simulated"
            mode = "paper"

            [paper]
            account_id = "paper"
            slippage_bps = "0"
            fee_bps = "0"

            [live]
            enabled = false
            "#,
            toml_path(&aapl_path),
            toml_path(&msft_path),
            toml_path(&feature_path),
            toml_path(&manifest_path)
        ),
    )
    .unwrap();
    config_path
}

fn write_weighted_alpha_config(run_id: &str) -> PathBuf {
    let config_path = temp_path("api-weighted-alpha-config", "toml");
    std::fs::write(
        &config_path,
        format!(
            r#"
            [runtime]
            mode = "backtest"
            run_id = "{run_id}"

            [database]
            url = "sqlite::memory:"

            [data]
            source = "csv"
            path = "datasets/sample/aapl_1d.csv"

            [strategy]
            name = "moving_average_cross"
            alpha = "moving_average_cross"
            alpha_conflict_resolution = "highest_confidence"
            symbols = ["US:NASDAQ:AAPL:EQUITY"]
            fast_window = 2
            slow_window = 3

            [[strategy.alpha_components]]
            name = "moving_average_cross"
            fast_window = 2
            slow_window = 3
            weight = 0.25

            [[strategy.alpha_components]]
            name = "moving_average_cross"
            fast_window = 2
            slow_window = 3
            weight = 0.5

            [portfolio]
            initial_cash = "100000"
            base_currency = "USD"
            order_qty = "1"
            max_abs_qty = "100"

            [risk]
            max_order_notional = "1000000"
            min_cash_after_order = "0"
            max_exposure = "1000000"
            max_drawdown = "1"
            max_leverage = "10"
            max_margin_used = "0"
            trading_halted = false

            [broker]
            kind = "simulated"
            mode = "paper"

            [paper]
            account_id = "paper"
            slippage_bps = "0"
            fee_bps = "0"

            [live]
            enabled = false
            "#
        ),
    )
    .unwrap();
    config_path
}

fn write_net_signal_alpha_config(run_id: &str) -> PathBuf {
    let bars_path = temp_path("api-net-signal-alpha-bars", "csv");
    std::fs::write(
        &bars_path,
        "ts_ms,open,high,low,close,volume\n1,10,10,10,10,1\n2,20,20,20,20,1\n",
    )
    .unwrap();
    let config_path = temp_path("api-net-signal-alpha-config", "toml");
    std::fs::write(
        &config_path,
        format!(
            r#"
            [runtime]
            mode = "backtest"
            run_id = "{run_id}"

            [database]
            url = "sqlite::memory:"

            [data]
            source = "csv"
            path = "{}"

            [strategy]
            name = "moving_average_cross"
            alpha = "moving_average_cross"
            alpha_conflict_resolution = "net_signal"
            symbols = ["US:NASDAQ:AAPL:EQUITY"]
            fast_window = 2
            slow_window = 3

            [[strategy.alpha_components]]
            name = "moving_average_cross"
            fast_window = 1
            slow_window = 2
            weight = 1.0

            [[strategy.alpha_components]]
            name = "moving_average_cross"
            fast_window = 2
            slow_window = 1
            weight = 0.25

            [portfolio]
            initial_cash = "100000"
            base_currency = "USD"
            order_qty = "1"
            max_abs_qty = "100"

            [risk]
            max_order_notional = "1000000"
            min_cash_after_order = "0"
            max_exposure = "1000000"
            max_drawdown = "1"
            max_leverage = "10"
            max_margin_used = "0"
            trading_halted = false

            [broker]
            kind = "simulated"
            mode = "paper"

            [paper]
            account_id = "paper"
            slippage_bps = "0"
            fee_bps = "0"

            [live]
            enabled = false
            "#,
            toml_path(&bars_path)
        ),
    )
    .unwrap();
    config_path
}

fn write_majority_vote_alpha_config(run_id: &str) -> PathBuf {
    let bars_path = temp_path("api-majority-vote-alpha-bars", "csv");
    std::fs::write(
        &bars_path,
        "ts_ms,open,high,low,close,volume\n1,10,10,10,10,1\n2,20,20,20,20,1\n",
    )
    .unwrap();
    let config_path = temp_path("api-majority-vote-alpha-config", "toml");
    std::fs::write(
        &config_path,
        format!(
            r#"
            [runtime]
            mode = "backtest"
            run_id = "{run_id}"

            [database]
            url = "sqlite::memory:"

            [data]
            source = "csv"
            path = "{}"

            [strategy]
            name = "moving_average_cross"
            alpha = "moving_average_cross"
            alpha_conflict_resolution = "majority_vote"
            symbols = ["US:NASDAQ:AAPL:EQUITY"]
            fast_window = 2
            slow_window = 3

            [[strategy.alpha_components]]
            name = "moving_average_cross"
            fast_window = 1
            slow_window = 2
            weight = 0.25

            [[strategy.alpha_components]]
            name = "moving_average_cross"
            fast_window = 1
            slow_window = 2
            weight = 0.5

            [[strategy.alpha_components]]
            name = "moving_average_cross"
            fast_window = 2
            slow_window = 1
            weight = 1.0

            [portfolio]
            initial_cash = "100000"
            base_currency = "USD"
            order_qty = "1"
            max_abs_qty = "100"

            [risk]
            max_order_notional = "1000000"
            min_cash_after_order = "0"
            max_exposure = "1000000"
            max_drawdown = "1"
            max_leverage = "10"
            max_margin_used = "0"
            trading_halted = false

            [broker]
            kind = "simulated"
            mode = "paper"

            [paper]
            account_id = "paper"
            slippage_bps = "0"
            fee_bps = "0"

            [live]
            enabled = false
            "#,
            toml_path(&bars_path)
        ),
    )
    .unwrap();
    config_path
}

fn write_category_majority_alpha_config(run_id: &str) -> PathBuf {
    let bars_path = temp_path("api-category-majority-alpha-bars", "csv");
    std::fs::write(
        &bars_path,
        "ts_ms,open,high,low,close,volume\n1,10,10,10,10,1\n2,20,20,20,20,1\n",
    )
    .unwrap();
    let config_path = temp_path("api-category-majority-alpha-config", "toml");
    std::fs::write(
        &config_path,
        format!(
            r#"
            [runtime]
            mode = "backtest"
            run_id = "{run_id}"

            [database]
            url = "sqlite::memory:"

            [data]
            source = "csv"
            path = "{}"

            [strategy]
            name = "moving_average_cross"
            alpha = "moving_average_cross"
            alpha_conflict_resolution = "category_majority"
            symbols = ["US:NASDAQ:AAPL:EQUITY"]
            fast_window = 2
            slow_window = 3

            [[strategy.alpha_components]]
            name = "moving_average_cross"
            category = "trend"
            fast_window = 2
            slow_window = 1
            weight = 0.25

            [[strategy.alpha_components]]
            name = "moving_average_cross"
            category = "trend"
            fast_window = 2
            slow_window = 1
            weight = 0.5

            [[strategy.alpha_components]]
            name = "moving_average_cross"
            category = "mean_reversion"
            fast_window = 1
            slow_window = 2
            weight = 1.0

            [[strategy.alpha_components]]
            name = "moving_average_cross"
            category = "quality"
            fast_window = 1
            slow_window = 2
            weight = 0.5

            [portfolio]
            initial_cash = "100000"
            base_currency = "USD"
            order_qty = "1"
            max_abs_qty = "100"

            [risk]
            max_order_notional = "1000000"
            min_cash_after_order = "0"
            max_exposure = "1000000"
            max_drawdown = "1"
            max_leverage = "10"
            max_margin_used = "0"
            trading_halted = false

            [broker]
            kind = "simulated"
            mode = "paper"

            [paper]
            account_id = "paper"
            slippage_bps = "0"
            fee_bps = "0"

            [live]
            enabled = false
            "#,
            toml_path(&bars_path)
        ),
    )
    .unwrap();
    config_path
}

fn temp_path(name: &str, extension: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "trader-{name}-{}.{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        extension
    ))
}

fn toml_path(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn stable_test_file_content_hash(path: &std::path::Path) -> String {
    let bytes = std::fs::read(path).unwrap();
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("fnv1a64:{hash:016x}")
}

async fn assert_status(app: axum::Router, run_id: &str, expected_status: &str) {
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/runs/{run_id}/status"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(
        bytes
            .as_ref()
            .windows(expected_status.len())
            .any(|window| window == expected_status.as_bytes())
    );
}

async fn wait_for_status(app: axum::Router, run_id: &str, expected_status: &str) {
    for _ in 0..50 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/v1/runs/{run_id}/status"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        if response.status() == StatusCode::OK {
            let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            if bytes
                .as_ref()
                .windows(expected_status.len())
                .any(|window| window == expected_status.as_bytes())
            {
                return;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("run {run_id} did not reach {expected_status}");
}
