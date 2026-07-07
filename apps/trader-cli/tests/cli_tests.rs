use assert_cmd::Command;
use feature_store::{
    FeatureBuildContract, FeatureManifestInput, FeatureRecord, build_feature_manifest,
    build_feature_manifest_with_contract, load_feature_manifest, load_feature_records_from_parquet,
    write_feature_manifest, write_feature_records_to_parquet,
};
use hmac::Mac;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use rust_decimal_macros::dec;
use std::path::PathBuf;

#[test]
fn check_config_prints_ok() {
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .arg("check-config")
        .assert()
        .success()
        .stdout(contains("config ok"));
}

#[test]
fn live_worker_starts_and_stops_over_jsonl() {
    let temp = std::env::temp_dir().join(format!("trader-live-worker-{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();
    let db_path = temp.join("worker.sqlite");
    let launch_path = temp.join("launch.json");
    let db_url = format!("sqlite:{}", toml_path(&db_path));
    let config_content = format!(
        r#"
        [runtime]
        mode = "live"
        run_id = "cli-live-worker"

        [database]
        url = "{}"

        [data]
        source = "csv"
        path = "datasets/sample/aapl_1d.csv"

        [strategy]
        name = "moving_average_cross"
        symbols = ["US:NASDAQ:AAPL:EQUITY"]
        fast_window = 2
        slow_window = 3

        [portfolio]
        initial_cash = "25000"
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
        slippage_bps = "25"
        fee_bps = "10"

        [live]
        enabled = true
        "#,
        db_url
    );
    let launch = serde_json::json!({
        "run_id": "cli-live-worker",
        "db_url": db_url,
        "config_path": null,
        "config_content": config_content,
        "config_format": "TOML",
        "run_spec": null,
        "broker_snapshot_interval_ms": null,
        "startup_recovery_unmatched_open_orders_policy": "Fail"
    });
    std::fs::write(&launch_path, serde_json::to_vec(&launch).unwrap()).unwrap();

    let mut command = Command::cargo_bin("trader").unwrap();
    let assert = command
        .current_dir(workspace_root())
        .arg("live-worker")
        .arg("--launch-file")
        .arg(&launch_path)
        .write_stdin("{\"type\":\"shutdown\",\"request_id\":\"stop-1\",\"reason\":\"test\"}\n")
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("\"type\":\"worker_started\""));
    assert!(stdout.contains("\"type\":\"runtime_started\""));
    assert!(stdout.contains("\"type\":\"runtime_stopped\""));
}

#[test]
fn live_worker_reconciliation_gate_allows_recent_clean_audit() {
    let (launch_path, db_path) = write_live_worker_gate_launch(
        "trader-live-worker-gate-ok",
        "paper",
        true,
        &["simulated:paper"],
        300_000,
    );
    seed_live_worker_gate_audits(
        &db_path,
        &[(
            "audit-ok",
            "simulated",
            "paper",
            chrono::Utc::now().timestamp_millis(),
        )],
    );

    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .arg("live-worker")
        .arg("--launch-file")
        .arg(&launch_path)
        .write_stdin("{\"type\":\"shutdown\",\"request_id\":\"stop-1\",\"reason\":\"test\"}\n")
        .assert()
        .success()
        .stdout(contains("\"type\":\"runtime_started\""));

    let logs = live_worker_gate_system_logs(&db_path, "trader-live-worker-gate-ok");
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].level, "INFO");
    assert_eq!(logs[0].message, "reconciliation_gate.allow");
    let fields = system_log_fields(&logs[0]);
    assert_eq!(fields["status"], "allow");
    assert_eq!(fields["source"], "cli.live_worker");
    assert_eq!(fields["requirements"][0]["broker"], "simulated");
    assert_eq!(fields["requirements"][0]["account_id"], "paper");
    assert!(
        fields["config_snapshot"]["checksum"]
            .as_str()
            .unwrap()
            .starts_with("fnv1a64:")
    );
}

#[test]
fn live_worker_reconciliation_gate_blocks_missing_audit() {
    let (launch_path, db_path) = write_live_worker_gate_launch(
        "trader-live-worker-gate-missing",
        "paper",
        true,
        &["simulated:paper"],
        300_000,
    );

    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .arg("live-worker")
        .arg("--launch-file")
        .arg(&launch_path)
        .assert()
        .failure()
        .stderr(contains("missing_required_audit"))
        .stderr(contains("reconciliation gate blocked"));

    let logs = live_worker_gate_system_logs(&db_path, "trader-live-worker-gate-missing");
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].level, "WARN");
    assert_eq!(logs[0].message, "reconciliation_gate.block");
    let fields = system_log_fields(&logs[0]);
    assert_eq!(fields["status"], "block");
    assert_eq!(fields["failure_count"], 1);
    assert_eq!(fields["failures"][0]["reason"], "missing_required_audit");
    assert_eq!(fields["failures"][0]["broker"], "simulated");
    assert_eq!(fields["failures"][0]["account_id"], "paper");
}

#[test]
fn live_worker_reconciliation_gate_blocks_stale_audit() {
    let (launch_path, db_path) = write_live_worker_gate_launch(
        "trader-live-worker-gate-stale",
        "paper",
        true,
        &["simulated:paper"],
        1,
    );
    seed_live_worker_gate_audits(&db_path, &[("audit-stale", "simulated", "paper", 1)]);

    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .arg("live-worker")
        .arg("--launch-file")
        .arg(&launch_path)
        .assert()
        .failure()
        .stderr(contains("audit_too_old"));
}

#[test]
fn live_worker_reconciliation_gate_blocks_missing_required_accounts() {
    let (launch_path, _db_path) = write_live_worker_gate_launch(
        "trader-live-worker-gate-no-accounts",
        "paper",
        true,
        &[],
        300_000,
    );

    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .arg("live-worker")
        .arg("--launch-file")
        .arg(&launch_path)
        .assert()
        .failure()
        .stderr(contains("reconciliation gate has no required accounts"));
}

#[test]
fn live_worker_reconciliation_gate_allows_multiple_broker_requirements() {
    let (launch_path, db_path) = write_live_worker_gate_launch(
        "trader-live-worker-gate-multi",
        "paper",
        true,
        &["simulated:paper", "binance:paper-binance"],
        300_000,
    );
    let now_ms = chrono::Utc::now().timestamp_millis();
    seed_live_worker_gate_audits(
        &db_path,
        &[
            ("audit-simulated", "simulated", "paper", now_ms),
            ("audit-binance", "binance", "paper-binance", now_ms),
        ],
    );

    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .arg("live-worker")
        .arg("--launch-file")
        .arg(&launch_path)
        .write_stdin("{\"type\":\"shutdown\",\"request_id\":\"stop-1\",\"reason\":\"test\"}\n")
        .assert()
        .success()
        .stdout(contains("\"type\":\"runtime_started\""));
}

#[test]
fn live_worker_real_money_mode_requires_reconciliation_gate_accounts() {
    let (launch_path, _db_path) = write_live_worker_gate_launch(
        "trader-live-worker-gate-real-money",
        "live",
        false,
        &[],
        300_000,
    );

    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .arg("live-worker")
        .arg("--launch-file")
        .arg(&launch_path)
        .assert()
        .failure()
        .stderr(contains("reconciliation gate has no required accounts"));
}

#[test]
fn backtest_accepts_config_argument() {
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args(["backtest", "--config", "configs/backtest/ma_cross.toml"])
        .assert()
        .success()
        .stdout(contains("backtest completed"));
}

#[test]
fn backtest_accepts_ema_cross_config() {
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args(["backtest", "--config", "configs/backtest/ema_cross.toml"])
        .assert()
        .success()
        .stdout(contains("backtest completed"));
}

#[test]
fn backtest_accepts_price_momentum_config() {
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "backtest",
            "--config",
            "configs/backtest/price_momentum.toml",
        ])
        .assert()
        .success()
        .stdout(contains("backtest completed"));
}

#[test]
fn backtest_accepts_price_channel_breakout_config() {
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "backtest",
            "--config",
            "configs/backtest/price_channel_breakout.toml",
        ])
        .assert()
        .success()
        .stdout(contains("backtest completed"));
}

#[test]
fn backtest_accepts_price_channel_reversion_config() {
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "backtest",
            "--config",
            "configs/backtest/price_channel_reversion.toml",
        ])
        .assert()
        .success()
        .stdout(contains("backtest completed"));
}

#[test]
fn backtest_accepts_rsi_reversion_config() {
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "backtest",
            "--config",
            "configs/backtest/rsi_reversion.toml",
        ])
        .assert()
        .success()
        .stdout(contains("backtest completed: signals=1 orders=1"));
}

#[test]
fn backtest_derives_short_permission_for_crypto_perp_config() {
    let config = write_crypto_perp_reversion_cli_config();
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args(["backtest", "--config", config.to_str().unwrap()])
        .assert()
        .success()
        .stdout(contains("backtest completed: signals=1 orders=1"));

    std::fs::remove_file(config).unwrap();
}

#[test]
fn backtest_accepts_multi_symbol_data_inputs() {
    let config = write_multi_symbol_cli_config("cli-backtest-multi-symbol", "backtest");
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args(["backtest", "--config", config.to_str().unwrap()])
        .assert()
        .success()
        .stdout(contains("backtest completed: signals=2 orders=2"));
}

#[test]
fn backtest_accepts_filtered_universe_config() {
    let config =
        write_filtered_multi_symbol_cli_config("cli-backtest-filtered-universe", "backtest");
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args(["backtest", "--config", config.to_str().unwrap()])
        .assert()
        .success()
        .stdout(contains("backtest completed: signals=1 orders=1"));
}

#[test]
fn backtest_accepts_ranked_universe_config() {
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "backtest",
            "--config",
            "configs/backtest/ranked_universe_ma_cross.toml",
        ])
        .assert()
        .success()
        .stdout(contains("backtest completed: signals=1 orders=1"));
}

#[test]
fn backtest_accepts_feature_ranked_universe_config() {
    let config = write_feature_ranked_multi_symbol_cli_config("cli-backtest-feature-ranked");
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args(["backtest", "--config", config.to_str().unwrap()])
        .assert()
        .success()
        .stdout(contains("backtest completed: signals=1 orders=1"));
}

#[test]
fn backtest_accepts_sample_feature_ranked_universe_config() {
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "backtest",
            "--config",
            "configs/backtest/feature_ranked_universe_ma_cross.toml",
        ])
        .assert()
        .success()
        .stdout(contains("backtest completed: signals=1 orders=1"));
}

#[test]
fn backtest_accepts_net_signal_alpha_config() {
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "backtest",
            "--config",
            "configs/backtest/net_signal_alpha_ma_cross.toml",
        ])
        .assert()
        .success()
        .stdout(contains("backtest completed"));
}

#[test]
fn backtest_accepts_majority_vote_alpha_config() {
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "backtest",
            "--config",
            "configs/backtest/majority_vote_alpha_ma_cross.toml",
        ])
        .assert()
        .success()
        .stdout(contains("backtest completed"));
}

#[test]
fn backtest_accepts_category_majority_alpha_config() {
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "backtest",
            "--config",
            "configs/backtest/category_majority_alpha_ma_cross.toml",
        ])
        .assert()
        .success()
        .stdout(contains("backtest completed"));
}

#[test]
fn backtest_accepts_sample_sma_feature_gate_config() {
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "backtest",
            "--config",
            "configs/backtest/sma_feature_gate.toml",
        ])
        .assert()
        .success()
        .stdout(contains("backtest completed"));
}

#[test]
fn backtest_accepts_sample_rsi_feature_gate_config() {
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "backtest",
            "--config",
            "configs/backtest/rsi_feature_gate.toml",
        ])
        .assert()
        .success()
        .stdout(contains("backtest completed: signals=1 orders=1"));
}

#[test]
fn backtest_sample_sma_feature_gate_suppresses_below_threshold() {
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "backtest",
            "--config",
            "configs/backtest/sma_feature_gate_suppressed.toml",
        ])
        .assert()
        .success()
        .stdout(contains("backtest completed: signals=0 orders=0"));
}

#[test]
fn backtest_accepts_sample_multi_symbol_sma_feature_gate_config() {
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "backtest",
            "--config",
            "configs/backtest/multi_symbol_sma_feature_gate.toml",
        ])
        .assert()
        .success()
        .stdout(contains("backtest completed: signals=2 orders=2"));
}

#[test]
fn paper_run_accepts_multi_symbol_data_inputs() {
    let config = write_multi_symbol_cli_config("cli-paper-multi-symbol", "paper");
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args(["paper-run", "--config", config.to_str().unwrap()])
        .assert()
        .success()
        .stdout(contains("paper completed: signals=2 orders=2"));
}

#[test]
fn import_bars_can_write_parquet_output() {
    let output = std::env::temp_dir().join(format!(
        "trader-cli-import-{}.parquet",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "import-bars",
            "--config",
            "configs/backtest/ma_cross.toml",
            "--output-parquet",
            output.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(contains("wrote parquet bars: 3"));

    assert!(output.exists());
    std::fs::remove_file(output).unwrap();
}

#[test]
fn feature_manifest_command_writes_json_summary() {
    let parquet = temp_output("trader-cli-features", "parquet");
    let manifest = temp_output("trader-cli-feature-manifest", "json");
    write_feature_records_to_parquet(
        &parquet,
        &[
            FeatureRecord::new(
                "research-1",
                "US:NASDAQ:AAPL:EQUITY",
                1,
                "quality_score",
                dec!(0.8),
                "v1",
            ),
            FeatureRecord::new(
                "research-1",
                "US:NASDAQ:MSFT:EQUITY",
                1,
                "quality_score",
                dec!(0.7),
                "v1",
            ),
        ],
    )
    .unwrap();

    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "feature-manifest",
            "--parquet",
            parquet.to_str().unwrap(),
            "--output",
            manifest.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(contains("wrote feature manifest: records=2"));

    let summary = load_feature_manifest(&manifest).unwrap();
    assert_eq!(summary.record_count, 2);
    assert_eq!(summary.run_ids, vec!["research-1"]);
    assert_eq!(summary.feature_names, vec!["quality_score"]);
    assert_eq!(summary.versions, vec!["v1"]);

    std::fs::remove_file(parquet).unwrap();
    std::fs::remove_file(manifest).unwrap();
}

#[test]
fn feature_build_sma_command_writes_features_and_manifest() {
    let bars = temp_output("trader-cli-feature-sma-bars", "csv");
    let output = temp_output("trader-cli-feature-sma", "parquet");
    let manifest = temp_output("trader-cli-feature-sma", "json");
    std::fs::write(
        &bars,
        "ts_ms,open,high,low,close,volume\n1,10,10,10,10,1\n2,12,12,12,12,1\n3,14,14,14,14,1\n",
    )
    .unwrap();

    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "feature-build-sma",
            "--source",
            "csv",
            "--input",
            bars.to_str().unwrap(),
            "--symbol",
            "US:NASDAQ:AAPL:EQUITY",
            "--run-id",
            "research-run-1",
            "--feature-name",
            "sma_close_2",
            "--period",
            "2",
            "--version",
            "v1",
            "--output",
            output.to_str().unwrap(),
            "--manifest-output",
            manifest.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(contains("wrote sma features: records=2"));

    let records = load_feature_records_from_parquet(&output).unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].ts_ms, 2);
    assert_eq!(records[0].value, dec!(11));
    assert_eq!(records[1].ts_ms, 3);
    assert_eq!(records[1].value, dec!(13));

    let summary = load_feature_manifest(&manifest).unwrap();
    assert_eq!(summary.record_count, 2);
    assert_eq!(summary.run_ids, vec!["research-run-1"]);
    assert_eq!(summary.symbols, vec!["US:NASDAQ:AAPL:EQUITY"]);
    assert_eq!(summary.feature_names, vec!["sma_close_2"]);
    assert_eq!(summary.versions, vec!["v1"]);

    std::fs::remove_file(bars).unwrap();
    std::fs::remove_file(output).unwrap();
    std::fs::remove_file(manifest).unwrap();
}

#[test]
fn feature_build_indicator_command_writes_ema_features_and_manifest() {
    let bars = temp_output("trader-cli-feature-ema-bars", "csv");
    let output = temp_output("trader-cli-feature-ema", "parquet");
    let manifest = temp_output("trader-cli-feature-ema", "json");
    std::fs::write(
        &bars,
        "ts_ms,open,high,low,close,volume\n1,10,10,10,10,1\n2,12,12,12,12,1\n3,14,14,14,14,1\n",
    )
    .unwrap();

    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "feature-build-indicator",
            "--indicator",
            "ema",
            "--source",
            "csv",
            "--input",
            bars.to_str().unwrap(),
            "--symbol",
            "US:NASDAQ:AAPL:EQUITY",
            "--run-id",
            "research-run-ema",
            "--feature-name",
            "ema_close_1",
            "--period",
            "1",
            "--version",
            "v1",
            "--output",
            output.to_str().unwrap(),
            "--manifest-output",
            manifest.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(contains("wrote ema features: records=3"));

    let records = load_feature_records_from_parquet(&output).unwrap();
    assert_eq!(records.len(), 3);
    assert_eq!(records[0].ts_ms, 1);
    assert_eq!(records[0].value, dec!(10));
    assert_eq!(records[1].ts_ms, 2);
    assert_eq!(records[1].value, dec!(12));
    assert_eq!(records[2].ts_ms, 3);
    assert_eq!(records[2].value, dec!(14));

    let summary = load_feature_manifest(&manifest).unwrap();
    assert_eq!(summary.record_count, 3);
    assert_eq!(summary.run_ids, vec!["research-run-ema"]);
    assert_eq!(summary.symbols, vec!["US:NASDAQ:AAPL:EQUITY"]);
    assert_eq!(summary.feature_names, vec!["ema_close_1"]);
    assert_eq!(summary.versions, vec!["v1"]);
    let build_contract = summary.build_contract.unwrap();
    assert_eq!(build_contract.builder, "feature-build-indicator");
    assert_eq!(build_contract.indicator, "ema");
    assert_eq!(build_contract.value_column, "close");
    assert_eq!(build_contract.period, 1);
    assert_eq!(build_contract.run_id, "research-run-ema");
    assert_eq!(build_contract.feature_name, "ema_close_1");
    assert_eq!(build_contract.version, "v1");
    assert_eq!(
        build_contract.inputs,
        vec![FeatureManifestInput {
            symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
            source: "csv".to_string(),
            path: bars.to_string_lossy().to_string(),
            content_hash: build_contract.inputs[0].content_hash.clone(),
            bar_count: Some(3),
            first_ts_ms: Some(1),
            last_ts_ms: Some(3),
        }]
    );

    std::fs::remove_file(bars).unwrap();
    std::fs::remove_file(output).unwrap();
    std::fs::remove_file(manifest).unwrap();
}

#[test]
fn feature_build_indicator_command_writes_rsi_features_and_manifest() {
    let bars = temp_output("trader-cli-feature-rsi-bars", "csv");
    let output = temp_output("trader-cli-feature-rsi", "parquet");
    let manifest = temp_output("trader-cli-feature-rsi", "json");
    std::fs::write(
        &bars,
        "ts_ms,open,high,low,close,volume\n1,10,10,10,10,1\n2,9,9,9,9,1\n3,8,8,8,8,1\n4,7,7,7,7,1\n",
    )
    .unwrap();

    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "feature-build-indicator",
            "--indicator",
            "rsi",
            "--source",
            "csv",
            "--input",
            bars.to_str().unwrap(),
            "--symbol",
            "US:NASDAQ:AAPL:EQUITY",
            "--run-id",
            "research-run-rsi",
            "--feature-name",
            "rsi_close_3",
            "--period",
            "3",
            "--version",
            "v1",
            "--output",
            output.to_str().unwrap(),
            "--manifest-output",
            manifest.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(contains("wrote rsi features: records=1"));

    let records = load_feature_records_from_parquet(&output).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].ts_ms, 4);
    assert_eq!(records[0].value, dec!(0));

    let summary = load_feature_manifest(&manifest).unwrap();
    assert_eq!(summary.record_count, 1);
    assert_eq!(summary.run_ids, vec!["research-run-rsi"]);
    assert_eq!(summary.symbols, vec!["US:NASDAQ:AAPL:EQUITY"]);
    assert_eq!(summary.feature_names, vec!["rsi_close_3"]);
    assert_eq!(summary.versions, vec!["v1"]);
    let build_contract = summary.build_contract.unwrap();
    assert_eq!(build_contract.builder, "feature-build-indicator");
    assert_eq!(build_contract.indicator, "rsi");
    assert_eq!(build_contract.value_column, "close");
    assert_eq!(build_contract.period, 3);
    assert_eq!(build_contract.run_id, "research-run-rsi");
    assert_eq!(build_contract.feature_name, "rsi_close_3");
    assert_eq!(build_contract.version, "v1");
    assert_eq!(build_contract.inputs.len(), 1);
    assert_eq!(build_contract.inputs[0].bar_count, Some(4));
    assert_eq!(build_contract.inputs[0].first_ts_ms, Some(1));
    assert_eq!(build_contract.inputs[0].last_ts_ms, Some(4));

    std::fs::remove_file(bars).unwrap();
    std::fs::remove_file(output).unwrap();
    std::fs::remove_file(manifest).unwrap();
}

#[test]
fn feature_build_indicator_command_writes_multi_symbol_features_from_config() {
    let config = write_multi_symbol_cli_config("cli-feature-multi-symbol", "backtest");
    let output = temp_output("trader-cli-feature-multi-symbol", "parquet");
    let manifest = temp_output("trader-cli-feature-multi-symbol", "json");

    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "feature-build-indicator",
            "--indicator",
            "sma",
            "--inputs-config",
            config.to_str().unwrap(),
            "--run-id",
            "research-run-multi",
            "--feature-name",
            "sma_close_2",
            "--period",
            "2",
            "--version",
            "v1",
            "--output",
            output.to_str().unwrap(),
            "--manifest-output",
            manifest.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(contains("wrote sma features: records=4"));

    let records = load_feature_records_from_parquet(&output).unwrap();
    assert_eq!(records.len(), 4);
    assert_eq!(records[0].key.symbol, "US:NASDAQ:AAPL:EQUITY");
    assert_eq!(records[0].ts_ms, 2);
    assert_eq!(records[0].value, dec!(10.5));
    assert_eq!(records[1].key.symbol, "US:NASDAQ:AAPL:EQUITY");
    assert_eq!(records[1].ts_ms, 3);
    assert_eq!(records[1].value, dec!(15.5));
    assert_eq!(records[2].key.symbol, "US:NASDAQ:MSFT:EQUITY");
    assert_eq!(records[2].ts_ms, 2);
    assert_eq!(records[2].value, dec!(30.5));
    assert_eq!(records[3].key.symbol, "US:NASDAQ:MSFT:EQUITY");
    assert_eq!(records[3].ts_ms, 3);
    assert_eq!(records[3].value, dec!(35.5));

    let summary = load_feature_manifest(&manifest).unwrap();
    assert_eq!(summary.record_count, 4);
    assert_eq!(summary.run_ids, vec!["research-run-multi"]);
    assert_eq!(
        summary.symbols,
        vec!["US:NASDAQ:AAPL:EQUITY", "US:NASDAQ:MSFT:EQUITY"]
    );
    assert_eq!(summary.feature_names, vec!["sma_close_2"]);
    assert_eq!(summary.versions, vec!["v1"]);
    let build_contract = summary.build_contract.unwrap();
    assert_eq!(build_contract.builder, "feature-build-indicator");
    assert_eq!(build_contract.indicator, "sma");
    assert_eq!(build_contract.value_column, "close");
    assert_eq!(build_contract.period, 2);
    assert_eq!(build_contract.run_id, "research-run-multi");
    assert_eq!(build_contract.feature_name, "sma_close_2");
    assert_eq!(build_contract.version, "v1");
    assert_eq!(build_contract.inputs.len(), 2);
    assert_eq!(build_contract.inputs[0].symbol, "US:NASDAQ:AAPL:EQUITY");
    assert_eq!(build_contract.inputs[0].source, "csv");
    assert_eq!(build_contract.inputs[1].symbol, "US:NASDAQ:MSFT:EQUITY");
    assert_eq!(build_contract.inputs[1].source, "csv");

    std::fs::remove_file(output).unwrap();
    std::fs::remove_file(manifest).unwrap();
}

#[test]
fn paper_preflight_rejects_alpha_gate_manifest_version_mismatch() {
    let bars = temp_output("trader-cli-alpha-gate-bars", "csv");
    let parquet = temp_output("trader-cli-alpha-gate-features", "parquet");
    let manifest_path = temp_output("trader-cli-alpha-gate-manifest", "json");
    let config = temp_output("trader-cli-alpha-gate", "toml");
    std::fs::write(
        &bars,
        "ts_ms,open,high,low,close,volume\n1,10,10,10,10,1\n2,11,11,11,11,1\n3,20,20,20,20,1\n",
    )
    .unwrap();
    let records = vec![FeatureRecord::new(
        "research-run-1",
        "US:NASDAQ:AAPL:EQUITY",
        1,
        "quality_score",
        dec!(0.8),
        "v1",
    )];
    write_feature_records_to_parquet(&parquet, &records).unwrap();
    let manifest = build_feature_manifest(&parquet, &records);
    write_feature_manifest(&manifest_path, &manifest).unwrap();
    std::fs::write(
        &config,
        format!(
            r#"
            [runtime]
            mode = "paper"
            run_id = "cli-alpha-gate-manifest"

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
            version = "v2"

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
            toml_path(&parquet),
            toml_path(&manifest_path)
        ),
    )
    .unwrap();

    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args(["paper-preflight", "--config", config.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(contains("version v2"));

    std::fs::remove_file(bars).unwrap();
    std::fs::remove_file(parquet).unwrap();
    std::fs::remove_file(manifest_path).unwrap();
    std::fs::remove_file(config).unwrap();
}

#[test]
fn paper_preflight_rejects_alpha_gate_manifest_parquet_path_mismatch() {
    let bars = temp_output("trader-cli-alpha-gate-path-bars", "csv");
    let parquet = temp_output("trader-cli-alpha-gate-path-features", "parquet");
    let manifest_path = temp_output("trader-cli-alpha-gate-path-manifest", "json");
    let config = temp_output("trader-cli-alpha-gate-path", "toml");
    std::fs::write(
        &bars,
        "ts_ms,open,high,low,close,volume\n1,10,10,10,10,1\n2,11,11,11,11,1\n3,20,20,20,20,1\n",
    )
    .unwrap();
    let records = vec![FeatureRecord::new(
        "research-run-1",
        "US:NASDAQ:AAPL:EQUITY",
        1,
        "quality_score",
        dec!(0.8),
        "v1",
    )];
    write_feature_records_to_parquet(&parquet, &records).unwrap();
    let manifest = build_feature_manifest("datasets/features/other.parquet", &records);
    write_feature_manifest(&manifest_path, &manifest).unwrap();
    std::fs::write(
        &config,
        format!(
            r#"
            [runtime]
            mode = "paper"
            run_id = "cli-alpha-gate-manifest-path"

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
            toml_path(&bars),
            toml_path(&parquet),
            toml_path(&manifest_path)
        ),
    )
    .unwrap();

    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args(["paper-preflight", "--config", config.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(contains("parquet_path"));

    std::fs::remove_file(bars).unwrap();
    std::fs::remove_file(parquet).unwrap();
    std::fs::remove_file(manifest_path).unwrap();
    std::fs::remove_file(config).unwrap();
}

#[test]
fn paper_preflight_rejects_alpha_gate_manifest_source_bars_mismatch() {
    let configured_bars = temp_output("trader-cli-alpha-gate-configured-bars", "csv");
    let research_bars = temp_output("trader-cli-alpha-gate-research-bars", "csv");
    let parquet = temp_output("trader-cli-alpha-gate-source-features", "parquet");
    let manifest_path = temp_output("trader-cli-alpha-gate-source-manifest", "json");
    let config = temp_output("trader-cli-alpha-gate-source", "toml");
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
        dec!(0.8),
        "v1",
    )];
    write_feature_records_to_parquet(&parquet, &records).unwrap();
    let manifest = build_feature_manifest_with_contract(
        &parquet,
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
        &config,
        format!(
            r#"
            [runtime]
            mode = "paper"
            run_id = "cli-alpha-gate-manifest-source"

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
            toml_path(&parquet),
            toml_path(&manifest_path)
        ),
    )
    .unwrap();

    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args(["paper-preflight", "--config", config.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(contains("build inputs"));

    std::fs::remove_file(configured_bars).unwrap();
    std::fs::remove_file(research_bars).unwrap();
    std::fs::remove_file(parquet).unwrap();
    std::fs::remove_file(manifest_path).unwrap();
    std::fs::remove_file(config).unwrap();
}

#[test]
fn paper_preflight_rejects_alpha_gate_manifest_build_contract_mismatch() {
    let bars = temp_output("trader-cli-alpha-gate-build-bars", "csv");
    let parquet = temp_output("trader-cli-alpha-gate-build-features", "parquet");
    let manifest_path = temp_output("trader-cli-alpha-gate-build-manifest", "json");
    let config = temp_output("trader-cli-alpha-gate-build", "toml");
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
        dec!(0.8),
        "v1",
    )];
    write_feature_records_to_parquet(&parquet, &records).unwrap();
    let manifest = build_feature_manifest_with_contract(
        &parquet,
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
        &config,
        format!(
            r#"
            [runtime]
            mode = "paper"
            run_id = "cli-alpha-gate-manifest-build"

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
            toml_path(&parquet),
            toml_path(&manifest_path)
        ),
    )
    .unwrap();

    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args(["paper-preflight", "--config", config.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(contains("build contract"))
        .stderr(contains("indicator"));

    std::fs::remove_file(bars).unwrap();
    std::fs::remove_file(parquet).unwrap();
    std::fs::remove_file(manifest_path).unwrap();
    std::fs::remove_file(config).unwrap();
}

#[test]
fn paper_preflight_rejects_alpha_gate_manifest_when_source_bars_content_changes() {
    let bars = temp_output("trader-cli-alpha-gate-content-bars", "csv");
    let parquet = temp_output("trader-cli-alpha-gate-content-features", "parquet");
    let manifest_path = temp_output("trader-cli-alpha-gate-content-manifest", "json");
    let config = temp_output("trader-cli-alpha-gate-content", "toml");
    std::fs::write(
        &bars,
        "ts_ms,open,high,low,close,volume\n1,10,10,10,10,1\n2,12,12,12,12,1\n3,14,14,14,14,1\n",
    )
    .unwrap();

    let mut build_command = Command::cargo_bin("trader").unwrap();
    build_command
        .current_dir(workspace_root())
        .args([
            "feature-build-indicator",
            "--indicator",
            "sma",
            "--source",
            "csv",
            "--input",
            bars.to_str().unwrap(),
            "--symbol",
            "US:NASDAQ:AAPL:EQUITY",
            "--run-id",
            "research-run-1",
            "--feature-name",
            "quality_score",
            "--period",
            "2",
            "--version",
            "v1",
            "--output",
            parquet.to_str().unwrap(),
            "--manifest-output",
            manifest_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    std::fs::write(
        &bars,
        "ts_ms,open,high,low,close,volume\n1,100,100,100,100,1\n2,120,120,120,120,1\n3,140,140,140,140,1\n",
    )
    .unwrap();
    std::fs::write(
        &config,
        format!(
            r#"
            [runtime]
            mode = "paper"
            run_id = "cli-alpha-gate-manifest-content"

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
            toml_path(&parquet),
            toml_path(&manifest_path)
        ),
    )
    .unwrap();

    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args(["paper-preflight", "--config", config.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(contains("content_hash"));

    std::fs::remove_file(bars).unwrap();
    std::fs::remove_file(parquet).unwrap();
    std::fs::remove_file(manifest_path).unwrap();
    std::fs::remove_file(config).unwrap();
}

#[test]
fn paper_run_accepts_config_argument() {
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args(["paper-run", "--config", "configs/backtest/ma_cross.toml"])
        .assert()
        .success()
        .stdout(contains("paper completed"));
}

#[test]
fn paper_preflight_prints_dry_run_summary() {
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "paper-preflight",
            "--config",
            "configs/backtest/slow-paper.toml",
        ])
        .assert()
        .success()
        .stdout(contains("paper preflight ok"))
        .stdout(contains("run_id=sample-slow-paper"))
        .stdout(contains("broker=simulated"))
        .stdout(contains("bars=3"))
        .stdout(contains("order_submit_enabled=false"));
}

#[test]
fn paper_preflight_fails_when_bars_are_missing() {
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "paper-preflight",
            "--config",
            "configs/backtest/missing-bars.toml",
        ])
        .assert()
        .failure()
        .stderr(contains("missing-bars.csv"));
}

#[test]
fn binance_paper_preflight_requires_testnet_credentials() {
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .env_remove("BINANCE_TESTNET_API_KEY")
        .env_remove("BINANCE_TESTNET_SECRET_KEY")
        .args([
            "paper-preflight",
            "--config",
            "configs/paper/binance_testnet.toml",
        ])
        .assert()
        .failure()
        .stderr(contains("BINANCE_TESTNET_API_KEY"));
}

#[test]
fn binance_paper_preflight_reports_real_testnet_readiness() {
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .env("BINANCE_TESTNET_API_KEY", "paper-key")
        .env("BINANCE_TESTNET_SECRET_KEY", "paper-secret")
        .args([
            "paper-preflight",
            "--config",
            "configs/paper/binance_testnet.toml",
        ])
        .assert()
        .success()
        .stdout(contains("broker=binance"))
        .stdout(contains("real_broker_connection=true"))
        .stdout(contains("order_submit_enabled=false"));
}

#[test]
fn binance_paper_readonly_requires_testnet_credentials() {
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .env_remove("BINANCE_TESTNET_API_KEY")
        .env_remove("BINANCE_TESTNET_SECRET_KEY")
        .args([
            "binance-paper-readonly",
            "--config",
            "configs/paper/binance_testnet.toml",
        ])
        .assert()
        .failure()
        .stderr(contains("BINANCE_TESTNET_API_KEY"));
}

#[test]
fn ibkr_paper_readonly_reports_connection_failure_without_gateway() {
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "ibkr-paper-readonly",
            "--config",
            "configs/paper/ibkr_aapl_1d_parquet.toml",
        ])
        .assert()
        .failure()
        .stderr(contains("unable to connect to IBKR paper gateway"));
}

#[test]
fn ibkr_paper_open_orders_reports_connection_failure_without_gateway() {
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "ibkr-paper-open-orders",
            "--config",
            "configs/paper/ibkr_aapl_1d_parquet.toml",
        ])
        .assert()
        .failure()
        .stderr(contains("unable to connect to IBKR paper gateway"));
}

#[test]
fn ibkr_paper_executions_reports_connection_failure_without_gateway() {
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "ibkr-paper-executions",
            "--config",
            "configs/paper/ibkr_aapl_1d_parquet.toml",
            "--request-id",
            "77",
        ])
        .assert()
        .failure()
        .stderr(contains("unable to connect to IBKR paper gateway"));
}

#[test]
fn ibkr_paper_reconcile_reports_connection_failure_without_gateway() {
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "ibkr-paper-reconcile",
            "--config",
            "configs/paper/ibkr_aapl_1d_parquet.toml",
            "--request-id",
            "77",
        ])
        .assert()
        .failure()
        .stderr(contains("unable to connect to IBKR paper gateway"));
}

#[test]
fn ibkr_paper_recover_succeeds_with_no_recoverable_orders() {
    let config = write_ibkr_cli_config(7497, "DU12345", "US:NASDAQ:AAPL:EQUITY");
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args(["ibkr-paper-recover", "--config", config.to_str().unwrap()])
        .assert()
        .success()
        .stdout(contains("ibkr paper recover ok: scanned=0"));

    std::fs::remove_file(config).unwrap();
}

#[test]
fn ibkr_paper_next_order_id_reports_connection_failure_without_gateway() {
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "ibkr-paper-next-order-id",
            "--config",
            "configs/paper/ibkr_aapl_1d_parquet.toml",
        ])
        .assert()
        .failure()
        .stderr(contains("unable to connect to IBKR paper gateway"));
}

#[test]
fn ibkr_paper_cancel_order_requires_explicit_confirmation() {
    let config = write_ibkr_cli_config(7497, "DU12345", "US:NASDAQ:AAPL:EQUITY");

    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "ibkr-paper-cancel-order",
            "--config",
            config.to_str().unwrap(),
            "--order-id",
            "42",
        ])
        .assert()
        .failure()
        .stderr(contains("--confirm-ibkr-paper-cancel"));

    std::fs::remove_file(config).unwrap();
}

#[test]
fn ibkr_paper_cancel_order_reports_connection_failure_without_gateway_after_confirmation() {
    let config = write_ibkr_cli_config(7497, "DU12345", "US:NASDAQ:AAPL:EQUITY");
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "ibkr-paper-cancel-order",
            "--config",
            config.to_str().unwrap(),
            "--order-id",
            "42",
            "--confirm-ibkr-paper-cancel",
        ])
        .assert()
        .failure()
        .stderr(contains("unable to connect to IBKR paper gateway"));

    std::fs::remove_file(config).unwrap();
}

#[test]
fn ibkr_paper_tiny_order_requires_explicit_confirmation() {
    let config = write_ibkr_cli_config(7497, "DU12345", "US:NASDAQ:AAPL:EQUITY");

    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "ibkr-paper-tiny-order",
            "--config",
            config.to_str().unwrap(),
            "--symbol",
            "AAPL",
            "--side",
            "buy",
            "--qty",
            "1",
            "--price",
            "185.25",
        ])
        .assert()
        .failure()
        .stderr(contains("--confirm-ibkr-paper-order"));

    std::fs::remove_file(config).unwrap();
}

#[test]
fn ibkr_paper_tiny_order_reports_connection_failure_without_gateway_after_confirmation() {
    let config = write_ibkr_cli_config(7497, "DU12345", "US:NASDAQ:AAPL:EQUITY");
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "ibkr-paper-tiny-order",
            "--config",
            config.to_str().unwrap(),
            "--symbol",
            "AAPL",
            "--side",
            "buy",
            "--qty",
            "1",
            "--price",
            "185.25",
            "--confirm-ibkr-paper-order",
        ])
        .assert()
        .failure()
        .stderr(contains("unable to connect to IBKR paper gateway"));

    std::fs::remove_file(config).unwrap();
}

#[test]
fn ibkr_paper_preflight_with_submit_reports_connection_failure_without_gateway() {
    let config =
        write_ibkr_cli_config_with_order_submit(7497, "DU12345", "US:NASDAQ:AAPL:EQUITY", true);
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args(["paper-preflight", "--config", config.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(contains("unable to connect to IBKR paper gateway"));

    std::fs::remove_file(config).unwrap();
}

#[test]
fn ibkr_paper_run_with_submit_reports_connection_failure_without_gateway() {
    let config =
        write_ibkr_cli_config_with_order_submit(7497, "DU12345", "US:NASDAQ:AAPL:EQUITY", true);
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args(["paper-run", "--config", config.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(contains("unable to connect to IBKR paper gateway"));

    std::fs::remove_file(config).unwrap();
}

#[test]
fn binance_paper_recover_requires_testnet_credentials() {
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .env_remove("BINANCE_TESTNET_API_KEY")
        .env_remove("BINANCE_TESTNET_SECRET_KEY")
        .args([
            "binance-paper-recover",
            "--config",
            "configs/paper/binance_testnet.toml",
        ])
        .assert()
        .failure()
        .stderr(contains("BINANCE_TESTNET_API_KEY"));
}

#[test]
fn binance_paper_open_orders_requires_testnet_credentials() {
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .env_remove("BINANCE_TESTNET_API_KEY")
        .env_remove("BINANCE_TESTNET_SECRET_KEY")
        .args([
            "binance-paper-open-orders",
            "--config",
            "configs/paper/binance_testnet.toml",
            "--symbol",
            "BTCUSDT",
        ])
        .assert()
        .failure()
        .stderr(contains("BINANCE_TESTNET_API_KEY"));
}

#[test]
fn binance_paper_reconcile_requires_testnet_credentials() {
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .env_remove("BINANCE_TESTNET_API_KEY")
        .env_remove("BINANCE_TESTNET_SECRET_KEY")
        .args([
            "binance-paper-reconcile",
            "--config",
            "configs/paper/binance_testnet.toml",
            "--symbol",
            "BTCUSDT",
        ])
        .assert()
        .failure()
        .stderr(contains("BINANCE_TESTNET_API_KEY"));
}

#[test]
fn binance_paper_cancel_open_orders_requires_explicit_confirmation() {
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "binance-paper-cancel-open-orders",
            "--config",
            "configs/paper/binance_testnet.toml",
            "--symbol",
            "BTCUSDT",
        ])
        .assert()
        .failure()
        .stderr(contains("--confirm-testnet-cancel"));
}

#[test]
fn risk_kill_switch_requires_explicit_confirmation() {
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "risk-kill-switch",
            "--config",
            "configs/backtest/ma_cross.toml",
            "--run-id",
            "kill-switch-test",
        ])
        .assert()
        .failure()
        .stderr(contains("--confirm-kill-switch"));
}

#[test]
fn risk_kill_switch_records_auditable_event() {
    let db_path = temp_output("trader-cli-risk-kill-switch", "sqlite");
    let config = write_report_cli_config(&db_path);

    let mut kill_switch = Command::cargo_bin("trader").unwrap();
    kill_switch
        .current_dir(workspace_root())
        .args([
            "risk-kill-switch",
            "--config",
            config.to_str().unwrap(),
            "--run-id",
            "cli-risk-kill-switch",
            "--confirm-kill-switch",
        ])
        .assert()
        .success()
        .stdout(contains("risk kill switch ok:"));

    let mut risk_events = Command::cargo_bin("trader").unwrap();
    risk_events
        .current_dir(workspace_root())
        .args([
            "risk-events",
            "--config",
            config.to_str().unwrap(),
            "--run-id",
            "cli-risk-kill-switch",
        ])
        .assert()
        .success()
        .stdout(contains("risk_event: run_id=cli-risk-kill-switch"))
        .stdout(contains("risk_type=operator_kill_switch"))
        .stdout(contains("reason=operator activated kill switch"));

    std::fs::remove_file(config).unwrap();
    std::fs::remove_file(db_path).unwrap();
}

#[test]
fn binance_paper_klines_rejects_zero_limit() {
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "binance-paper-klines",
            "--config",
            "configs/paper/binance_testnet.toml",
            "--symbol",
            "BTCUSDT",
            "--interval",
            "1m",
            "--limit",
            "0",
            "--format",
            "parquet",
            "--output",
            "target/tmp/unused-binance-klines.parquet",
        ])
        .assert()
        .failure()
        .stderr(contains("limit must be between 1 and 1000"));
}

#[test]
fn binance_paper_tiny_order_requires_explicit_confirmation() {
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "binance-paper-tiny-order",
            "--config",
            "configs/paper/binance_testnet.toml",
            "--symbol",
            "BTCUSDT",
            "--side",
            "buy",
            "--qty",
            "0.001",
            "--price",
            "10000",
        ])
        .assert()
        .failure()
        .stderr(contains("--confirm-testnet-order"));
}

#[test]
fn replay_accepts_config_argument() {
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args(["replay", "--config", "configs/backtest/ma_cross.toml"])
        .assert()
        .success()
        .stdout(contains("replay completed: bars="));
}

#[test]
fn report_requires_explicit_run_id() {
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args(["paper-run", "--config", "configs/backtest/ma_cross.toml"])
        .assert()
        .success();

    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args(["report", "--config", "configs/backtest/ma_cross.toml"])
        .assert()
        .failure()
        .stderr(contains("--run-id"));
}

#[test]
fn report_accepts_explicit_run_id() {
    let config = seed_report_cli_storage();
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "report",
            "--config",
            config.to_str().unwrap(),
            "--run-id",
            "cli-report-b",
        ])
        .assert()
        .success()
        .stdout(contains("report: run_id=cli-report-b"))
        .stdout(contains("snapshots=1"))
        .stdout(contains("cli-report-a").not());

    std::fs::remove_file(config).unwrap();
}

#[test]
fn report_can_export_csv() {
    let output = temp_output("trader-report", "csv");
    run_paper();

    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "report",
            "--config",
            "configs/backtest/ma_cross.toml",
            "--run-id",
            "sample-ma-cross",
            "--format",
            "csv",
            "--output",
            output.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(contains("wrote report"));

    let content = std::fs::read_to_string(&output).unwrap();
    assert!(content.contains("run_id,status,orders,fills,balances,snapshots"));
    assert!(content.contains("sample-ma-cross"));
    std::fs::remove_file(output).unwrap();
}

#[test]
fn report_can_export_html() {
    let output = temp_output("trader-report", "html");
    run_paper();

    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "report",
            "--config",
            "configs/backtest/ma_cross.toml",
            "--run-id",
            "sample-ma-cross",
            "--format",
            "html",
            "--output",
            output.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(contains("wrote report"));

    let content = std::fs::read_to_string(&output).unwrap();
    assert!(content.contains("<h1>Trader Report</h1>"));
    assert!(content.contains("sample-ma-cross"));
    std::fs::remove_file(output).unwrap();
}

#[test]
fn positions_list_prints_contract_positions() {
    let config = seed_contract_cli_storage();

    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "positions",
            "list",
            "--config",
            config.to_str().unwrap(),
            "--run-id",
            "cli-contract-run",
            "--account",
            "paper",
            "--exchange",
            "BINANCE",
        ])
        .assert()
        .success()
        .stdout(contains("crypto_position: run_id=cli-contract-run"))
        .stdout(contains("symbol=BTCUSDT_PERP"))
        .stdout(contains("side=LONG"))
        .stdout(contains("qty=0.25"))
        .stdout(contains("funding_fee=-1.25"));

    std::fs::remove_file(config).unwrap();
}

#[test]
fn funding_list_prints_filtered_funding_rates() {
    let config = seed_contract_cli_storage();

    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "funding",
            "list",
            "--config",
            config.to_str().unwrap(),
            "--exchange",
            "BINANCE",
            "--symbol",
            "BTCUSDT_PERP",
            "--from",
            "0",
            "--to",
            "2000",
        ])
        .assert()
        .success()
        .stdout(contains("funding_rate: exchange=BINANCE"))
        .stdout(contains("symbol=BTCUSDT_PERP"))
        .stdout(contains("funding_time_ms=1000"))
        .stdout(contains("funding_rate=0.0002"))
        .stdout(contains("mark_price=50000"));

    std::fs::remove_file(config).unwrap();
}

#[test]
fn snapshots_cash_prints_filtered_cash_snapshots() {
    let config = seed_contract_cli_storage();

    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "snapshots",
            "cash",
            "--config",
            config.to_str().unwrap(),
            "--run-id",
            "cli-contract-run",
            "--currency",
            "USDT",
            "--from",
            "1000",
            "--to",
            "2000",
        ])
        .assert()
        .success()
        .stdout(contains("cash_snapshot: run_id=cli-contract-run"))
        .stdout(contains("currency=USDT"))
        .stdout(contains("cash=99900"))
        .stdout(contains("ts_ms=1500"));

    std::fs::remove_file(config).unwrap();
}

#[test]
fn snapshots_positions_prints_filtered_position_snapshots() {
    let config = seed_contract_cli_storage();

    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "snapshots",
            "positions",
            "--config",
            config.to_str().unwrap(),
            "--run-id",
            "cli-contract-run",
            "--symbol",
            "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP",
        ])
        .assert()
        .success()
        .stdout(contains("position_snapshot: run_id=cli-contract-run"))
        .stdout(contains("symbol=CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP"))
        .stdout(contains("side="))
        .stdout(contains("qty=0.25"))
        .stdout(contains("avg_price=50000"));

    std::fs::remove_file(config).unwrap();
}

#[test]
fn reconciliation_prints_snapshot_and_drift_status() {
    let config = seed_contract_cli_storage();

    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "reconciliation",
            "--config",
            config.to_str().unwrap(),
            "--run-id",
            "cli-contract-run",
        ])
        .assert()
        .success()
        .stdout(contains("reconciliation: run_id=cli-contract-run"))
        .stdout(contains("status=drift"))
        .stdout(contains("cash_snapshots=1"))
        .stdout(contains("position_snapshots=1"))
        .stdout(contains("drift_events=1"));

    std::fs::remove_file(config).unwrap();
}

#[test]
fn reconciliation_drifts_lists_filtered_audit_rows() {
    let config = seed_contract_cli_storage();

    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "reconciliation-drifts",
            "--config",
            config.to_str().unwrap(),
            "--run-id",
            "cli-contract-run",
            "--account-id",
            "paper",
            "--limit",
            "1",
        ])
        .assert()
        .success()
        .stdout(contains("reconciliation_drift: run_id=cli-contract-run"))
        .stdout(contains("account=paper"))
        .stdout(contains("decision=warn"))
        .stdout(contains("reason=qty mismatch"));

    std::fs::remove_file(config).unwrap();
}

#[test]
fn order_events_lists_filtered_audit_rows() {
    let config = seed_contract_cli_storage();

    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "order-events",
            "--config",
            config.to_str().unwrap(),
            "--run-id",
            "cli-contract-run",
            "--status",
            "FILLED",
            "--event-type",
            "broker.order.recovered",
            "--from",
            "1500",
            "--to",
            "2500",
            "--limit",
            "1",
        ])
        .assert()
        .success()
        .stdout(contains("order_event: run_id=cli-contract-run"))
        .stdout(contains("status=FILLED"))
        .stdout(contains("event_type=broker.order.recovered"))
        .stdout(contains(
            "message=startup recovery matched broker order state",
        ))
        .stdout(predicates::str::contains("order-two").not());

    std::fs::remove_file(config).unwrap();
}

#[test]
fn risk_events_lists_filtered_audit_rows() {
    let config = seed_contract_cli_storage();

    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "risk-events",
            "--config",
            config.to_str().unwrap(),
            "--run-id",
            "cli-contract-run",
            "--risk-type",
            "reconciliation_drift",
            "--decision",
            "warn",
            "--account-id",
            "paper",
            "--from",
            "1400",
            "--to",
            "1600",
            "--limit",
            "1",
        ])
        .assert()
        .success()
        .stdout(contains("risk_event: run_id=cli-contract-run"))
        .stdout(contains("risk_type=reconciliation_drift"))
        .stdout(contains("decision=warn"))
        .stdout(contains("reason=qty mismatch"))
        .stdout(predicates::str::contains("cash guard tripped").not());

    std::fs::remove_file(config).unwrap();
}

#[test]
fn config_lifecycle_commands_print_release_audit_and_run_binding_status() {
    let config = seed_config_lifecycle_cli_storage();

    let mut releases = Command::cargo_bin("trader").unwrap();
    releases
        .current_dir(workspace_root())
        .args([
            "configs",
            "releases",
            "--config",
            config.to_str().unwrap(),
            "--config-id",
            "config-paper",
        ])
        .assert()
        .success()
        .stdout(contains("config_release: config_id=config-paper"))
        .stdout(contains("version=v1"))
        .stdout(contains("status=released"));

    let mut audits = Command::cargo_bin("trader").unwrap();
    audits
        .current_dir(workspace_root())
        .args([
            "configs",
            "audits",
            "--config",
            config.to_str().unwrap(),
            "--config-id",
            "config-paper",
        ])
        .assert()
        .success()
        .stdout(contains("config_audit: config_id=config-paper"))
        .stdout(contains("action=rollback"))
        .stdout(contains("reason=restore previous release"));

    let mut binding = Command::cargo_bin("trader").unwrap();
    binding
        .current_dir(workspace_root())
        .args([
            "runs",
            "config-version",
            "--config",
            config.to_str().unwrap(),
            "--run-id",
            "cli-config-run",
        ])
        .assert()
        .success()
        .stdout(contains("run_config_version: run_id=cli-config-run"))
        .stdout(contains("config_id=config-paper"))
        .stdout(contains("version=v1"));

    std::fs::remove_file(config).unwrap();
}

#[test]
fn cli_backtest_records_run_config_snapshot_binding() {
    let config = write_multi_symbol_cli_config("cli-config-snapshot-backtest", "backtest");

    let mut backtest = Command::cargo_bin("trader").unwrap();
    backtest
        .current_dir(workspace_root())
        .args(["backtest", "--config", config.to_str().unwrap()])
        .assert()
        .success()
        .stdout(contains("backtest completed"));

    let mut binding = Command::cargo_bin("trader").unwrap();
    binding
        .current_dir(workspace_root())
        .args([
            "runs",
            "config-version",
            "--config",
            config.to_str().unwrap(),
            "--run-id",
            "cli-config-snapshot-backtest",
        ])
        .assert()
        .success()
        .stdout(contains(
            "run_config_version: run_id=cli-config-snapshot-backtest",
        ))
        .stdout(contains("config_id=run:cli-config-snapshot-backtest"))
        .stdout(contains("version=fnv1a64:"));

    std::fs::remove_file(config).unwrap();
}

#[test]
fn cli_backtest_config_snapshot_includes_run_spec_contract() {
    let config = write_multi_symbol_cli_config("cli-run-spec-snapshot-backtest", "backtest");

    let mut backtest = Command::cargo_bin("trader").unwrap();
    backtest
        .current_dir(workspace_root())
        .args(["backtest", "--config", config.to_str().unwrap()])
        .assert()
        .success()
        .stdout(contains("backtest completed"));

    let app_config = config::AppConfig::from_toml_file(config.to_str().unwrap()).unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let snapshot = runtime.block_on(async {
        let db = storage::Db::connect(&app_config.database.url)
            .await
            .unwrap();
        db.get_config_by_name("cli-run-spec-snapshot-backtest")
            .await
            .unwrap()
            .unwrap()
    });
    assert_eq!(snapshot.id, "run:cli-run-spec-snapshot-backtest");
    assert_eq!(snapshot.format, "JSON");
    assert!(snapshot.content.contains("\"run_spec\""));
    assert!(
        snapshot
            .content
            .contains("\"run_id\":\"cli-run-spec-snapshot-backtest\"")
    );
    assert!(snapshot.content.contains("\"mode\":\"backtest\""));

    std::fs::remove_file(config).unwrap();
}

#[test]
fn cli_paper_and_replay_record_run_config_snapshot_bindings() {
    for (command_name, run_id, config, completion) in [
        (
            "paper-run",
            "cli-config-snapshot-paper",
            write_multi_symbol_cli_config("cli-config-snapshot-paper", "paper"),
            "paper completed",
        ),
        (
            "replay",
            "cli-config-snapshot-replay",
            write_replay_cli_config("cli-config-snapshot-replay"),
            "replay completed",
        ),
    ] {
        let mut command = Command::cargo_bin("trader").unwrap();
        command
            .current_dir(workspace_root())
            .args([command_name, "--config", config.to_str().unwrap()])
            .assert()
            .success()
            .stdout(contains(completion));

        let mut binding = Command::cargo_bin("trader").unwrap();
        binding
            .current_dir(workspace_root())
            .args([
                "runs",
                "config-version",
                "--config",
                config.to_str().unwrap(),
                "--run-id",
                run_id,
            ])
            .assert()
            .success()
            .stdout(contains(format!("run_config_version: run_id={run_id}")))
            .stdout(contains(format!("config_id=run:{run_id}")))
            .stdout(contains("version=fnv1a64:"));

        std::fs::remove_file(config).unwrap();
    }
}

#[test]
fn config_management_commands_create_transition_show_diff_and_rollback() {
    let config = seed_config_management_cli_storage();
    let version_one = temp_output("trader-cli-config-v1", "json");
    let version_two = temp_output("trader-cli-config-v2", "json");
    std::fs::write(
        &version_one,
        r#"{"enabled":true,"risk":{"max_order_notional":"1000","symbols":["BTCUSDT"]}}"#,
    )
    .unwrap();
    std::fs::write(
        &version_two,
        r#"{"enabled":true,"risk":{"max_order_notional":"1500","max_position":"2"}}"#,
    )
    .unwrap();

    let mut create_one = Command::cargo_bin("trader").unwrap();
    create_one
        .current_dir(workspace_root())
        .args([
            "configs",
            "create",
            "--config",
            config.to_str().unwrap(),
            "--name",
            "paper-risk",
            "--file",
            version_one.to_str().unwrap(),
            "--created-by",
            "ops",
            "--ts-ms",
            "100",
        ])
        .assert()
        .success()
        .stdout(contains("config_version: name=paper-risk"))
        .stdout(contains("version=1"))
        .stdout(contains("state=draft"));

    let mut create_two = Command::cargo_bin("trader").unwrap();
    create_two
        .current_dir(workspace_root())
        .args([
            "configs",
            "create",
            "--config",
            config.to_str().unwrap(),
            "--name",
            "paper-risk",
            "--file",
            version_two.to_str().unwrap(),
            "--created-by",
            "ops",
            "--parent-version",
            "1",
            "--ts-ms",
            "200",
        ])
        .assert()
        .success()
        .stdout(contains("version=2"))
        .stdout(contains("parent_version=1"));

    let mut list = Command::cargo_bin("trader").unwrap();
    list.current_dir(workspace_root())
        .args([
            "configs",
            "list",
            "--config",
            config.to_str().unwrap(),
            "--name",
            "paper-risk",
        ])
        .assert()
        .success()
        .stdout(contains("config_version: name=paper-risk version=1"))
        .stdout(contains("config_version: name=paper-risk version=2"));

    for (subcommand, expected_state, ts_ms) in [
        ("submit-review", "pending_review", "300"),
        ("approve", "approved", "400"),
        ("publish", "published", "500"),
    ] {
        let mut transition = Command::cargo_bin("trader").unwrap();
        transition
            .current_dir(workspace_root())
            .args([
                "configs",
                subcommand,
                "--config",
                config.to_str().unwrap(),
                "--name",
                "paper-risk",
                "--version",
                "1",
                "--changed-by",
                "ops",
                "--reason",
                expected_state,
                "--ts-ms",
                ts_ms,
            ])
            .assert()
            .success()
            .stdout(contains("config_version: name=paper-risk version=1"))
            .stdout(contains(format!("state={expected_state}")));
    }

    let mut show_published = Command::cargo_bin("trader").unwrap();
    show_published
        .current_dir(workspace_root())
        .args([
            "configs",
            "show",
            "--config",
            config.to_str().unwrap(),
            "--name",
            "paper-risk",
            "--published",
        ])
        .assert()
        .success()
        .stdout(contains("state=published"))
        .stdout(contains(r#""max_order_notional":"1000""#));

    let mut diff = Command::cargo_bin("trader").unwrap();
    diff.current_dir(workspace_root())
        .args([
            "configs",
            "diff",
            "--config",
            config.to_str().unwrap(),
            "--name",
            "paper-risk",
            "--v1",
            "1",
            "--v2",
            "2",
        ])
        .assert()
        .success()
        .stdout(contains("config_diff: name=paper-risk v1=1 v2=2"))
        .stdout(contains("changed=1"))
        .stdout(contains("removed=1"))
        .stdout(contains("added=1"))
        .stdout(contains(
            "config_diff_changed: path=risk.max_order_notional",
        ));

    let mut rollback = Command::cargo_bin("trader").unwrap();
    rollback
        .current_dir(workspace_root())
        .args([
            "configs",
            "rollback",
            "--config",
            config.to_str().unwrap(),
            "--name",
            "paper-risk",
            "--version",
            "1",
            "--actor",
            "ops",
            "--reason",
            "restore",
            "--ts-ms",
            "600",
        ])
        .assert()
        .success()
        .stdout(contains("config_version: name=paper-risk version=3"))
        .stdout(contains("state=draft"))
        .stdout(contains("parent_version=1"));

    std::fs::remove_file(config).unwrap();
    std::fs::remove_file(version_one).unwrap();
    std::fs::remove_file(version_two).unwrap();
}

#[test]
fn config_management_commands_enforce_production_governance() {
    let config = seed_config_management_cli_storage();
    let version_one = temp_output("trader-cli-prod-config-v1", "json");
    std::fs::write(&version_one, r#"{"risk":{"max_order_notional":"1000"}}"#).unwrap();

    let mut create = Command::cargo_bin("trader").unwrap();
    create
        .current_dir(workspace_root())
        .args([
            "configs",
            "create",
            "--config",
            config.to_str().unwrap(),
            "--name",
            "prod-risk",
            "--file",
            version_one.to_str().unwrap(),
            "--created-by",
            "release",
            "--target-env",
            "production",
            "--rollout",
            "canary",
            "--ts-ms",
            "100",
        ])
        .assert()
        .success()
        .stdout(contains("config_governance: name=prod-risk version=1"))
        .stdout(contains("target_env=production"))
        .stdout(contains("rollout=canary"));

    for (subcommand, actor, ts_ms) in [
        ("submit-review", "release", "200"),
        ("approve", "release", "300"),
    ] {
        let mut transition = Command::cargo_bin("trader").unwrap();
        transition
            .current_dir(workspace_root())
            .args([
                "configs",
                subcommand,
                "--config",
                config.to_str().unwrap(),
                "--name",
                "prod-risk",
                "--version",
                "1",
                "--changed-by",
                actor,
                "--reason",
                subcommand,
                "--ts-ms",
                ts_ms,
            ])
            .assert()
            .success();
    }

    let mut self_publish = Command::cargo_bin("trader").unwrap();
    self_publish
        .current_dir(workspace_root())
        .args([
            "configs",
            "publish",
            "--config",
            config.to_str().unwrap(),
            "--name",
            "prod-risk",
            "--version",
            "1",
            "--changed-by",
            "release",
            "--reason",
            "publish",
            "--ts-ms",
            "400",
        ])
        .assert()
        .failure()
        .stderr(contains(
            "production config publish requires independent approver",
        ));

    let mut independent_approve = Command::cargo_bin("trader").unwrap();
    independent_approve
        .current_dir(workspace_root())
        .args([
            "configs",
            "approve",
            "--config",
            config.to_str().unwrap(),
            "--name",
            "prod-risk",
            "--version",
            "1",
            "--changed-by",
            "risk-owner",
            "--reason",
            "independent",
            "--ts-ms",
            "500",
        ])
        .assert()
        .success()
        .stdout(contains("approved_by=risk-owner"));

    let mut publish = Command::cargo_bin("trader").unwrap();
    publish
        .current_dir(workspace_root())
        .args([
            "configs",
            "publish",
            "--config",
            config.to_str().unwrap(),
            "--name",
            "prod-risk",
            "--version",
            "1",
            "--changed-by",
            "release",
            "--reason",
            "publish",
            "--ts-ms",
            "600",
        ])
        .assert()
        .success()
        .stdout(contains("state=published"))
        .stdout(contains("approved_by=risk-owner"))
        .stdout(contains("published_by=release"));

    std::fs::remove_file(config).unwrap();
    std::fs::remove_file(version_one).unwrap();
}

#[test]
fn config_management_commands_enforce_roles_and_print_pending_approvals() {
    let config = seed_config_management_cli_storage();
    let version_one = temp_output("trader-cli-prod-queue-config-v1", "json");
    std::fs::write(&version_one, r#"{"risk":{"max_order_notional":"1000"}}"#).unwrap();

    let mut create = Command::cargo_bin("trader").unwrap();
    create
        .current_dir(workspace_root())
        .args([
            "configs",
            "create",
            "--config",
            config.to_str().unwrap(),
            "--name",
            "prod-queue",
            "--file",
            version_one.to_str().unwrap(),
            "--created-by",
            "release",
            "--target-env",
            "production",
            "--rollout",
            "canary",
            "--ts-ms",
            "100",
        ])
        .assert()
        .success();

    let mut unauthorized_submit = Command::cargo_bin("trader").unwrap();
    unauthorized_submit
        .current_dir(workspace_root())
        .args([
            "configs",
            "submit-review",
            "--config",
            config.to_str().unwrap(),
            "--name",
            "prod-queue",
            "--version",
            "1",
            "--changed-by",
            "trader",
            "--actor-role",
            "viewer",
            "--reason",
            "request approval",
            "--ts-ms",
            "200",
        ])
        .assert()
        .failure()
        .stderr(contains(
            "production config pending_review requires role release_manager",
        ));

    let mut submit = Command::cargo_bin("trader").unwrap();
    submit
        .current_dir(workspace_root())
        .args([
            "configs",
            "submit-review",
            "--config",
            config.to_str().unwrap(),
            "--name",
            "prod-queue",
            "--version",
            "1",
            "--changed-by",
            "release",
            "--actor-role",
            "release_manager",
            "--reason",
            "request approval",
            "--ts-ms",
            "300",
        ])
        .assert()
        .success()
        .stdout(contains("state=pending_review"));

    let mut pending = Command::cargo_bin("trader").unwrap();
    pending
        .current_dir(workspace_root())
        .args([
            "configs",
            "pending-approvals",
            "--config",
            config.to_str().unwrap(),
            "--target-env",
            "production",
        ])
        .assert()
        .success()
        .stdout(contains("config_approval: name=prod-queue version=1"))
        .stdout(contains("target_env=production"))
        .stdout(contains("state=pending_review"));

    let mut unauthorized_approve = Command::cargo_bin("trader").unwrap();
    unauthorized_approve
        .current_dir(workspace_root())
        .args([
            "configs",
            "approve",
            "--config",
            config.to_str().unwrap(),
            "--name",
            "prod-queue",
            "--version",
            "1",
            "--changed-by",
            "release",
            "--actor-role",
            "release_manager",
            "--reason",
            "approve",
            "--ts-ms",
            "400",
        ])
        .assert()
        .failure()
        .stderr(contains(
            "production config approved requires role approver",
        ));

    let mut approve = Command::cargo_bin("trader").unwrap();
    approve
        .current_dir(workspace_root())
        .args([
            "configs",
            "approve",
            "--config",
            config.to_str().unwrap(),
            "--name",
            "prod-queue",
            "--version",
            "1",
            "--changed-by",
            "risk-owner",
            "--actor-role",
            "approver",
            "--reason",
            "risk approval",
            "--ts-ms",
            "500",
        ])
        .assert()
        .success()
        .stdout(contains("approved_by=risk-owner"));

    let mut publish = Command::cargo_bin("trader").unwrap();
    publish
        .current_dir(workspace_root())
        .args([
            "configs",
            "publish",
            "--config",
            config.to_str().unwrap(),
            "--name",
            "prod-queue",
            "--version",
            "1",
            "--changed-by",
            "release",
            "--actor-role",
            "release_manager",
            "--reason",
            "publish",
            "--ts-ms",
            "600",
        ])
        .assert()
        .success()
        .stdout(contains("state=published"))
        .stdout(contains("published_by=release"));

    std::fs::remove_file(config).unwrap();
    std::fs::remove_file(version_one).unwrap();
}

#[test]
fn config_management_commands_enforce_staging_roles_and_print_pending_approvals() {
    let config = seed_config_management_cli_storage();
    let version_one = temp_output("trader-cli-staging-queue-config-v1", "json");
    std::fs::write(&version_one, r#"{"risk":{"max_order_notional":"1000"}}"#).unwrap();

    let mut create = Command::cargo_bin("trader").unwrap();
    create
        .current_dir(workspace_root())
        .args([
            "configs",
            "create",
            "--config",
            config.to_str().unwrap(),
            "--name",
            "staging-queue",
            "--file",
            version_one.to_str().unwrap(),
            "--created-by",
            "release",
            "--target-env",
            "staging",
            "--rollout",
            "canary",
            "--ts-ms",
            "100",
        ])
        .assert()
        .success();

    let mut unauthorized_submit = Command::cargo_bin("trader").unwrap();
    unauthorized_submit
        .current_dir(workspace_root())
        .args([
            "configs",
            "submit-review",
            "--config",
            config.to_str().unwrap(),
            "--name",
            "staging-queue",
            "--version",
            "1",
            "--changed-by",
            "trader",
            "--actor-role",
            "viewer",
            "--reason",
            "request approval",
            "--ts-ms",
            "200",
        ])
        .assert()
        .failure()
        .stderr(contains(
            "staging config pending_review requires role release_manager",
        ));

    let mut submit = Command::cargo_bin("trader").unwrap();
    submit
        .current_dir(workspace_root())
        .args([
            "configs",
            "submit-review",
            "--config",
            config.to_str().unwrap(),
            "--name",
            "staging-queue",
            "--version",
            "1",
            "--changed-by",
            "release",
            "--actor-role",
            "release_manager",
            "--reason",
            "request approval",
            "--ts-ms",
            "300",
        ])
        .assert()
        .success()
        .stdout(contains("state=pending_review"));

    let mut pending = Command::cargo_bin("trader").unwrap();
    pending
        .current_dir(workspace_root())
        .args([
            "configs",
            "pending-approvals",
            "--config",
            config.to_str().unwrap(),
            "--target-env",
            "staging",
        ])
        .assert()
        .success()
        .stdout(contains("config_approval: name=staging-queue version=1"))
        .stdout(contains("target_env=staging"))
        .stdout(contains("state=pending_review"));

    let mut unauthorized_approve = Command::cargo_bin("trader").unwrap();
    unauthorized_approve
        .current_dir(workspace_root())
        .args([
            "configs",
            "approve",
            "--config",
            config.to_str().unwrap(),
            "--name",
            "staging-queue",
            "--version",
            "1",
            "--changed-by",
            "release",
            "--actor-role",
            "release_manager",
            "--reason",
            "approve",
            "--ts-ms",
            "400",
        ])
        .assert()
        .failure()
        .stderr(contains("staging config approved requires role approver"));

    let mut approve = Command::cargo_bin("trader").unwrap();
    approve
        .current_dir(workspace_root())
        .args([
            "configs",
            "approve",
            "--config",
            config.to_str().unwrap(),
            "--name",
            "staging-queue",
            "--version",
            "1",
            "--changed-by",
            "qa-owner",
            "--actor-role",
            "approver",
            "--reason",
            "qa approval",
            "--ts-ms",
            "500",
        ])
        .assert()
        .success()
        .stdout(contains("approved_by=qa-owner"));

    let mut unauthorized_publish = Command::cargo_bin("trader").unwrap();
    unauthorized_publish
        .current_dir(workspace_root())
        .args([
            "configs",
            "publish",
            "--config",
            config.to_str().unwrap(),
            "--name",
            "staging-queue",
            "--version",
            "1",
            "--changed-by",
            "qa-owner",
            "--actor-role",
            "approver",
            "--reason",
            "publish",
            "--ts-ms",
            "600",
        ])
        .assert()
        .failure()
        .stderr(contains(
            "staging config published requires role release_manager",
        ));

    let mut publish = Command::cargo_bin("trader").unwrap();
    publish
        .current_dir(workspace_root())
        .args([
            "configs",
            "publish",
            "--config",
            config.to_str().unwrap(),
            "--name",
            "staging-queue",
            "--version",
            "1",
            "--changed-by",
            "release",
            "--actor-role",
            "release_manager",
            "--reason",
            "publish",
            "--ts-ms",
            "700",
        ])
        .assert()
        .success()
        .stdout(contains("state=published"))
        .stdout(contains("approved_by=qa-owner"))
        .stdout(contains("published_by=release"));

    std::fs::remove_file(config).unwrap();
    std::fs::remove_file(version_one).unwrap();
}

#[test]
fn logs_commands_filter_and_purge_system_logs() {
    let config = seed_logs_cli_storage();
    let export = temp_output("trader-cli-logs-export", "jsonl");

    let mut list = Command::cargo_bin("trader").unwrap();
    list.current_dir(workspace_root())
        .args([
            "logs",
            "list",
            "--config",
            config.to_str().unwrap(),
            "--run-id",
            "cli-logs-run",
            "--level",
            "ERROR",
            "--target",
            "runtime.execution",
            "--from",
            "100",
            "--to",
            "300",
        ])
        .assert()
        .success()
        .stdout(contains("system_log: run_id=cli-logs-run"))
        .stdout(contains("level=ERROR"))
        .stdout(contains("target=runtime.execution"))
        .stdout(contains("message=execution failed"));

    let mut export_command = Command::cargo_bin("trader").unwrap();
    export_command
        .current_dir(workspace_root())
        .args([
            "logs",
            "export",
            "--config",
            config.to_str().unwrap(),
            "--output",
            export.to_str().unwrap(),
            "--run-id",
            "cli-logs-run",
            "--level",
            "ERROR",
            "--target",
            "runtime.execution",
            "--from",
            "100",
            "--to",
            "300",
        ])
        .assert()
        .success()
        .stdout(contains("system_logs_exported: count=1"));
    let exported = std::fs::read_to_string(&export).unwrap();
    assert!(exported.contains("\"run_id\":\"cli-logs-run\""));
    assert!(exported.contains("\"level\":\"ERROR\""));
    assert!(exported.contains("\"target\":\"runtime.execution\""));
    assert!(exported.contains("\"message\":\"execution failed\""));
    assert!(exported.contains("\"fields\":{\"category\":\"runtime\"}"));

    let mut purge = Command::cargo_bin("trader").unwrap();
    purge
        .current_dir(workspace_root())
        .args([
            "logs",
            "purge",
            "--config",
            config.to_str().unwrap(),
            "--before",
            "150",
            "--target",
            "runtime.execution",
            "--run-id",
            "cli-logs-run",
        ])
        .assert()
        .success()
        .stdout(contains("system_logs_purged: count=1"));

    std::fs::remove_file(export).unwrap();
    std::fs::remove_file(config).unwrap();
}

#[test]
fn logs_metrics_prints_drop_counter_and_writer_config() {
    let config = seed_logs_cli_storage();

    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args(["logs", "metrics", "--config", config.to_str().unwrap()])
        .assert()
        .success()
        .stdout(contains("logging_metrics: dropped_logs=0"))
        .stdout(contains("enabled=true"))
        .stdout(contains("buffer_size=1000"))
        .stdout(contains("flush_interval_ms=5000"));

    std::fs::remove_file(config).unwrap();
}

#[test]
fn logs_ship_posts_filtered_system_logs_to_collector() {
    let config = seed_logs_cli_storage();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = Vec::new();
        let mut chunk = [0u8; 1024];
        let mut content_length = None;
        loop {
            let size = std::io::Read::read(&mut stream, &mut chunk).unwrap();
            if size == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..size]);
            if content_length.is_none()
                && let Some(headers_end) = buf.windows(4).position(|window| window == b"\r\n\r\n")
            {
                let headers = String::from_utf8_lossy(&buf[..headers_end + 4]);
                content_length = headers
                    .lines()
                    .find_map(|line| {
                        line.strip_prefix("content-length: ")
                            .or_else(|| line.strip_prefix("Content-Length: "))
                    })
                    .and_then(|value| value.trim().parse::<usize>().ok());
            }
            if let Some(expected) = content_length
                && let Some(headers_end) = buf.windows(4).position(|window| window == b"\r\n\r\n")
            {
                let body_len = buf.len().saturating_sub(headers_end + 4);
                if body_len >= expected {
                    break;
                }
            }
        }
        let request = String::from_utf8_lossy(&buf).to_string();
        std::io::Write::write_all(
            &mut stream,
            b"HTTP/1.1 202 Accepted\r\ncontent-length: 8\r\ncontent-type: text/plain\r\n\r\naccepted",
        )
        .unwrap();
        request
    });

    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .env("TRADER_LOG_SHIP_SECRET", "ship-secret")
        .args([
            "logs",
            "ship",
            "--config",
            config.to_str().unwrap(),
            "--collector-url",
            &format!("http://{addr}/logs"),
            "--run-id",
            "cli-logs-run",
            "--level",
            "ERROR",
            "--search",
            "failed",
            "--bearer-token",
            "ship-token",
            "--signature-secret-env",
            "TRADER_LOG_SHIP_SECRET",
        ])
        .assert()
        .success()
        .stdout(contains("system_logs_shipped: count=1 status=202"));

    let request = server.join().unwrap();
    assert!(request.starts_with("POST /logs HTTP/1.1"));
    assert!(
        request.contains("authorization: Bearer ship-token")
            || request.contains("Authorization: Bearer ship-token")
    );
    assert!(
        request.contains("content-type: application/x-ndjson")
            || request.contains("Content-Type: application/x-ndjson")
    );
    let timestamp = http_header_value(&request, "x-trader-log-timestamp")
        .expect("missing X-Trader-Log-Timestamp header");
    let signature = http_header_value(&request, "x-trader-log-signature")
        .expect("missing X-Trader-Log-Signature header");
    let body = http_body(&request);
    assert_eq!(
        signature,
        format!(
            "v1={}",
            test_log_ship_signature("ship-secret", &timestamp, body)
        )
    );
    assert!(request.contains("\"run_id\":\"cli-logs-run\""));
    assert!(request.contains("\"level\":\"ERROR\""));
    assert!(request.contains("\"target\":\"runtime.execution\""));
    assert!(request.contains("\"message\":\"execution failed\""));
    assert!(request.contains("\"fields\":{\"category\":\"runtime\"}"));
    assert!(!request.contains("execution started"));

    std::fs::remove_file(config).unwrap();
}

#[test]
fn logs_ship_retries_retryable_collector_failures() {
    let config = seed_logs_cli_storage();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let mut requests = Vec::new();
        for response in [
            b"HTTP/1.1 500 Internal Server Error\r\ncontent-length: 5\r\n\r\nerror".as_slice(),
            b"HTTP/1.1 202 Accepted\r\ncontent-length: 8\r\n\r\naccepted".as_slice(),
        ] {
            let (mut stream, _) = listener.accept().unwrap();
            requests.push(read_http_request(&mut stream));
            std::io::Write::write_all(&mut stream, response).unwrap();
        }
        requests
    });

    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "logs",
            "ship",
            "--config",
            config.to_str().unwrap(),
            "--collector-url",
            &format!("http://{addr}/logs"),
            "--run-id",
            "cli-logs-run",
            "--level",
            "ERROR",
            "--search",
            "failed",
            "--max-retries",
            "1",
            "--retry-backoff-ms",
            "1",
        ])
        .assert()
        .success()
        .stdout(contains(
            "system_logs_shipped: count=1 status=202 attempts=2",
        ));

    let requests = server.join().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests.iter().all(|request| {
        request.contains("\"message\":\"execution failed\"")
            && !request.contains("execution started")
    }));

    std::fs::remove_file(config).unwrap();
}

#[test]
fn logs_commands_support_search_count_and_tail() {
    let config = seed_logs_cli_storage();

    let mut list = Command::cargo_bin("trader").unwrap();
    list.current_dir(workspace_root())
        .args([
            "logs",
            "list",
            "--config",
            config.to_str().unwrap(),
            "--run-id",
            "cli-logs-run",
            "--search",
            "failed",
            "--limit",
            "1",
            "--offset",
            "0",
        ])
        .assert()
        .success()
        .stdout(contains("message=execution failed"))
        .stdout(predicates::str::contains("execution started").not());

    let mut count = Command::cargo_bin("trader").unwrap();
    count
        .current_dir(workspace_root())
        .args([
            "logs",
            "count",
            "--config",
            config.to_str().unwrap(),
            "--run-id",
            "cli-logs-run",
            "--search",
            "execution",
        ])
        .assert()
        .success()
        .stdout(contains("system_logs_count: count=2"));

    let mut tail = Command::cargo_bin("trader").unwrap();
    tail.current_dir(workspace_root())
        .args([
            "logs",
            "tail",
            "--config",
            config.to_str().unwrap(),
            "--run-id",
            "cli-logs-run",
            "--poll-interval-ms",
            "1",
            "--max-polls",
            "1",
        ])
        .assert()
        .success()
        .stdout(contains("system_log: run_id=cli-logs-run"))
        .stdout(contains("message=execution started"))
        .stdout(contains("message=execution failed"));

    std::fs::remove_file(config).unwrap();
}

#[test]
fn reconciliation_alerts_summary_reports_runtime_alert_aggregate() {
    let config = seed_reconciliation_alerts_cli_storage();
    let export = temp_output("trader-cli-reconciliation-alerts-export", "jsonl");

    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "reconciliation-alerts-summary",
            "--config",
            config.to_str().unwrap(),
            "--account-id",
            "paper",
        ])
        .assert()
        .success()
        .stdout(contains("reconciliation_alert_summary: run_id=*"))
        .stdout(contains("alert_count=2"))
        .stdout(contains("runs=cli-alert-a,cli-alert-b"))
        .stdout(contains("reasons=cash_total_drift,position_qty_drift"));

    let mut export_command = Command::cargo_bin("trader").unwrap();
    export_command
        .current_dir(workspace_root())
        .args([
            "reconciliation-alerts-export",
            "--config",
            config.to_str().unwrap(),
            "--output",
            export.to_str().unwrap(),
            "--account-id",
            "paper",
        ])
        .assert()
        .success()
        .stdout(contains("reconciliation_alerts_exported: count=2"));
    let exported = std::fs::read_to_string(&export).unwrap();
    assert!(exported.contains("\"message\":\"reconciliation_drift.alert\""));
    assert!(exported.contains("\"account_id\":\"paper\""));
    assert!(exported.contains("\"dedup_key\":\"reconciliation_drift.alert|cli-alert-a|paper|CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP|position_qty_drift\""));
    assert!(exported.contains("\"reason\":\"cash_total_drift\""));

    std::fs::remove_file(export).unwrap();
    std::fs::remove_file(config).unwrap();
}

#[test]
fn reconciliation_alert_delivery_summary_reports_delivery_aggregate() {
    let config = seed_reconciliation_alert_delivery_cli_storage();
    let export = temp_output("trader-cli-reconciliation-alert-deliveries-export", "jsonl");

    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "reconciliation-alert-deliveries-summary",
            "--config",
            config.to_str().unwrap(),
            "--account-id",
            "paper",
        ])
        .assert()
        .success()
        .stdout(contains("reconciliation_alert_delivery_summary: run_id=*"))
        .stdout(contains("delivery_count=2"))
        .stdout(contains("sent_count=1"))
        .stdout(contains("failed_count=1"))
        .stdout(contains("sinks=file,webhook"))
        .stdout(contains("statuses=failed,sent"));

    let mut export_command = Command::cargo_bin("trader").unwrap();
    export_command
        .current_dir(workspace_root())
        .args([
            "reconciliation-alert-deliveries-export",
            "--config",
            config.to_str().unwrap(),
            "--output",
            export.to_str().unwrap(),
            "--account-id",
            "paper",
        ])
        .assert()
        .success()
        .stdout(contains(
            "reconciliation_alert_deliveries_exported: count=2",
        ));
    let exported = std::fs::read_to_string(&export).unwrap();
    assert!(exported.contains("\"message\":\"alert.delivery\""));
    assert!(exported.contains("\"status\":\"failed\""));
    assert!(exported.contains("\"sink\":\"webhook\""));
    assert!(exported.contains("\"http_status\":500"));
    assert!(exported.contains("\"attempts\":1"));

    std::fs::remove_file(export).unwrap();
    std::fs::remove_file(config).unwrap();
}

#[test]
fn reconciliation_alert_redeliver_posts_failed_alert_to_webhook() {
    let config = seed_reconciliation_alert_delivery_cli_storage();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = Vec::new();
        let mut chunk = [0u8; 1024];
        let mut content_length = None;
        loop {
            let size = std::io::Read::read(&mut stream, &mut chunk).unwrap();
            if size == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..size]);
            if content_length.is_none()
                && let Some(headers_end) = buf.windows(4).position(|window| window == b"\r\n\r\n")
            {
                let headers = String::from_utf8_lossy(&buf[..headers_end + 4]);
                content_length = headers
                    .lines()
                    .find_map(|line| {
                        line.strip_prefix("content-length: ")
                            .or_else(|| line.strip_prefix("Content-Length: "))
                    })
                    .and_then(|value| value.trim().parse::<usize>().ok());
            }
            if let Some(expected) = content_length
                && let Some(headers_end) = buf.windows(4).position(|window| window == b"\r\n\r\n")
            {
                let body_len = buf.len().saturating_sub(headers_end + 4);
                if body_len >= expected {
                    break;
                }
            }
        }
        let request = String::from_utf8_lossy(&buf).to_string();
        std::io::Write::write_all(
            &mut stream,
            b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\ncontent-type: text/plain\r\n\r\nok",
        )
        .unwrap();
        request
    });

    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args([
            "reconciliation-alert-redeliver",
            "--config",
            config.to_str().unwrap(),
            "--webhook-url",
            &format!("http://{addr}/alerts"),
            "--account-id",
            "paper",
        ])
        .assert()
        .success()
        .stdout(contains("reconciliation_alerts_redelivered: count=1"));

    let request = server.join().unwrap();
    assert!(request.starts_with("POST /alerts HTTP/1.1"));
    assert!(request.contains("\"message\":\"reconciliation_drift.alert\""));
    assert!(request.contains("\"dedup_key\":\"reconciliation_drift.alert|cli-delivery-a|paper|CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP|position_qty_drift\""));

    std::fs::remove_file(config).unwrap();
}

#[test]
fn ingest_status_shows_last_fetch_time() {
    let config = seed_ingestion_cli_storage();
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args(["ingest", "status", "--config", config.to_str().unwrap()])
        .assert()
        .success()
        .stdout(contains("ingestion_status: source=binance"))
        .stdout(contains("table=funding_rates"))
        .stdout(contains("rows_fetched=3"))
        .stdout(contains("rows_upserted=2"));

    std::fs::remove_file(config).unwrap();
}

fn run_paper() {
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args(["paper-run", "--config", "configs/backtest/ma_cross.toml"])
        .assert()
        .success();
}

fn temp_output(prefix: &str, extension: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "{}-{}.{}",
        prefix,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        extension
    ))
}

fn write_live_worker_gate_launch(
    prefix: &str,
    broker_mode: &str,
    gate_enabled: bool,
    required_accounts: &[&str],
    max_audit_age_ms: i64,
) -> (PathBuf, PathBuf) {
    let db_path = temp_output(prefix, "sqlite");
    let launch_path = temp_output(prefix, "json");
    let db_url = format!("sqlite://{}", toml_path(&db_path));
    let required_accounts = required_accounts
        .iter()
        .map(|account| format!(r#""{account}""#))
        .collect::<Vec<_>>()
        .join(", ");
    let config_content = format!(
        r#"
        [runtime]
        mode = "live"
        run_id = "{prefix}"

        [database]
        url = "{db_url}"

        [data]
        source = "csv"
        path = "datasets/sample/aapl_1d.csv"

        [strategy]
        name = "moving_average_cross"
        symbols = ["US:NASDAQ:AAPL:EQUITY"]
        fast_window = 2
        slow_window = 3

        [portfolio]
        initial_cash = "25000"
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
        mode = "{broker_mode}"

        [paper]
        account_id = "paper"
        slippage_bps = "25"
        fee_bps = "10"

        [live]
        enabled = true

        [live.reconciliation_gate]
        enabled = {gate_enabled}
        min_successful_audits = 1
        max_audit_age_ms = {max_audit_age_ms}
        required_accounts = [{required_accounts}]
        "#
    );
    let launch = serde_json::json!({
        "run_id": prefix,
        "db_url": db_url,
        "config_path": null,
        "config_content": config_content,
        "config_format": "TOML",
        "run_spec": null,
        "broker_snapshot_interval_ms": null,
        "startup_recovery_unmatched_open_orders_policy": "Fail"
    });
    std::fs::write(&launch_path, serde_json::to_vec(&launch).unwrap()).unwrap();
    (launch_path, db_path)
}

fn seed_live_worker_gate_audits(db_path: &std::path::Path, audits: &[(&str, &str, &str, i64)]) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let db = storage::Db::connect(&format!("sqlite://{}", toml_path(db_path)))
            .await
            .unwrap();
        db.migrate().await.unwrap();
        db.start_strategy_run(storage::StrategyRunStartCommand {
            run_id: "live-worker-gate-seed".to_string(),
            name: "live-worker-gate".to_string(),
            mode: "live".to_string(),
            started_at_ms: 1,
            config: serde_json::json!({}),
        })
        .await
        .unwrap();
        for (id, broker_kind, account_id, ts_ms) in audits {
            db.record_reconciliation_audit(storage::ReconciliationAuditCommand {
                id: (*id).to_string(),
                run_id: "live-worker-gate-seed".to_string(),
                account_id: (*account_id).to_string(),
                broker_kind: (*broker_kind).to_string(),
                ts_ms: *ts_ms,
                severity: "info".to_string(),
                cash_drift_count: 0,
                position_drift_count: 0,
                open_order_drift_count: 0,
                execution_drift_count: 0,
                stale_input_count: 0,
                payload_json: "{}".to_string(),
            })
            .await
            .unwrap();
        }
    });
}

fn live_worker_gate_system_logs(
    db_path: &std::path::Path,
    run_id: &str,
) -> Vec<storage::StoredSystemLog> {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let db = storage::Db::connect(&format!("sqlite://{}", toml_path(db_path)))
            .await
            .unwrap();
        db.list_system_logs_filtered(storage::SystemLogFilter {
            run_id: Some(run_id.to_string()),
            target: Some("runtime.reconciliation_gate".to_string()),
            ..storage::SystemLogFilter::default()
        })
        .await
        .unwrap()
    })
}

fn system_log_fields(log: &storage::StoredSystemLog) -> serde_json::Value {
    serde_json::from_str(log.fields_json.as_deref().unwrap()).unwrap()
}

fn write_ibkr_cli_config(port: u16, account_id: &str, symbol: &str) -> PathBuf {
    write_ibkr_cli_config_with_order_submit(port, account_id, symbol, false)
}

fn write_ibkr_cli_config_with_order_submit(
    port: u16,
    account_id: &str,
    symbol: &str,
    order_submit_enabled: bool,
) -> PathBuf {
    let config = temp_output("trader-ibkr-cli", "toml");
    std::fs::write(
        &config,
        format!(
            r#"
            [runtime]
            mode = "paper"
            run_id = "ibkr-cli-test"

            [database]
            url = "sqlite::memory:"

            [data]
            source = "parquet"
            path = "datasets/ibkr/aapl_1d.parquet"

            [strategy]
            name = "moving_average_cross"
            symbols = ["{symbol}"]
            fast_window = 2
            slow_window = 3

            [portfolio]
            initial_cash = "100000"
            base_currency = "USD"
            order_qty = "1"
            max_abs_qty = "100"

            [risk]
            max_order_notional = "1000"
            min_cash_after_order = "1000"
            max_exposure = "10000"
            max_drawdown = "0.2"
            max_leverage = "1"
            max_margin_used = "0"
            trading_halted = false

            [broker]
            kind = "ibkr"
            mode = "paper"
            host = "127.0.0.1"
            port = {port}
            client_id = 1
            order_submit_enabled = {order_submit_enabled}

            [paper]
            account_id = "{account_id}"
            slippage_bps = "5"
            fee_bps = "2"

            [live]
            enabled = false
            "#
        ),
    )
    .unwrap();
    config
}

fn seed_contract_cli_storage() -> PathBuf {
    let db_path = temp_output("trader-cli-contract-storage", "sqlite");
    let config = write_contract_cli_config(&db_path);

    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let db = storage::Db::connect(&format!("sqlite://{}", toml_path(&db_path)))
            .await
            .unwrap();
        db.migrate().await.unwrap();
        db.start_strategy_run(storage::StrategyRunStartCommand {
            run_id: "cli-contract-run".to_string(),
            name: "contract-cli".to_string(),
            mode: "paper".to_string(),
            started_at_ms: 1,
            config: serde_json::json!({}),
        })
        .await
        .unwrap();
        db.record_crypto_position(storage::CryptoPositionCommand {
            run_id: "cli-contract-run".to_string(),
            account_id: "paper".to_string(),
            exchange: "BINANCE".to_string(),
            symbol: "BTCUSDT_PERP".to_string(),
            asset_class: "CRYPTO_PERP".to_string(),
            margin_mode: "cross".to_string(),
            position_side: "LONG".to_string(),
            leverage: dec!(10),
            qty: dec!(0.25),
            avg_price: dec!(50000),
            margin_used: dec!(1250),
            funding_fee: dec!(-1.25),
            realized_pnl: dec!(-1.25),
            unrealized_pnl: dec!(12.5),
            updated_at_ms: 1500,
        })
        .await
        .unwrap();
        db.record_crypto_position(storage::CryptoPositionCommand {
            run_id: "cli-contract-run".to_string(),
            account_id: "other".to_string(),
            exchange: "BINANCE".to_string(),
            symbol: "ETHUSDT_PERP".to_string(),
            asset_class: "CRYPTO_PERP".to_string(),
            margin_mode: "cross".to_string(),
            position_side: "LONG".to_string(),
            leverage: dec!(5),
            qty: dec!(1),
            avg_price: dec!(3000),
            margin_used: dec!(600),
            funding_fee: dec!(0),
            realized_pnl: dec!(0),
            unrealized_pnl: dec!(0),
            updated_at_ms: 1500,
        })
        .await
        .unwrap();
        db.record_funding_rate(storage::FundingRateCommand {
            id: "funding-cli-1".to_string(),
            exchange: "BINANCE".to_string(),
            symbol: "BTCUSDT_PERP".to_string(),
            funding_time_ms: 1000,
            funding_rate: dec!(0.0002),
            mark_price: Some(dec!(50000)),
            source: "seed".to_string(),
        })
        .await
        .unwrap();
        db.record_funding_rate(storage::FundingRateCommand {
            id: "funding-cli-outside".to_string(),
            exchange: "BINANCE".to_string(),
            symbol: "BTCUSDT_PERP".to_string(),
            funding_time_ms: 2500,
            funding_rate: dec!(0.0003),
            mark_price: Some(dec!(51000)),
            source: "seed".to_string(),
        })
        .await
        .unwrap();
        db.record_paper_portfolio_snapshot(storage::PaperPortfolioSnapshotCommand {
            run_id: "cli-contract-run".to_string(),
            account_id: "paper".to_string(),
            ts_ms: 1500,
            base_currency: "USDT".to_string(),
            cash: dec!(99900),
            market_value: dec!(12500),
            equity: dec!(112400),
            realized_pnl: dec!(-1.25),
            unrealized_pnl: dec!(12.5),
            positions: vec![storage::PositionCommand {
                run_id: "cli-contract-run".to_string(),
                account_id: "paper".to_string(),
                symbol: "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP".to_string(),
                qty: dec!(0.25),
                avg_price: dec!(50000),
                updated_at_ms: 1500,
            }],
        })
        .await
        .unwrap();
        db.record_runtime_event(storage::RuntimeEventCommand {
            source: "cli-contract-run".to_string(),
            ts_ms: 1400,
            category: "broker.order.submitted".to_string(),
            payload: serde_json::json!({
                "run_id": "cli-contract-run",
                "order_id": "order-one",
                "client_order_id": "client-one",
                "broker_order_id": "broker-one",
                "account_id": "paper",
                "symbol": "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP",
                "status": "NEW",
                "message": "submitted to broker"
            }),
        })
        .await
        .unwrap();
        db.record_runtime_event(storage::RuntimeEventCommand {
            source: "cli-contract-run".to_string(),
            ts_ms: 1600,
            category: "broker.order.recovered".to_string(),
            payload: serde_json::json!({
                "run_id": "cli-contract-run",
                "order_id": "order-one",
                "client_order_id": "client-one",
                "broker_order_id": "broker-one",
                "account_id": "paper",
                "symbol": "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP",
                "status": "FILLED",
                "message": "startup recovery matched broker order state"
            }),
        })
        .await
        .unwrap();
        db.record_runtime_event(storage::RuntimeEventCommand {
            source: "cli-contract-run".to_string(),
            ts_ms: 2600,
            category: "broker.order.failed".to_string(),
            payload: serde_json::json!({
                "run_id": "cli-contract-run",
                "order_id": "order-two",
                "client_order_id": "client-two",
                "broker_order_id": "broker-two",
                "account_id": "other",
                "symbol": "CRYPTO:BINANCE:ETHUSDT_PERP:CRYPTO_PERP",
                "status": "REJECTED",
                "message": "risk rejected"
            }),
        })
        .await
        .unwrap();
        db.record_runtime_event(storage::RuntimeEventCommand {
            source: "cli-contract-run".to_string(),
            ts_ms: 1500,
            category: "algorithm.risk.rejected".to_string(),
            payload: serde_json::json!({
                "run_id": "cli-contract-run",
                "account_id": "paper",
                "symbol": "BTCUSDT_PERP",
                "risk_type": "reconciliation_drift",
                "decision": "warn",
                "reason": "qty mismatch",
                "threshold": "1",
                "observed_value": "2"
            }),
        })
        .await
        .unwrap();
        db.record_runtime_event(storage::RuntimeEventCommand {
            source: "cli-contract-run".to_string(),
            ts_ms: 2600,
            category: "algorithm.risk.rejected".to_string(),
            payload: serde_json::json!({
                "run_id": "cli-contract-run",
                "account_id": "other",
                "symbol": "CRYPTO:BINANCE:ETHUSDT_PERP:CRYPTO_PERP",
                "risk_type": "cash_limit",
                "decision": "reject",
                "reason": "cash guard tripped",
                "threshold": "100",
                "observed_value": "120"
            }),
        })
        .await
        .unwrap();
    });

    config
}

fn seed_ingestion_cli_storage() -> PathBuf {
    let db_path = temp_output("trader-cli-ingestion-storage", "sqlite");
    let config = write_contract_cli_config(&db_path);

    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let db = storage::Db::connect(&format!("sqlite://{}", toml_path(&db_path)))
            .await
            .unwrap();
        db.migrate().await.unwrap();
        db.record_system_log(storage::SystemLogCommand {
            run_id: None,
            ts_ms: 1234,
            level: "INFO".to_string(),
            target: "ingestion".to_string(),
            message: "ingested 2 rows into funding_rates from binance".to_string(),
            fields: Some(serde_json::json!({
                "source": "binance",
                "table": "funding_rates",
                "rows_fetched": 3,
                "rows_upserted": 2,
                "duration_ms": 25
            })),
        })
        .await
        .unwrap();
    });

    config
}

fn seed_config_lifecycle_cli_storage() -> PathBuf {
    let db_path = temp_output("trader-cli-config-lifecycle-storage", "sqlite");
    let config = write_contract_cli_config(&db_path);

    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let db = storage::Db::connect(&format!("sqlite://{}", toml_path(&db_path)))
            .await
            .unwrap();
        db.migrate().await.unwrap();
        db.start_strategy_run(storage::StrategyRunStartCommand {
            run_id: "cli-config-run".to_string(),
            name: "config-lifecycle-cli".to_string(),
            mode: "paper".to_string(),
            started_at_ms: 1,
            config: serde_json::json!({}),
        })
        .await
        .unwrap();
        db.record_config(storage::ConfigRecordCommand {
            id: "config-paper".to_string(),
            name: "paper-binance".to_string(),
            config_type: "BROKER".to_string(),
            content: "order_submit_enabled = true".to_string(),
            format: "TOML".to_string(),
            checksum: Some("sha256:v1".to_string()),
            ts_ms: 2,
        })
        .await
        .unwrap();
        db.record_config_release(storage::ConfigReleaseCommand {
            config_id: "config-paper".to_string(),
            version: "v1".to_string(),
            status: "released".to_string(),
            released_by: Some("ops".to_string()),
            notes: Some("paper broker rollout".to_string()),
            ts_ms: 3,
        })
        .await
        .unwrap();
        db.bind_run_config_version(storage::RunConfigVersionBindingCommand {
            run_id: "cli-config-run".to_string(),
            config_id: "config-paper".to_string(),
            version: "v1".to_string(),
            ts_ms: 4,
        })
        .await
        .unwrap();
        db.record_config_audit(storage::ConfigAuditCommand {
            config_id: "config-paper".to_string(),
            version: Some("v1".to_string()),
            action: "rollback".to_string(),
            actor: Some("ops".to_string()),
            reason: Some("restore previous release".to_string()),
            ts_ms: 5,
        })
        .await
        .unwrap();
    });

    config
}

fn seed_config_management_cli_storage() -> PathBuf {
    let db_path = temp_output("trader-cli-config-management-storage", "sqlite");
    let config = write_contract_cli_config(&db_path);

    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let db = storage::Db::connect(&format!("sqlite://{}", toml_path(&db_path)))
            .await
            .unwrap();
        db.migrate().await.unwrap();
    });

    config
}

fn seed_logs_cli_storage() -> PathBuf {
    let db_path = temp_output("trader-cli-logs-storage", "sqlite");
    let config = write_contract_cli_config(&db_path);

    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let db = storage::Db::connect(&format!("sqlite://{}", toml_path(&db_path)))
            .await
            .unwrap();
        db.migrate().await.unwrap();
        for (run_id, ts_ms, level, target, message) in [
            (
                Some("cli-logs-run"),
                100,
                "INFO",
                "runtime.execution",
                "execution started",
            ),
            (
                Some("cli-logs-run"),
                200,
                "ERROR",
                "runtime.execution",
                "execution failed",
            ),
            (None, 200, "WARN", "system.scheduler", "scheduler lag"),
        ] {
            db.record_system_log(storage::SystemLogCommand {
                run_id: run_id.map(str::to_string),
                ts_ms,
                level: level.to_string(),
                target: target.to_string(),
                message: message.to_string(),
                fields: Some(serde_json::json!({
                    "category": target.split('.').next().unwrap_or(target)
                })),
            })
            .await
            .unwrap();
        }
    });

    config
}

fn read_http_request(stream: &mut std::net::TcpStream) -> String {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    let mut content_length = None;
    loop {
        let size = std::io::Read::read(stream, &mut chunk).unwrap();
        if size == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..size]);
        if content_length.is_none()
            && let Some(headers_end) = buf.windows(4).position(|window| window == b"\r\n\r\n")
        {
            let headers = String::from_utf8_lossy(&buf[..headers_end + 4]);
            content_length = headers
                .lines()
                .find_map(|line| {
                    line.strip_prefix("content-length: ")
                        .or_else(|| line.strip_prefix("Content-Length: "))
                })
                .and_then(|value| value.trim().parse::<usize>().ok());
        }
        if let Some(expected) = content_length
            && let Some(headers_end) = buf.windows(4).position(|window| window == b"\r\n\r\n")
        {
            let body_len = buf.len().saturating_sub(headers_end + 4);
            if body_len >= expected {
                break;
            }
        }
    }
    String::from_utf8_lossy(&buf).to_string()
}

fn http_header_value(request: &str, name: &str) -> Option<String> {
    let expected = format!("{name}:");
    request
        .lines()
        .find(|line| line.to_ascii_lowercase().starts_with(&expected))
        .and_then(|line| {
            line.split_once(':')
                .map(|(_, value)| value.trim().to_string())
        })
}

fn http_body(request: &str) -> &str {
    request.split_once("\r\n\r\n").map_or("", |(_, body)| body)
}

fn test_log_ship_signature(secret: &str, timestamp_ms: &str, body: &str) -> String {
    let mut mac = hmac::Hmac::<sha2::Sha256>::new_from_slice(secret.as_bytes())
        .expect("HMAC accepts keys of any length");
    hmac::Mac::update(&mut mac, timestamp_ms.as_bytes());
    hmac::Mac::update(&mut mac, b".");
    hmac::Mac::update(&mut mac, body.as_bytes());
    hex::encode(hmac::Mac::finalize(mac).into_bytes())
}

fn seed_reconciliation_alerts_cli_storage() -> PathBuf {
    let db_path = temp_output("trader-cli-reconciliation-alerts-storage", "sqlite");
    let config = write_contract_cli_config(&db_path);

    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let db = storage::Db::connect(&format!("sqlite://{}", toml_path(&db_path)))
            .await
            .unwrap();
        db.migrate().await.unwrap();
        for (run_id, ts_ms, message, fields) in [
            (
                Some("cli-alert-a"),
                100,
                "reconciliation_drift.alert",
                serde_json::json!({
                    "account_id": "paper",
                    "symbol": "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP",
                    "reason": "position_qty_drift"
                }),
            ),
            (
                Some("cli-alert-b"),
                200,
                "reconciliation_drift.alert",
                serde_json::json!({
                    "account_id": "paper",
                    "symbol": "CRYPTO:BINANCE:ETHUSDT_PERP:CRYPTO_PERP",
                    "reason": "cash_total_drift"
                }),
            ),
        ] {
            db.record_system_log(storage::SystemLogCommand {
                run_id: run_id.map(str::to_string),
                ts_ms,
                level: "ERROR".to_string(),
                target: "runtime.alert".to_string(),
                message: message.to_string(),
                fields: Some(fields),
            })
            .await
            .unwrap();
        }
    });

    config
}

fn seed_reconciliation_alert_delivery_cli_storage() -> PathBuf {
    let db_path = temp_output("trader-cli-reconciliation-alert-delivery-storage", "sqlite");
    let config = write_contract_cli_config(&db_path);

    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let db = storage::Db::connect(&format!("sqlite://{}", toml_path(&db_path)))
            .await
            .unwrap();
        db.migrate().await.unwrap();
        for (run_id, ts_ms, message, fields) in [
            (
                Some("cli-delivery-a"),
                90,
                "reconciliation_drift.alert",
                serde_json::json!({
                    "account_id": "paper",
                    "symbol": "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP",
                    "reason": "position_qty_drift"
                }),
            ),
            (
                Some("cli-delivery-b"),
                190,
                "reconciliation_drift.alert",
                serde_json::json!({
                    "account_id": "paper",
                    "symbol": "CRYPTO:BINANCE:ETHUSDT_PERP:CRYPTO_PERP",
                    "reason": "cash_total_drift"
                }),
            ),
        ] {
            db.record_system_log(storage::SystemLogCommand {
                run_id: run_id.map(str::to_string),
                ts_ms,
                level: "ERROR".to_string(),
                target: "runtime.alert".to_string(),
                message: message.to_string(),
                fields: Some(fields),
            })
            .await
            .unwrap();
        }
        for (run_id, ts_ms, fields) in [
            (
                Some("cli-delivery-a"),
                100,
                serde_json::json!({
                    "account_id": "paper",
                    "symbol": "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP",
                    "sink": "webhook",
                    "status": "failed",
                    "http_status": 500,
                    "attempts": 1,
                    "dedup_key": "reconciliation_drift.alert|cli-delivery-a|paper|CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP|position_qty_drift"
                }),
            ),
            (
                Some("cli-delivery-b"),
                200,
                serde_json::json!({
                    "account_id": "paper",
                    "symbol": "CRYPTO:BINANCE:ETHUSDT_PERP:CRYPTO_PERP",
                    "sink": "file",
                    "status": "sent",
                    "attempts": 1,
                    "dedup_key": "reconciliation_drift.alert|cli-delivery-b|paper|CRYPTO:BINANCE:ETHUSDT_PERP:CRYPTO_PERP|cash_total_drift"
                }),
            ),
        ] {
            db.record_system_log(storage::SystemLogCommand {
                run_id: run_id.map(str::to_string),
                ts_ms,
                level: "INFO".to_string(),
                target: "runtime.alert_delivery".to_string(),
                message: "alert.delivery".to_string(),
                fields: Some(fields),
            })
            .await
            .unwrap();
        }
    });

    config
}

fn seed_report_cli_storage() -> PathBuf {
    let db_path = temp_output("trader-cli-report-storage", "sqlite");
    let config = write_report_cli_config(&db_path);

    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let db = storage::Db::connect(&format!("sqlite://{}", toml_path(&db_path)))
            .await
            .unwrap();
        db.migrate().await.unwrap();
        for (run_id, equity) in [
            ("cli-report-a", dec!(100000)),
            ("cli-report-b", dec!(101500)),
        ] {
            db.start_strategy_run(storage::StrategyRunStartCommand {
                run_id: run_id.to_string(),
                name: format!("strategy-{run_id}"),
                mode: "paper".to_string(),
                started_at_ms: 1,
                config: serde_json::json!({ "run_id": run_id }),
            })
            .await
            .unwrap();
            db.record_paper_portfolio_snapshot(storage::PaperPortfolioSnapshotCommand {
                run_id: run_id.to_string(),
                account_id: "paper".to_string(),
                ts_ms: 10,
                base_currency: "USD".to_string(),
                cash: equity,
                market_value: dec!(0),
                equity,
                realized_pnl: dec!(0),
                unrealized_pnl: dec!(0),
                positions: Vec::new(),
            })
            .await
            .unwrap();
        }
    });

    config
}

fn write_report_cli_config(db_path: &std::path::Path) -> PathBuf {
    let config = temp_output("trader-cli-report-storage", "toml");
    std::fs::write(
        &config,
        format!(
            r#"
            [runtime]
            mode = "paper"
            run_id = "cli-report-a"

            [database]
            url = "sqlite://{}"

            [data]
            source = "csv"
            path = "unused.csv"

            [strategy]
            name = "moving_average_cross"
            symbols = ["US:NASDAQ:AAPL:EQUITY"]
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
            toml_path(db_path)
        ),
    )
    .unwrap();
    config
}

fn write_contract_cli_config(db_path: &std::path::Path) -> PathBuf {
    let config = temp_output("trader-cli-contract-storage", "toml");
    std::fs::write(
        &config,
        format!(
            r#"
            [runtime]
            mode = "paper"
            run_id = "cli-contract-run"

            [database]
            url = "sqlite://{}"

            [data]
            source = "csv"
            path = "unused.csv"

            [strategy]
            name = "price_channel_reversion"
            symbols = ["CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP"]
            fast_window = 1
            slow_window = 2

            [portfolio]
            initial_cash = "100000"
            base_currency = "USDT"
            order_qty = "1"
            max_abs_qty = "100"

            [risk]
            max_order_notional = "1000000"
            min_cash_after_order = "0"
            max_exposure = "1000000"
            max_drawdown = "1"
            max_leverage = "10"
            max_margin_used = "1000000"
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
            toml_path(db_path)
        ),
    )
    .unwrap();
    config
}

fn write_crypto_perp_reversion_cli_config() -> PathBuf {
    let bars_path = temp_output("trader-cli-crypto-perp-reversion", "csv");
    let db_path = temp_output("trader-cli-crypto-perp-reversion", "sqlite");
    let config = temp_output("trader-cli-crypto-perp-reversion", "toml");
    std::fs::write(
        &bars_path,
        "ts_ms,open,high,low,close,volume\n1,10,10,10,10,1\n2,11,11,11,11,1\n3,20,20,20,20,1\n",
    )
    .unwrap();
    std::fs::write(
        &config,
        format!(
            r#"
            [runtime]
            mode = "backtest"
            run_id = "crypto-perp-reversion"

            [database]
            url = "sqlite://{}"

            [data]
            source = "csv"
            path = "{}"

            [strategy]
            name = "price_channel_reversion"
            alpha = "price_channel_reversion"
            symbols = ["CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP"]
            fast_window = 1
            slow_window = 2

            [portfolio]
            initial_cash = "100000"
            base_currency = "USDT"
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
            toml_path(&db_path),
            toml_path(&bars_path)
        ),
    )
    .unwrap();
    config
}

fn write_multi_symbol_cli_config(run_id: &str, runtime_mode: &str) -> PathBuf {
    let aapl_path = temp_output("trader-cli-aapl", "csv");
    let msft_path = temp_output("trader-cli-msft", "csv");
    let db_path = temp_output("trader-cli-multi-symbol", "sqlite");
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

    let config = temp_output("trader-cli-multi-symbol", "toml");
    std::fs::write(
        &config,
        format!(
            r#"
            [runtime]
            mode = "{runtime_mode}"
            run_id = "{run_id}"

            [database]
            url = "sqlite://{}"

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
            toml_path(&db_path),
            toml_path(&aapl_path),
            toml_path(&msft_path)
        ),
    )
    .unwrap();
    config
}

fn write_replay_cli_config(run_id: &str) -> PathBuf {
    let bars_path = temp_output("trader-cli-replay-bars", "csv");
    let db_path = temp_output("trader-cli-replay", "sqlite");
    let config = temp_output("trader-cli-replay", "toml");
    std::fs::write(
        &bars_path,
        "ts_ms,open,high,low,close,volume\n1,10,10,10,10,1\n2,11,11,11,11,1\n3,20,20,20,20,1\n",
    )
    .unwrap();
    std::fs::write(
        &config,
        format!(
            r#"
            [runtime]
            mode = "replay"
            run_id = "{run_id}"

            [database]
            url = "sqlite://{}"

            [data]
            source = "csv"
            path = "{}"

            [strategy]
            name = "moving_average_cross"
            symbols = ["US:NASDAQ:AAPL:EQUITY"]
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
            toml_path(&db_path),
            toml_path(&bars_path)
        ),
    )
    .unwrap();
    config
}

fn write_filtered_multi_symbol_cli_config(run_id: &str, runtime_mode: &str) -> PathBuf {
    let aapl_path = temp_output("trader-cli-filtered-aapl", "csv");
    let msft_path = temp_output("trader-cli-filtered-msft", "csv");
    let db_path = temp_output("trader-cli-filtered", "sqlite");
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

    let config = temp_output("trader-cli-filtered", "toml");
    std::fs::write(
        &config,
        format!(
            r#"
            [runtime]
            mode = "{runtime_mode}"
            run_id = "{run_id}"

            [database]
            url = "sqlite://{}"

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
            toml_path(&db_path),
            toml_path(&aapl_path),
            toml_path(&msft_path)
        ),
    )
    .unwrap();
    config
}

fn write_feature_ranked_multi_symbol_cli_config(run_id: &str) -> PathBuf {
    let aapl_path = temp_output("trader-cli-feature-ranked-aapl", "csv");
    let msft_path = temp_output("trader-cli-feature-ranked-msft", "csv");
    let db_path = temp_output("trader-cli-feature-ranked", "sqlite");
    let feature_path = temp_output("trader-cli-feature-ranked", "parquet");
    let manifest_path = temp_output("trader-cli-feature-ranked", "json");
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
            dec!(0.1),
            "v1",
        ),
        FeatureRecord::new(
            "research-rank",
            "US:NASDAQ:MSFT:EQUITY",
            1,
            "quality_score",
            dec!(0.9),
            "v1",
        ),
    ];
    write_feature_records_to_parquet(&feature_path, &records).unwrap();
    let manifest = build_feature_manifest(&feature_path, &records);
    write_feature_manifest(&manifest_path, &manifest).unwrap();

    let config = temp_output("trader-cli-feature-ranked", "toml");
    std::fs::write(
        &config,
        format!(
            r#"
            [runtime]
            mode = "backtest"
            run_id = "{run_id}"

            [database]
            url = "sqlite://{}"

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
            toml_path(&db_path),
            toml_path(&aapl_path),
            toml_path(&msft_path),
            toml_path(&feature_path),
            toml_path(&manifest_path)
        ),
    )
    .unwrap();
    config
}

fn toml_path(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn workspace_root() -> &'static str {
    Box::leak(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .unwrap()
            .to_string_lossy()
            .into_owned()
            .into_boxed_str(),
    )
}
