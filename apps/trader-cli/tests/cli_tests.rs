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
