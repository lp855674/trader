use assert_cmd::Command;
use predicates::str::contains;

#[test]
fn check_config_prints_ok() {
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .arg("check-config")
        .assert()
        .success()
        .stdout(contains("config ok"));
}
