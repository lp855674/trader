# Live Process Isolation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Run API-started Live runtimes in supervised child processes while preserving the current Live HTTP API, SQLite audit trail, startup recovery semantics, reconciliation snapshots, and alert delivery behavior.

**Architecture:** Keep the API server as the public control plane and add a focused `LiveProcessSupervisor` for Live runs only. The supervisor writes a non-secret launch file, starts `trader live-worker --launch-file ...`, exchanges JSONL commands/events over stdin/stdout, captures stderr into bounded diagnostics, and uses SQLite as the source of truth for business state.

**Tech Stack:** Rust 2024, Tokio process/io/time, Axum, SQLx SQLite through `storage::Db`, serde/serde_json JSONL, clap.

## Global Constraints

- Do not submit real broker orders as part of the default implementation or tests.
- Do not replace SQLite run state, events, snapshots, or alert logs with IPC state.
- Do not move Paper, Backtest, or Replay into isolated processes in this phase.
- Do not introduce a distributed worker service, queue, or orchestrator.
- Do not add a second public Live API shape unless existing status responses cannot represent the new state.
- Launch files must not include API keys, secret keys, bearer tokens, or account credentials.
- Child process command lines must avoid embedding secrets.
- Worker stderr capture must be bounded or streamed to avoid unbounded memory growth.
- Shutdown must prefer graceful cancellation before kill.
- No IPC command may submit, cancel, or mutate broker orders in this phase.
- Backoff/restart is out of scope; automatic restart after process crash must not happen silently.

---

## File Structure

Create:

- `crates/runtime/src/worker_protocol.rs` - JSONL command/event types, launch spec, serde helpers, and line parsing.
- `crates/runtime/src/process.rs` - `LiveProcessSupervisor`, child registry, heartbeat tracking, shutdown, exit classification, and stderr capture.
- `crates/runtime/tests/worker_protocol_tests.rs` - protocol serde and launch spec redaction tests.
- `crates/runtime/tests/live_process_supervisor_tests.rs` - supervisor tests using a re-executed test binary fake worker.

Modify:

- `crates/runtime/src/runtime.rs` - export the protocol and process modules.
- `crates/runtime/Cargo.toml` - add `serde.workspace = true` and enable any Tokio features already available from the workspace.
- `apps/trader-cli/src/main.rs` - add `trader live-worker --launch-file <path>` and worker runtime loop.
- `apps/trader-cli/Cargo.toml` - add `serde.workspace = true` if needed by launch parsing in the CLI.
- `apps/trader-cli/tests/cli_tests.rs` - add CLI worker protocol smoke tests.
- `crates/api/src/state.rs` - store a process-aware Live supervisor next to the existing in-process `RuntimeManager`.
- `crates/api/src/api.rs` - route Live start/stop/status through the supervisor while leaving other modes on `RuntimeManager`.
- `crates/api/tests/api_tests.rs` - update existing Live route tests and add crash/startup-recovery failure coverage.
- `docs/architecture.md` - document that API-started Live is child-process isolated.
- `docs/api.md` - document that the existing Live API shape is unchanged and process state is internal.

---

### Task 1: Add Live Worker Protocol And Launch Spec

**Files:**
- Create: `crates/runtime/src/worker_protocol.rs`
- Create: `crates/runtime/tests/worker_protocol_tests.rs`
- Modify: `crates/runtime/src/runtime.rs`
- Modify: `crates/runtime/Cargo.toml`

**Interfaces:**
- Consumes: `runtime::RunSpec`, `runtime::LiveRuntimeSettings`, `runtime::StartupRecoveryUnmatchedOpenOrdersPolicy`
- Produces: `runtime::LiveWorkerCommand`, `runtime::LiveWorkerEvent`, `runtime::LiveWorkerLaunchSpec`, `runtime::parse_worker_command_line(line: &str) -> anyhow::Result<LiveWorkerCommand>`, `runtime::worker_event_line(event: &LiveWorkerEvent) -> anyhow::Result<String>`

- [ ] **Step 1: Write failing protocol serde tests**

Create `crates/runtime/tests/worker_protocol_tests.rs`:

```rust
use runtime::{
    LiveWorkerCommand, LiveWorkerEvent, LiveWorkerLaunchSpec,
    StartupRecoveryUnmatchedOpenOrdersPolicy, parse_worker_command_line, worker_event_line,
};

#[test]
fn worker_command_jsonl_parses_health_and_shutdown() {
    let health = parse_worker_command_line(r#"{"type":"health_check","request_id":"health-1"}"#)
        .unwrap();
    assert_eq!(
        health,
        LiveWorkerCommand::HealthCheck {
            request_id: "health-1".to_string()
        }
    );

    let shutdown =
        parse_worker_command_line(r#"{"type":"shutdown","request_id":"stop-1","reason":"api_stop"}"#)
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
        config_content: "[broker]\napi_key_env = \"BINANCE_KEY\"\nsecret_key_env = \"BINANCE_SECRET\"\n"
            .to_string(),
        config_format: "TOML".to_string(),
        run_spec: None,
        broker_snapshot_interval_ms: Some(1000),
        startup_recovery_unmatched_open_orders_policy:
            StartupRecoveryUnmatchedOpenOrdersPolicy::Fail,
    };

    assert!(spec.validate_no_embedded_secrets().is_ok());

    let mut invalid = spec.clone();
    invalid.config_content.push_str("api_key = \"literal-secret\"\n");
    let error = invalid.validate_no_embedded_secrets().unwrap_err();
    assert!(error.to_string().contains("launch file contains secret-like key"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p runtime worker_protocol --test worker_protocol_tests`

Expected: FAIL because `worker_protocol` types and helpers do not exist.

- [ ] **Step 3: Implement `worker_protocol.rs`**

Create `crates/runtime/src/worker_protocol.rs` with these public shapes:

```rust
use crate::{RunSpec, StartupRecoveryUnmatchedOpenOrdersPolicy};
use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveWorkerLaunchSpec {
    pub run_id: String,
    pub db_url: String,
    pub config_path: Option<String>,
    pub config_content: String,
    pub config_format: String,
    pub run_spec: Option<RunSpec>,
    pub broker_snapshot_interval_ms: Option<u64>,
    pub startup_recovery_unmatched_open_orders_policy:
        StartupRecoveryUnmatchedOpenOrdersPolicy,
}

impl LiveWorkerLaunchSpec {
    pub fn validate_no_embedded_secrets(&self) -> anyhow::Result<()> {
        match self.config_format.as_str() {
            "TOML" => {
                let parsed: toml::Value = toml::from_str(&self.config_content)
                    .context("failed to parse launch config_content as TOML")?;
                reject_secret_like_toml_values(None, &parsed)
            }
            "JSON" => {
                let parsed: serde_json::Value = serde_json::from_str(&self.config_content)
                    .context("failed to parse launch config_content as JSON")?;
                reject_secret_like_json_values(None, &parsed)
            }
            other => bail!("unsupported launch config_format {other}"),
        }
    }
}

fn reject_secret_like_toml_values(path: Option<&str>, value: &toml::Value) -> anyhow::Result<()> {
    match value {
        toml::Value::Table(table) => {
            for (key, value) in table {
                let path = path.map_or_else(|| key.to_string(), |prefix| format!("{prefix}.{key}"));
                let lower = key.to_ascii_lowercase();
                if matches!(
                    lower.as_str(),
                    "api_key" | "secret_key" | "auth_token" | "bearer_token" | "password"
                ) {
                    bail!("launch file contains secret-like key {path}");
                }
                reject_secret_like_toml_values(Some(&path), value)?;
            }
            Ok(())
        }
        toml::Value::Array(values) => {
            for value in values {
                reject_secret_like_toml_values(path, value)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn reject_secret_like_json_values(
    path: Option<&str>,
    value: &serde_json::Value,
) -> anyhow::Result<()> {
    match value {
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                let path = path.map_or_else(|| key.to_string(), |prefix| format!("{prefix}.{key}"));
                let lower = key.to_ascii_lowercase();
                if matches!(
                    lower.as_str(),
                    "api_key" | "secret_key" | "auth_token" | "bearer_token" | "password"
                ) {
                    bail!("launch file contains secret-like key {path}");
                }
                reject_secret_like_json_values(Some(&path), value)?;
            }
            Ok(())
        }
        serde_json::Value::Array(values) => {
            for value in values {
                reject_secret_like_json_values(path, value)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LiveWorkerCommand {
    HealthCheck { request_id: String },
    Shutdown { request_id: String, reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LiveWorkerEvent {
    WorkerStarted { run_id: String, pid: u32 },
    RuntimeStarted { run_id: String },
    Heartbeat { run_id: String, status: String, ts_ms: i64 },
    Health { run_id: String, request_id: String, status: String },
    RuntimeStopping { run_id: String, reason: String },
    RuntimeStopped { run_id: String, status: String },
    RuntimeFailed { run_id: String, error: String },
}

pub fn parse_worker_command_line(line: &str) -> anyhow::Result<LiveWorkerCommand> {
    serde_json::from_str(line).context("failed to parse worker command JSONL")
}

pub fn parse_worker_event_line(line: &str) -> anyhow::Result<LiveWorkerEvent> {
    serde_json::from_str(line).context("failed to parse worker event JSONL")
}

pub fn worker_command_line(command: &LiveWorkerCommand) -> anyhow::Result<String> {
    serde_json::to_string(command).context("failed to serialize worker command")
}

pub fn worker_event_line(event: &LiveWorkerEvent) -> anyhow::Result<String> {
    serde_json::to_string(event).context("failed to serialize worker event")
}
```

Also derive `Serialize` and `Deserialize` for `RunSpec` and nested spec structs in `crates/runtime/src/run_spec.rs`, and for `StartupRecoveryUnmatchedOpenOrdersPolicy` in `crates/runtime/src/live.rs`.

- [ ] **Step 4: Export the module and add dependencies**

In `crates/runtime/src/runtime.rs`, add:

```rust
mod worker_protocol;

pub use worker_protocol::{
    LiveWorkerCommand, LiveWorkerEvent, LiveWorkerLaunchSpec, parse_worker_command_line,
    parse_worker_event_line, worker_command_line, worker_event_line,
};
```

In `crates/runtime/Cargo.toml`, add:

```toml
serde.workspace = true
toml.workspace = true
```

- [ ] **Step 5: Run protocol tests**

Run:

```powershell
cargo test -p runtime --test worker_protocol_tests
cargo check -p runtime
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add crates/runtime/src/worker_protocol.rs crates/runtime/src/runtime.rs crates/runtime/src/run_spec.rs crates/runtime/src/live.rs crates/runtime/Cargo.toml crates/runtime/tests/worker_protocol_tests.rs
git commit -m "feat: add live worker protocol"
```

### Task 2: Add `trader live-worker`

**Files:**
- Modify: `apps/trader-cli/src/main.rs`
- Modify: `apps/trader-cli/Cargo.toml`
- Modify: `apps/trader-cli/tests/cli_tests.rs`

**Interfaces:**
- Consumes: `runtime::LiveWorkerLaunchSpec`, `runtime::LiveWorkerCommand`, `runtime::LiveWorkerEvent`, `runtime::LiveRuntime::run`
- Produces: CLI command `trader live-worker --launch-file <path>` that emits JSONL events on stdout, reads JSONL commands on stdin, and returns exit code `0` for graceful stop or startup failure already persisted by `LiveRuntime`

- [ ] **Step 1: Add failing CLI smoke test for worker startup and shutdown**

Add this test to `apps/trader-cli/tests/cli_tests.rs`. Use a file-backed SQLite URL, not `sqlite::memory:`, because the worker is a separate process.

```rust
#[test]
fn live_worker_starts_and_stops_over_jsonl() {
    let temp = std::env::temp_dir().join(format!(
        "trader-live-worker-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp).unwrap();
    let db_path = temp.join("worker.sqlite");
    let launch_path = temp.join("launch.json");
    let config_content = format!(
        r#"
        [runtime]
        mode = "live"
        run_id = "cli-live-worker"

        [database]
        url = "sqlite:{}"

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
        db_path.display()
    );
    let launch = serde_json::json!({
        "run_id": "cli-live-worker",
        "db_url": format!("sqlite:{}", db_path.display()),
        "config_path": null,
        "config_content": config_content,
        "config_format": "TOML",
        "run_spec": null,
        "broker_snapshot_interval_ms": null,
        "startup_recovery_unmatched_open_orders_policy": "Fail"
    });
    std::fs::write(&launch_path, serde_json::to_vec(&launch).unwrap()).unwrap();

    let mut command = assert_cmd::Command::cargo_bin("trader").unwrap();
    let assert = command
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p trader-cli live_worker_starts_and_stops_over_jsonl --test cli_tests`

Expected: FAIL because `live-worker` is not a known subcommand.

- [ ] **Step 3: Add the hidden worker subcommand**

In `apps/trader-cli/src/main.rs`, add this `Command` variant:

```rust
LiveWorker {
    #[arg(long)]
    launch_file: String,
},
```

In `main`, dispatch it before operator-facing commands:

```rust
Command::LiveWorker { launch_file } => run_live_worker(&launch_file).await?,
```

- [ ] **Step 4: Implement `run_live_worker`**

Add a helper that:

1. Reads `LiveWorkerLaunchSpec` from JSON.
2. Calls `validate_no_embedded_secrets`.
3. Parses `config::AppConfig` from `launch.config_content` according to `launch.config_format`.
4. Connects and migrates `storage::Db::connect(&launch.db_url)`.
5. Builds `LiveRuntimeSettings` from the launch/config.
6. Emits `worker_started`.
7. Starts `LiveRuntime::run(cancel)` in a Tokio task.
8. Emits `runtime_started`.
9. Reads stdin lines; `health_check` emits `health`, `shutdown` emits `runtime_stopping` and cancels.
10. Emits `heartbeat` every 1 second while running.
11. Emits `runtime_stopped` if the runtime returns `Ok(())`; emits `runtime_failed` if it returns `Err(error)`.

Use this skeleton and adapt existing CLI helper names such as `log_writer_settings` where available:

```rust
async fn run_live_worker(launch_file: &str) -> Result<()> {
    let launch_bytes = tokio::fs::read(launch_file).await?;
    let launch: runtime::LiveWorkerLaunchSpec = serde_json::from_slice(&launch_bytes)?;
    launch.validate_no_embedded_secrets()?;
    let app_config = match launch.config_format.as_str() {
        "TOML" => config::AppConfig::from_toml_str(&launch.config_content)?,
        "JSON" => serde_json::from_str(&launch.config_content)?,
        other => anyhow::bail!("unsupported launch config_format {other}"),
    };
    let db = storage::Db::connect(&launch.db_url).await?;
    db.migrate().await?;

    let initial_cash = Decimal::from_str(&app_config.portfolio.initial_cash)?;
    let settings = runtime::LiveRuntimeSettings {
        run_id: launch.run_id.clone(),
        broker_kind: broker_kind_from_config(app_config.broker.kind),
        account_id: app_config.paper.account_id.clone(),
        base_currency: app_config.portfolio.base_currency.clone(),
        initial_cash,
        broker_snapshot_interval_ms: launch.broker_snapshot_interval_ms,
        alert_sink: live_alert_sink_settings(&app_config.live.alerts),
        logging: log_writer_settings(&app_config),
    };
    let broker = live_worker_broker_for_config(&app_config)?;
    let cancel = runtime::CancellationFlag::default();

    write_worker_event(runtime::LiveWorkerEvent::WorkerStarted {
        run_id: launch.run_id.clone(),
        pid: std::process::id(),
    })?;

    let runtime = runtime::LiveRuntime::new_with_broker(db, settings, broker)
        .with_startup_recovery_unmatched_open_orders_policy(
            launch.startup_recovery_unmatched_open_orders_policy,
        );
    let runtime_cancel = cancel.clone();
    let runtime_join = tokio::spawn(async move { runtime.run(runtime_cancel).await });

    write_worker_event(runtime::LiveWorkerEvent::RuntimeStarted {
        run_id: launch.run_id.clone(),
    })?;

    // Use tokio::select! over stdin lines, heartbeat interval, and runtime_join.
    // On shutdown, call cancel.cancel() and continue waiting for runtime_join.
    // Flush stdout after each event so the supervisor can observe progress immediately.
    Ok(())
}
```

Keep `live_worker_broker_for_config` credential-free by default: `Simulated`, `Futu`, `Okx`, and `Binance` use `FakeBrokerAdapter`; `InteractiveBrokers` may construct `IbkrPaperGatewayAdapter` only when the existing config validation allows paper mode.

- [ ] **Step 5: Run CLI worker tests**

Run:

```powershell
cargo test -p trader-cli live_worker_starts_and_stops_over_jsonl --test cli_tests
cargo check -p trader-cli
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add apps/trader-cli/src/main.rs apps/trader-cli/Cargo.toml apps/trader-cli/tests/cli_tests.rs
git commit -m "feat: add live worker command"
```

### Task 3: Add `LiveProcessSupervisor`

**Files:**
- Create: `crates/runtime/src/process.rs`
- Create: `crates/runtime/tests/live_process_supervisor_tests.rs`
- Modify: `crates/runtime/src/runtime.rs`

**Interfaces:**
- Consumes: `LiveWorkerLaunchSpec`, `LiveWorkerCommand`, `LiveWorkerEvent`, `storage::Db`
- Produces: `LiveProcessSupervisor::new(db: Db) -> Self`, `LiveProcessSupervisor::with_options(db: Db, options: LiveProcessSupervisorOptions) -> Self`, `start(run_id: String, launch: LiveWorkerLaunchSpec) -> Result<(), LiveProcessError>`, `stop(run_id: &str) -> bool`, `snapshot(run_id: &str) -> Option<LiveProcessSnapshot>`, `is_active(run_id: &str) -> bool`, `check_heartbeats() -> usize`

- [ ] **Step 1: Add failing supervisor tests with fake worker process**

Create `crates/runtime/tests/live_process_supervisor_tests.rs`. Re-execute the test binary as a fake worker by running `std::env::current_exe()` with `--exact fake_live_worker_process --nocapture` and setting `TRADER_FAKE_LIVE_WORKER=healthy|crash_after_started|silent`.

Test names and expectations:

```rust
#[tokio::test]
async fn supervisor_rejects_duplicate_active_run_id() {
    // Start healthy fake worker for run-1.
    // Second start for run-1 returns LiveProcessError::AlreadyRunning.
    // Stop run-1 and assert snapshot status is StopRequested or Exited.
}

#[tokio::test]
async fn supervisor_records_heartbeat_and_health() {
    // Start healthy fake worker.
    // Wait until snapshot.last_heartbeat_at_ms is Some(_).
    // Assert snapshot.ipc_status == Some("running").
}

#[tokio::test]
async fn supervisor_marks_non_terminal_run_failed_on_crash() {
    // Insert a live strategy run with status "running" into a temp file-backed DB.
    // Start fake worker that emits worker_started/runtime_started and exits 17.
    // Wait for supervisor exit handling.
    // Assert DB status is "failed" and a runtime.live_process system log exists.
}

#[tokio::test]
async fn supervisor_kills_stale_heartbeat_worker() {
    // Start fake worker that handshakes but never heartbeats.
    // Configure heartbeat_stale_after_ms = 20 and health_response_timeout_ms = 20.
    // Run check_heartbeats().
    // Assert the run becomes failed in DB.
}
```

Add a normal ignored-by-default test body named `fake_live_worker_process` in the same file. When `TRADER_FAKE_LIVE_WORKER` is not set, it returns immediately; when set, it prints JSONL events and reads stdin commands:

```rust
#[test]
fn fake_live_worker_process() {
    let Ok(mode) = std::env::var("TRADER_FAKE_LIVE_WORKER") else {
        return;
    };
    let run_id = std::env::var("TRADER_FAKE_RUN_ID").unwrap_or_else(|_| "run-1".to_string());
    println!(r#"{{"type":"worker_started","run_id":"{run_id}","pid":{}}}"#, std::process::id());
    println!(r#"{{"type":"runtime_started","run_id":"{run_id}"}}"#);
    std::io::Write::flush(&mut std::io::stdout()).unwrap();
    match mode.as_str() {
        "crash_after_started" => std::process::exit(17),
        "silent" => std::thread::sleep(std::time::Duration::from_secs(60)),
        "healthy" => {
            println!(
                r#"{{"type":"heartbeat","run_id":"{run_id}","status":"running","ts_ms":1}}"#
            );
            std::io::Write::flush(&mut std::io::stdout()).unwrap();
            let mut line = String::new();
            while std::io::stdin().read_line(&mut line).unwrap() > 0 {
                if line.contains("\"shutdown\"") {
                    println!(r#"{{"type":"runtime_stopped","run_id":"{run_id}","status":"stopped"}}"#);
                    std::io::Write::flush(&mut std::io::stdout()).unwrap();
                    return;
                }
                line.clear();
            }
        }
        other => panic!("unknown fake worker mode {other}"),
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p runtime live_process_supervisor --test live_process_supervisor_tests`

Expected: FAIL because `LiveProcessSupervisor` does not exist.

- [ ] **Step 3: Implement supervisor data types**

In `crates/runtime/src/process.rs`, define:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveProcessSupervisorOptions {
    pub trader_exe: std::path::PathBuf,
    pub launch_root: std::path::PathBuf,
    pub handshake_timeout_ms: u64,
    pub graceful_shutdown_timeout_ms: u64,
    pub heartbeat_stale_after_ms: u64,
    pub health_response_timeout_ms: u64,
    pub stderr_line_limit: usize,
    pub extra_args: Vec<String>,
    pub extra_env: Vec<(String, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveProcessStatus {
    Starting,
    Running,
    StopRequested,
    Exited,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveProcessSnapshot {
    pub run_id: String,
    pub pid: Option<u32>,
    pub status: LiveProcessStatus,
    pub started_at_ms: i64,
    pub last_state_change_at_ms: i64,
    pub last_heartbeat_at_ms: Option<i64>,
    pub ipc_status: Option<String>,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiveProcessError {
    AlreadyRunning,
    LaunchFailed(String),
    HandshakeTimeout,
}
```

Default `trader_exe` should be `std::env::current_exe()` with the executable filename changed to `trader.exe` on Windows or `trader` elsewhere when possible. Tests override it with the test binary path.

- [ ] **Step 4: Implement `start` and event readers**

`start` must:

1. Reject duplicate active run IDs.
2. Write `launch.json` under `launch_root/<run_id>/launch.json`.
3. Spawn `trader_exe live-worker --launch-file <launch.json>` plus `extra_args`.
4. Pipe stdin/stdout/stderr.
5. Insert a `LiveProcessStatus::Starting` registry entry.
6. Spawn stdout reader task that parses `LiveWorkerEvent` and updates snapshot fields.
7. Spawn stderr reader task that stores at most `stderr_line_limit` recent lines.
8. Spawn wait task that calls exit handling.
9. Wait for `worker_started` and `runtime_started` until `handshake_timeout_ms`; on timeout, kill child and mark DB failed.

Use `tokio::process::Command`, `tokio::io::AsyncBufReadExt`, and an `Arc<Mutex<HashMap<String, LiveChildHandle>>>`.

- [ ] **Step 5: Implement stop, heartbeat, and exit handling**

`stop` must write:

```json
{"type":"shutdown","request_id":"stop-<run_id>","reason":"api_stop"}
```

Then wait up to `graceful_shutdown_timeout_ms`; if the child is still active, kill it. Preserve persisted `stopped` when `LiveRuntime` already wrote it; otherwise mark failed with a `runtime.live_process` system log.

`check_heartbeats` must:

1. Find active children with `last_heartbeat_at_ms` older than `heartbeat_stale_after_ms`.
2. Send `health_check`.
3. If no matching `health` response arrives within `health_response_timeout_ms`, kill child and mark the DB run failed.

Unexpected exit handling must:

1. Read `db.get_strategy_run(run_id)`.
2. If DB state is terminal (`completed`, `failed`, `cancelled`, `stopped`), leave it untouched.
3. If DB state is missing or non-terminal, write `failed` and a `runtime.live_process` system log with exit code and bounded stderr.

- [ ] **Step 6: Export supervisor API**

In `crates/runtime/src/runtime.rs`, add:

```rust
mod process;

pub use process::{
    LiveProcessError, LiveProcessSnapshot, LiveProcessStatus, LiveProcessSupervisor,
    LiveProcessSupervisorOptions,
};
```

- [ ] **Step 7: Run supervisor tests**

Run:

```powershell
cargo test -p runtime --test live_process_supervisor_tests
cargo test -p runtime --test worker_protocol_tests
cargo check -p runtime
```

Expected: PASS.

- [ ] **Step 8: Commit**

```powershell
git add crates/runtime/src/process.rs crates/runtime/src/runtime.rs crates/runtime/tests/live_process_supervisor_tests.rs
git commit -m "feat: supervise live worker processes"
```

### Task 4: Route Live API Through The Supervisor

**Files:**
- Modify: `crates/api/src/state.rs`
- Modify: `crates/api/src/api.rs`
- Modify: `crates/api/tests/api_tests.rs`

**Interfaces:**
- Consumes: `LiveProcessSupervisor::start`, `LiveProcessSupervisor::stop`, `LiveProcessSupervisor::snapshot`
- Produces: API-started Live runs execute in child processes; `/api/v1/live-runs`, `/status`, and `/stop` response shapes remain compatible.

- [ ] **Step 1: Add failing API assertion for process-backed Live status**

Extend `live_runtime_routes_start_report_status_and_stop` in `crates/api/tests/api_tests.rs` so it still asserts:

```rust
assert_eq!(response.status(), StatusCode::ACCEPTED);
wait_for_body_fragment(app.clone(), "/api/v1/live-runs/sample-ma-cross/status", "running").await;
```

Add a DB/system-log assertion after stop that no `runtime.live_process` error was written for the normal path:

```rust
let logs = db
    .list_system_logs_filtered(storage::SystemLogFilter {
        run_id: Some("sample-ma-cross".to_string()),
        target: Some("runtime.live_process".to_string()),
        level: Some("ERROR".to_string()),
        from_ms: None,
        to_ms: None,
        search: None,
        limit: None,
        offset: None,
    })
    .await
    .unwrap();
assert!(logs.is_empty());
```

If the test currently moves `db` into `AppState`, clone it before constructing state.

- [ ] **Step 2: Run API test to verify it still exercises old path**

Run: `cargo test -p api live_runtime_routes_start_report_status_and_stop --test api_tests`

Expected before implementation: FAIL or hang once the test expects process-specific behavior not yet wired.

- [ ] **Step 3: Add supervisor to `AppState`**

In `crates/api/src/state.rs`, add:

```rust
pub live_process_supervisor: runtime::LiveProcessSupervisor,
```

Initialize it in `AppState::with_server_config`:

```rust
let live_process_supervisor = runtime::LiveProcessSupervisor::new(db.clone());
```

Keep `runtime_manager` for Backtest/Paper/Replay and any existing in-process routes.

- [ ] **Step 4: Build launch files from `start_live_run`**

Replace the direct `RuntimeManager::spawn_with_metadata` Live path in `crates/api/src/api.rs` with:

```rust
let launch = runtime::LiveWorkerLaunchSpec {
    run_id: run_id.clone(),
    db_url: app_config.database.url.clone(),
    config_path: None,
    config_content: snapshot.content.clone(),
    config_format: snapshot.format.to_string(),
    run_spec: Some(run_spec.clone()),
    broker_snapshot_interval_ms: app_config.live.broker_snapshot_interval_ms,
    startup_recovery_unmatched_open_orders_policy:
        startup_recovery_unmatched_open_orders_policy(&app_config),
};
launch.validate_no_embedded_secrets()?;
state.live_process_supervisor.start(run_id.clone(), launch).await?;
```

Remove API-side Live broker construction from the start path. Keep `live_broker_for_config` only if other API code still uses it; otherwise delete it in a separate cleanup step inside this task.

- [ ] **Step 5: Route Live stop to supervisor**

Replace `stop_live_run` body with:

```rust
state.live_process_supervisor.stop(&run_id).await;
get_run_status(State(state), Path(run_id)).await
```

Do not update DB status in the API stop handler; let the worker persist `stopped`, and let supervisor failure handling write `failed` only when graceful stop does not complete.

- [ ] **Step 6: Merge process state into status response**

In `get_run_status`, before returning a storage-only response for Live runs, check:

```rust
if let Some(process) = state.live_process_supervisor.snapshot(&run_id).await
    && matches!(
        process.status,
        runtime::LiveProcessStatus::Starting
            | runtime::LiveProcessStatus::Running
            | runtime::LiveProcessStatus::StopRequested
    )
{
    return Ok(Json(RunStatusResponse {
        run_id,
        status: if process.status == runtime::LiveProcessStatus::StopRequested {
            "stopping".to_string()
        } else {
            "running".to_string()
        },
        error: None,
        mode: Some("live".to_string()),
        started_at_ms: Some(process.started_at_ms),
        last_state_change_at_ms: Some(process.last_state_change_at_ms),
        status_source: "process",
        mode_source: Some("process"),
        timestamp_source: Some("process"),
    })
    .into_response());
}
```

Then fall back to the existing storage response.

- [ ] **Step 7: Run Live API compatibility tests**

Run:

```powershell
cargo test -p api live_runtime_routes_start_report_status_and_stop --test api_tests
cargo test -p api live_runtime_route_uses_configured_broker_snapshot_interval --test api_tests
cargo check -p api
```

Expected: PASS.

- [ ] **Step 8: Commit**

```powershell
git add crates/api/src/state.rs crates/api/src/api.rs crates/api/tests/api_tests.rs
git commit -m "feat: route live api through process supervisor"
```

### Task 5: Cover Crash And Startup-Recovery Failure Semantics

**Files:**
- Modify: `crates/api/tests/api_tests.rs`
- Modify: `crates/runtime/tests/live_process_supervisor_tests.rs`
- Modify as needed: `crates/runtime/src/process.rs`

**Interfaces:**
- Consumes: process exit handling from Task 3 and API wiring from Task 4.
- Produces: deterministic regression coverage for unexpected worker exit, handshake timeout, stale heartbeat, and startup recovery failure reason preservation.

- [ ] **Step 1: Add API startup recovery failure preservation test**

In `crates/api/tests/api_tests.rs`, extend the existing startup recovery failure test around `api-live-startup-recovery-fail` to assert the stored error contains the specific recovery reason:

```rust
wait_for_body_fragment(
    app.clone(),
    "/api/v1/live-runs/api-live-startup-recovery-fail/status",
    "unmatched remote open orders during startup recovery",
)
.await;

let run = db
    .get_strategy_run("api-live-startup-recovery-fail")
    .await
    .unwrap()
    .unwrap();
assert_eq!(run.status, "failed");
assert!(
    run.error
        .as_deref()
        .unwrap_or_default()
        .contains("unmatched remote open orders during startup recovery")
);
```

- [ ] **Step 2: Add API normal stop preservation test**

Add an assertion to the normal stop test:

```rust
let run = db.get_strategy_run("sample-ma-cross").await.unwrap().unwrap();
assert_eq!(run.status, "stopped");
assert!(run.error.is_none());
```

- [ ] **Step 3: Add handshake timeout supervisor test**

In `crates/runtime/tests/live_process_supervisor_tests.rs`, add:

```rust
#[tokio::test]
async fn supervisor_fails_run_on_handshake_timeout() {
    // Fake worker mode "silent_before_handshake" sleeps without printing worker_started.
    // Configure handshake_timeout_ms = 20.
    // start(...) returns LiveProcessError::HandshakeTimeout.
    // DB run is failed and runtime.live_process ERROR log contains "handshake timeout".
}
```

Extend `fake_live_worker_process` with `silent_before_handshake`.

- [ ] **Step 4: Fix failure handling until tests pass**

If the API startup recovery test shows a generic process-exit error replacing the existing DB error, update supervisor exit handling:

```rust
if let Some(run) = self.db.get_strategy_run(run_id).await?
    && is_terminal_run_status(&run.status)
{
    self.record_exit_diagnostic_only(run_id, exit_code, stderr).await?;
    return Ok(());
}
```

Use the same terminal set as the API: `completed`, `failed`, `cancelled`, `stopped`.

- [ ] **Step 5: Run targeted failure tests**

Run:

```powershell
cargo test -p runtime --test live_process_supervisor_tests
cargo test -p api live_startup_recovery --test api_tests
cargo test -p api live_runtime_routes_start_report_status_and_stop --test api_tests
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add crates/api/tests/api_tests.rs crates/runtime/tests/live_process_supervisor_tests.rs crates/runtime/src/process.rs
git commit -m "test: cover live process failure handling"
```

### Task 6: Run Full Verification And Update Docs

**Files:**
- Modify: `docs/architecture.md`
- Modify: `docs/api.md`
- Modify if needed: `docs/web-admin-api.md`

**Interfaces:**
- Consumes: completed process-isolated Live implementation.
- Produces: documented behavior and a verified implementation ready for review.

- [ ] **Step 1: Document architecture**

Add a short Live section to `docs/architecture.md`:

```markdown
### Live Process Isolation

API-started Live runs execute in a supervised local child process launched as
`trader live-worker --launch-file <path>`. The API server owns HTTP request
validation, launch-file creation, process supervision, heartbeat health, and
stop requests. The worker owns `LiveRuntime::run` and continues to write the
existing SQLite run state, runtime events, system logs, reconciliation
snapshots, alert logs, and alert delivery logs.
```

- [ ] **Step 2: Document unchanged API surface**

Add to `docs/api.md` near the Live endpoints:

```markdown
Live runs started through `POST /api/v1/live-runs` are process-isolated
internally. The public request and response shape is unchanged; status may use
fresh supervisor process state while the child is active and falls back to
SQLite terminal state after exit.
```

- [ ] **Step 3: Run crate-level tests**

Run:

```powershell
cargo test -p runtime
cargo test -p api
cargo test -p trader-cli
cargo check -p runtime -p api -p trader-cli
```

Expected: PASS.

- [ ] **Step 4: Run recovery verification scripts**

Run:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\verify-live-recovery.ps1 -Iterations 1
powershell -ExecutionPolicy Bypass -File .\scripts\verify-live-recovery.ps1 -Iterations 20 -DelaySeconds 1
```

Expected: both scripts finish with zero non-zero runtime exits and no credential prompts.

- [ ] **Step 5: Run workspace verification**

Run:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\verify.ps1
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add docs/architecture.md docs/api.md docs/web-admin-api.md
git commit -m "docs: document live process isolation"
```

---

## Self-Review

- Spec coverage: Tasks cover worker entry point, JSONL IPC, launch file redaction, process state, SQLite run state preservation, launch failure, handshake timeout, heartbeat stale, unexpected exit, graceful shutdown timeout, startup recovery failure preservation, API compatibility, and recovery verification scripts.
- Placeholder scan: No placeholder markers or open-ended "handle edge cases" steps remain; each task has concrete files, commands, and expected outcomes.
- Type consistency: `LiveWorkerLaunchSpec`, `LiveWorkerCommand`, `LiveWorkerEvent`, `LiveProcessSupervisor`, `LiveProcessSnapshot`, and `LiveProcessStatus` names are introduced before later tasks consume them.
