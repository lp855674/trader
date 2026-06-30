# Live Process Isolation Design

## Goal

Move Live runtime execution out of the API server process while preserving the existing Live HTTP surface, SQLite audit trail, startup recovery behavior, reconciliation snapshots, and alert delivery semantics.

## Current Context

Live runs are currently launched by `crates/api/src/api.rs::start_live_run`, which parses config, persists the run config snapshot, constructs a broker adapter, and registers an in-process Tokio task with `runtime::RuntimeManager::spawn_with_metadata`. The spawned task constructs `runtime::LiveRuntime` and calls `LiveRuntime::run(cancel)`.

`LiveRuntime` owns the run lifecycle after launch. It writes `strategy_runs`, `runtime_events`, `system_logs`, cash snapshots, position snapshots, reconciliation drift risk events, alert logs, and alert delivery logs. Startup recovery reads local recoverable orders, broker open orders, and broker executions, then either recovers local order/fill state or fails the run according to the configured unmatched-open-order policy.

The existing long-run verification pass `live-recovery-df3cec2a63f1` completed 20 local fake/injected broker iterations with 320 targeted runtime test invocations and zero non-zero exits. This design starts from that verified recovery boundary.

## Chosen Approach

Use a supervised child process with a small JSONL IPC control plane:

- API process remains the external HTTP control surface.
- A Live process supervisor starts one child process per Live run.
- The child process executes the existing `LiveRuntime` logic.
- `stdin` carries supervisor-to-worker JSONL commands.
- `stdout` carries worker-to-supervisor JSONL events.
- `stderr` remains process diagnostics and is captured into system logs.
- SQLite remains the source of truth for business state and audit records.

This is intentionally not a service split. The worker is a local child process owned by the API process, so the first implementation can deliver crash isolation, graceful shutdown, heartbeat health, and crash-to-failed status without introducing HTTP/gRPC worker deployment, auth, service discovery, or remote scheduling.

## Non-Goals

- Do not submit real broker orders as part of the default implementation or tests.
- Do not replace SQLite run state, events, snapshots, or alert logs with IPC state.
- Do not move Paper, Backtest, or Replay into isolated processes in this phase.
- Do not introduce a distributed worker service, queue, or orchestrator.
- Do not add a second public Live API shape unless existing status responses cannot represent the new state.

## Architecture

```text
HTTP client
  |
  v
API server process
  |
  | RuntimeProcessManager
  |   - active child registry
  |   - stdin command writer
  |   - stdout event reader
  |   - stderr log reader
  |   - heartbeat tracking
  |   - exit handling
  v
Live worker child process
  |
  | LiveRuntime::run(cancel)
  v
SQLite + broker adapter + system logs
```

The API server remains responsible for request validation, config snapshot persistence, and public response mapping. The worker is responsible for reconstructing the runtime from a run launch spec, running startup recovery, recording snapshots and reconciliation drift, sending alerts, and handling graceful cancellation.

## Worker Entry Point

The first implementation should add a `trader-cli live-worker` subcommand rather than a new workspace binary. The existing CLI already owns command-line entry points, config loading patterns, and operator scripts. A subcommand keeps packaging simple while still producing a real child process.

The worker command accepts a local launch file path:

```powershell
trader live-worker --launch-file data/live-process/<run_id>/launch.json
```

The launch file contains:

- `run_id`
- absolute or repo-relative config path when available
- raw config TOML snapshot or a path to the persisted snapshot
- resolved run spec fields needed to rebuild `LiveRuntimeSettings`
- broker selection fields
- startup recovery unmatched-open-order policy
- logging settings

The launch file must not contain broker secrets. Existing credential mechanisms remain environment-variable based or local config based, and tests must use fake/injected broker paths by default.

## IPC Protocol

All IPC messages are one JSON object per line.

### Supervisor Commands

```json
{"type":"health_check","request_id":"health-1"}
{"type":"shutdown","request_id":"stop-1","reason":"api_stop"}
```

`health_check` asks the worker to respond immediately with current process health. It must not query the broker or mutate run state.

`shutdown` asks the worker to cancel the `LiveRuntime` cancellation flag, wait for `LiveRuntime::run` to complete, and then emit a terminal event. If graceful shutdown times out, the supervisor may kill the child process and mark the run failed or stopped according to the last persisted DB state.

### Worker Events

```json
{"type":"worker_started","run_id":"live-1","pid":1234}
{"type":"runtime_started","run_id":"live-1"}
{"type":"heartbeat","run_id":"live-1","status":"running","ts_ms":1800000000000}
{"type":"health","run_id":"live-1","request_id":"health-1","status":"running"}
{"type":"runtime_stopping","run_id":"live-1","reason":"api_stop"}
{"type":"runtime_stopped","run_id":"live-1","status":"stopped"}
{"type":"runtime_failed","run_id":"live-1","error":"startup recovery failed"}
```

Worker events are operational signals, not the audit truth. The worker must still write the existing DB records that tests and APIs already rely on.

## State Model

There are two layers of state:

- Process state: maintained by the supervisor in memory. Examples: child PID, last heartbeat time, last IPC status, stdin availability, exit code, and whether shutdown was requested.
- Run state: persisted in SQLite by `LiveRuntime` and storage repositories. Examples: `running`, `stopped`, `failed`, startup recovery logs, snapshots, risk events, alert logs, and delivery logs.

Status responses should use both:

- If a child is active and heartbeat is fresh, report the run as active even if a DB read is briefly stale.
- If no child is active, report the persisted DB terminal state.
- If the child exits unexpectedly while DB state is non-terminal, supervisor records a `runtime.live_process` system log and marks the run `failed`.
- If worker startup recovery fails and writes a DB failure, supervisor must preserve that failure reason instead of overwriting it with a generic process-exit message.

## Failure Handling

The supervisor classifies failures as:

- Worker launch failure: no child process started. Mark run `failed` and record `runtime.live_process` error.
- IPC handshake timeout: child started but did not emit `worker_started` or `runtime_started` in time. Kill child, mark run `failed`, record system log.
- Heartbeat stale: child process exists but stopped sending events. Send `health_check`; if unanswered, kill child and mark run `failed`.
- Unexpected process exit: if DB run state is non-terminal, mark run `failed`; otherwise record exit diagnostics only.
- Graceful shutdown timeout: kill child; preserve `stopped` if already persisted, otherwise mark `failed`.

Backoff/restart is out of scope for the first implementation. The design should leave room for a later explicit restart policy, but automatic restart after process crash must not happen silently because it could re-enter broker recovery paths without operator visibility.

## Security And Safety

- Default tests and scripts must use fake/injected brokers only.
- Launch files must not include API keys, secret keys, bearer tokens, or account credentials.
- Child process command lines must avoid embedding secrets.
- Worker stderr capture must be bounded or streamed to avoid unbounded memory growth.
- Shutdown must prefer graceful cancellation before kill.
- No IPC command may submit, cancel, or mutate broker orders in this phase.

## Testing Strategy

The implementation plan should keep tests local and deterministic:

- Runtime manager tests for child registry, heartbeat tracking, duplicate run rejection, and terminal state handling.
- Worker protocol tests that feed stdin JSONL and assert stdout JSONL events.
- API route tests showing `/api/v1/live-runs`, `/status`, and `/stop` remain compatible.
- Crash handling tests with a fake worker command that exits non-zero after handshake.
- Startup recovery failure propagation tests proving a worker-reported Live failure persists the existing DB failure reason.
- Existing long-run verification script must keep passing:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\verify-live-recovery.ps1 -Iterations 1
powershell -ExecutionPolicy Bypass -File .\scripts\verify-live-recovery.ps1 -Iterations 20 -DelaySeconds 1
```

## Acceptance Criteria

- API-started Live runs execute in a child process, not an in-process Tokio task.
- Existing Live start/status/stop API tests remain compatible.
- Stop requests use IPC shutdown before force-kill.
- Supervisor can detect heartbeat freshness.
- Unexpected child exit marks a non-terminal run failed and writes a `runtime.live_process` system log.
- Worker startup recovery failure preserves the specific Live failure reason in SQLite.
- Default verification remains credential-free and does not touch real brokers.
- The implementation does not weaken the previously verified startup recovery, reconciliation drift, or alert delivery behavior.

## Implementation Notes

The likely code split is:

- `crates/runtime/src/process.rs`: process supervisor, child handles, IPC event model, heartbeat state.
- `crates/runtime/src/worker_protocol.rs`: command/event JSON types and JSONL parsing helpers.
- `apps/trader-cli/src/main.rs`: `live-worker` subcommand that reads launch file and runs `LiveRuntime`.
- `crates/api/src/api.rs`: replace direct Live `spawn_with_metadata` with process supervisor launch.
- `crates/api/src/state.rs`: store the process-aware runtime manager or a new supervisor field.

If the existing `RuntimeManager` remains useful for Paper/Backtest-like in-process tasks, do not force it to model child processes. Prefer adding a focused `LiveProcessSupervisor` and wiring only Live to it.
