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
