use assert_cmd::Command;
use predicates::str::contains;
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
fn report_accepts_config_argument() {
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
        .success()
        .stdout(contains("report: run_id=sample-ma-cross"));
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
