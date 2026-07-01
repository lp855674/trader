use runtime::{
    LiveWorkerCommand, LiveWorkerEvent, LiveWorkerLaunchSpec,
    StartupRecoveryUnmatchedOpenOrdersPolicy, parse_worker_command_line, worker_event_line,
};

#[test]
fn worker_command_jsonl_parses_health_and_shutdown() {
    let health =
        parse_worker_command_line(r#"{"type":"health_check","request_id":"health-1"}"#).unwrap();
    assert_eq!(
        health,
        LiveWorkerCommand::HealthCheck {
            request_id: "health-1".to_string()
        }
    );

    let shutdown = parse_worker_command_line(
        r#"{"type":"shutdown","request_id":"stop-1","reason":"api_stop"}"#,
    )
    .unwrap();
    assert_eq!(
        shutdown,
        LiveWorkerCommand::Shutdown {
            request_id: "stop-1".to_string(),
            reason: "api_stop".to_string()
        }
    );
}

#[test]
fn worker_event_jsonl_serializes_with_type_tags() {
    let line = worker_event_line(&LiveWorkerEvent::WorkerStarted {
        run_id: "live-1".to_string(),
        pid: 1234,
    })
    .unwrap();

    assert_eq!(
        line,
        r#"{"type":"worker_started","run_id":"live-1","pid":1234}"#
    );
}

#[test]
fn launch_spec_redaction_rejects_secret_fields() {
    let spec = LiveWorkerLaunchSpec {
        run_id: "live-1".to_string(),
        db_url: "sqlite:data/trader.db".to_string(),
        config_path: Some("configs/backtest/ma_cross.toml".to_string()),
        config_content:
            "[broker]\napi_key_env = \"BINANCE_KEY\"\nsecret_key_env = \"BINANCE_SECRET\"\n"
                .to_string(),
        config_format: "TOML".to_string(),
        run_spec: None,
        broker_snapshot_interval_ms: Some(1000),
        startup_recovery_unmatched_open_orders_policy:
            StartupRecoveryUnmatchedOpenOrdersPolicy::Fail,
    };

    assert!(spec.validate_no_embedded_secrets().is_ok());

    let mut invalid = spec.clone();
    invalid
        .config_content
        .push_str("api_key = \"literal-secret\"\n");
    let error = invalid.validate_no_embedded_secrets().unwrap_err();
    assert!(
        error
            .to_string()
            .contains("launch file contains secret-like key")
    );
}

#[test]
fn launch_spec_redaction_rejects_webhook_auth_token() {
    let spec = LiveWorkerLaunchSpec {
        run_id: "live-1".to_string(),
        db_url: "sqlite:data/trader.db".to_string(),
        config_path: None,
        config_content: "[runtime]\nmode = \"live\"\nrun_id = \"live-1\"\n[live]\nenabled = true\n[live.alerts]\nwebhook_auth_token = \"literal-token\"\n".to_string(),
        config_format: "TOML".to_string(),
        run_spec: None,
        broker_snapshot_interval_ms: None,
        startup_recovery_unmatched_open_orders_policy:
            StartupRecoveryUnmatchedOpenOrdersPolicy::Fail,
    };

    let error = spec.validate_no_embedded_secrets().unwrap_err();
    assert!(
        error
            .to_string()
            .contains("launch file contains secret-like key live.alerts.webhook_auth_token")
    );
}

#[test]
fn launch_spec_redaction_rejects_credentialed_database_url() {
    let spec = LiveWorkerLaunchSpec {
        run_id: "live-1".to_string(),
        db_url: "postgres://trader:secret@example.test/trader".to_string(),
        config_path: None,
        config_content: "[runtime]\nmode = \"live\"\nrun_id = \"live-1\"\n".to_string(),
        config_format: "TOML".to_string(),
        run_spec: None,
        broker_snapshot_interval_ms: None,
        startup_recovery_unmatched_open_orders_policy:
            StartupRecoveryUnmatchedOpenOrdersPolicy::Fail,
    };

    let error = spec.validate_no_embedded_secrets().unwrap_err();
    assert!(
        error
            .to_string()
            .contains("launch file contains credentialed db_url")
    );
}
