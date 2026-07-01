use runtime::{
    LiveProcessError, LiveProcessStatus, LiveProcessSupervisor, LiveProcessSupervisorOptions,
    LiveWorkerLaunchSpec, StartupRecoveryUnmatchedOpenOrdersPolicy,
};
use std::{
    io::{BufRead, Write},
    path::PathBuf,
};
use storage::{Db, StrategyRunStartCommand, SystemLogFilter};

#[tokio::test]
async fn supervisor_rejects_duplicate_active_run_id() {
    let (db, db_url) = temp_db("duplicate").await;
    let supervisor = fake_supervisor(db, "healthy", "run-1");

    supervisor
        .start("run-1".to_string(), launch_spec("run-1", &db_url))
        .await
        .unwrap();
    let duplicate = supervisor
        .start("run-1".to_string(), launch_spec("run-1", &db_url))
        .await;

    assert_eq!(duplicate.unwrap_err(), LiveProcessError::AlreadyRunning);
    assert!(supervisor.stop("run-1").await);
}

#[tokio::test]
async fn supervisor_records_heartbeat_and_health() {
    let (db, db_url) = temp_db("heartbeat").await;
    let supervisor = fake_supervisor(db, "healthy", "run-1");

    supervisor
        .start("run-1".to_string(), launch_spec("run-1", &db_url))
        .await
        .unwrap();
    let snapshot = wait_for_snapshot(&supervisor, "run-1", |snapshot| {
        snapshot.last_heartbeat_at_ms.is_some()
    })
    .await;

    assert_eq!(snapshot.status, LiveProcessStatus::Running);
    assert_eq!(snapshot.ipc_status.as_deref(), Some("running"));
    assert!(supervisor.stop("run-1").await);
}

#[tokio::test]
async fn supervisor_marks_non_terminal_run_failed_on_crash() {
    let (db, db_url) = temp_db("crash").await;
    seed_running_live_run(&db, "run-1").await;
    let supervisor = fake_supervisor(db.clone(), "crash_after_started", "run-1");

    supervisor
        .start("run-1".to_string(), launch_spec("run-1", &db_url))
        .await
        .unwrap();
    wait_for_snapshot(&supervisor, "run-1", |snapshot| {
        snapshot.status == LiveProcessStatus::Failed
    })
    .await;

    let run = db.get_strategy_run("run-1").await.unwrap().unwrap();
    assert_eq!(run.status, "failed");
    wait_for_process_logs(&db, "run-1", "ERROR", None).await;
}

#[tokio::test]
async fn supervisor_kills_stale_heartbeat_worker() {
    let (db, db_url) = temp_db("stale").await;
    seed_running_live_run(&db, "run-1").await;
    let mut options = fake_options("silent", "run-1");
    options.heartbeat_stale_after_ms = 20;
    options.health_response_timeout_ms = 20;
    let supervisor = LiveProcessSupervisor::with_options(db.clone(), options);

    supervisor
        .start("run-1".to_string(), launch_spec("run-1", &db_url))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    assert_eq!(supervisor.check_heartbeats().await, 1);

    let run = db.get_strategy_run("run-1").await.unwrap().unwrap();
    assert_eq!(run.status, "failed");
}

#[tokio::test]
async fn supervisor_fails_run_on_handshake_timeout() {
    let (db, db_url) = temp_db("handshake-timeout").await;
    seed_running_live_run(&db, "run-1").await;
    let mut options = fake_options("silent_before_handshake", "run-1");
    options.handshake_timeout_ms = 20;
    let supervisor = LiveProcessSupervisor::with_options(db.clone(), options);

    let result = supervisor
        .start("run-1".to_string(), launch_spec("run-1", &db_url))
        .await;

    assert_eq!(result.unwrap_err(), LiveProcessError::HandshakeTimeout);
    let run = db.get_strategy_run("run-1").await.unwrap().unwrap();
    assert_eq!(run.status, "failed");
    assert_eq!(run.error.as_deref(), Some("handshake timeout"));
    wait_for_process_logs(&db, "run-1", "ERROR", Some("handshake timeout")).await;
}

#[test]
fn fake_live_worker_process() {
    let Ok(mode) = std::env::var("TRADER_FAKE_LIVE_WORKER") else {
        return;
    };
    let run_id = std::env::var("TRADER_FAKE_RUN_ID").unwrap_or_else(|_| "run-1".to_string());
    if mode == "silent_before_handshake" {
        std::thread::sleep(std::time::Duration::from_secs(60));
        return;
    }
    println!(
        r#"{{"type":"worker_started","run_id":"{run_id}","pid":{}}}"#,
        std::process::id()
    );
    println!(r#"{{"type":"runtime_started","run_id":"{run_id}"}}"#);
    std::io::stdout().flush().unwrap();
    match mode.as_str() {
        "crash_after_started" => std::process::exit(17),
        "silent" => std::thread::sleep(std::time::Duration::from_secs(60)),
        "healthy" => {
            println!(r#"{{"type":"heartbeat","run_id":"{run_id}","status":"running","ts_ms":1}}"#);
            std::io::stdout().flush().unwrap();
            let mut line = String::new();
            while std::io::stdin().lock().read_line(&mut line).unwrap() > 0 {
                if line.contains("\"shutdown\"") {
                    println!(
                        r#"{{"type":"runtime_stopped","run_id":"{run_id}","status":"stopped"}}"#
                    );
                    std::io::stdout().flush().unwrap();
                    return;
                }
                line.clear();
            }
        }
        other => panic!("unknown fake worker mode {other}"),
    }
}

fn fake_supervisor(db: Db, mode: &str, run_id: &str) -> LiveProcessSupervisor {
    LiveProcessSupervisor::with_options(db, fake_options(mode, run_id))
}

fn fake_options(mode: &str, run_id: &str) -> LiveProcessSupervisorOptions {
    LiveProcessSupervisorOptions {
        trader_exe: std::env::current_exe().unwrap(),
        launch_root: temp_path("launch-root"),
        handshake_timeout_ms: 1_000,
        graceful_shutdown_timeout_ms: 1_000,
        heartbeat_stale_after_ms: 1_000,
        health_response_timeout_ms: 1_000,
        stderr_line_limit: 32,
        extra_args: Vec::new(),
        extra_env: vec![
            ("TRADER_FAKE_LIVE_WORKER".to_string(), mode.to_string()),
            ("TRADER_FAKE_RUN_ID".to_string(), run_id.to_string()),
        ],
        worker_args_override: Some(vec![
            "--exact".to_string(),
            "fake_live_worker_process".to_string(),
            "--nocapture".to_string(),
        ]),
    }
}

async fn temp_db(name: &str) -> (Db, String) {
    let path = temp_path(name).join("db.sqlite");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let db_url = format!("sqlite:{}", path.to_string_lossy().replace('\\', "/"));
    let db = Db::connect(&db_url).await.unwrap();
    db.migrate().await.unwrap();
    (db, db_url)
}

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "trader-live-process-supervisor-{name}-{}",
        uuid::Uuid::new_v4()
    ))
}

fn launch_spec(run_id: &str, db_url: &str) -> LiveWorkerLaunchSpec {
    LiveWorkerLaunchSpec {
        run_id: run_id.to_string(),
        db_url: db_url.to_string(),
        config_path: None,
        config_content: "[runtime]\nmode = \"live\"\nrun_id = \"run-1\"\n".to_string(),
        config_format: "TOML".to_string(),
        run_spec: None,
        broker_snapshot_interval_ms: None,
        startup_recovery_unmatched_open_orders_policy:
            StartupRecoveryUnmatchedOpenOrdersPolicy::Fail,
    }
}

async fn seed_running_live_run(db: &Db, run_id: &str) {
    db.start_strategy_run(StrategyRunStartCommand {
        run_id: run_id.to_string(),
        name: "live".to_string(),
        mode: "live".to_string(),
        started_at_ms: 1,
        config: serde_json::json!({ "run_id": run_id }),
    })
    .await
    .unwrap();
}

async fn wait_for_snapshot(
    supervisor: &LiveProcessSupervisor,
    run_id: &str,
    predicate: impl Fn(&runtime::LiveProcessSnapshot) -> bool,
) -> runtime::LiveProcessSnapshot {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        if let Some(snapshot) = supervisor.snapshot(run_id).await
            && predicate(&snapshot)
        {
            return snapshot;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for live process snapshot"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

async fn wait_for_process_logs(db: &Db, run_id: &str, level: &str, search: Option<&str>) {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        let logs = db
            .list_system_logs_filtered(SystemLogFilter {
                run_id: Some(run_id.to_string()),
                target: Some("runtime.live_process".to_string()),
                level: Some(level.to_string()),
                search: search.map(str::to_string),
                ..SystemLogFilter::default()
            })
            .await
            .unwrap();
        if !logs.is_empty() {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for live process logs"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}
