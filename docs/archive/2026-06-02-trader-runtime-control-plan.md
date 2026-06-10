# Trader Runtime Control and Server Smoke Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Execute inline; do not dispatch subagents.

**Goal:** Add a minimal runtime control surface and reliable server smoke workflow so paper/backtest runs can be started, queried, failed, and cancelled through explicit APIs.

**Architecture:** Keep the runtime manager small and synchronous for this phase. `storage` remains the only SQL owner and stores status/error transitions. `api` exposes command/query endpoints and maps storage/runtime errors to stable HTTP responses. Scripts exercise a real running `trader-server` when the local Windows target directory allows binary builds.

**Tech Stack:** Rust 2024, Tokio, Axum, SQLx SQLite, serde, rust_decimal, PowerShell smoke scripts.

---

## Current Baseline

- Phase 4 is merged to `main`.
- `POST /api/v1/paper-runs` runs local paper workflow and persists orders, fills, positions, account balances, portfolio snapshots, metrics inputs, and strategy run record.
- `POST /api/v1/backtests` runs backtest workflow.
- `GET /api/v1/runs` and `GET /api/v1/runs/{run_id}` query run records.
- `scripts/rest-smoke.ps1` validates a running server, but has not been executed in this shell because Windows refused `target` artifact writes/removals for `cargo build -p trader-server`.

## Execution Rules

- Use inline execution only; no subagents.
- Use TDD: write a failing test, run it, implement, verify it passes.
- Work in small commits after each task passes.
- Keep SQL inside `crates/storage`.
- Keep all financial values as `Decimal` in Rust and decimal strings in SQLite.
- Keep production tests in `tests/`; do not add inline `#[cfg(test)] mod tests`.
- Keep explicit library entry files: every library crate must use `[lib] path = "src/<crate_name>.rs"`.
- Use workspace dependencies (`foo.workspace = true`) for internal crates.

## File Structure

Modify:

- `migrations/0001_init.sql`
- `crates/storage/src/repositories.rs`
- `crates/storage/tests/runtime_repository_tests.rs`
- `crates/api/src/api.rs`
- `crates/api/tests/backtest_api_tests.rs`
- `apps/trader-server/src/main.rs`
- `README.md`
- `tech.md`
- `docs/superpowers/plans/2026-06-02-trader-runtime-control-plan.md`

Create:

- `scripts/server-smoke.ps1`

---

### Task 1: Run Status Error Persistence

**Files:**
- Modify: `migrations/0001_init.sql`
- Modify: `crates/storage/src/repositories.rs`
- Modify: `crates/storage/tests/runtime_repository_tests.rs`

- [x] **Step 1: Add failing storage test for run error updates**

In `crates/storage/tests/runtime_repository_tests.rs`, after inserting `run-1`, add:

```rust
db.update_strategy_run_status("run-1", "failed", Some(9), Some("boom"))
    .await
    .unwrap();
let failed = db.get_strategy_run("run-1").await.unwrap().unwrap();
assert_eq!(failed.status, "failed");
assert_eq!(failed.ended_at_ms, Some(9));
assert_eq!(failed.error, Some("boom".to_string()));
```

- [x] **Step 2: Run test and verify RED**

Run:

```powershell
cargo test -p storage
```

Expected: FAIL because `update_strategy_run_status` and `StrategyRunRecord.error` are missing.

- [x] **Step 3: Extend schema and record**

In `migrations/0001_init.sql`, add `error TEXT` to `strategy_runs`.

Update `NewStrategyRun` and `StrategyRunRecord`:

```rust
pub error: Option<String>,
```

Update all inserts to bind `error`.

- [x] **Step 4: Implement status update**

Add to `Db`:

```rust
pub async fn update_strategy_run_status(
    &self,
    run_id: &str,
    status: &str,
    ended_at_ms: Option<i64>,
    error: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE strategy_runs
        SET status = ?, ended_at_ms = ?, error = ?
        WHERE id = ?
        "#,
    )
    .bind(status)
    .bind(ended_at_ms)
    .bind(error)
    .bind(run_id)
    .execute(self.pool())
    .await?;
    Ok(())
}
```

- [x] **Step 5: Verify and commit**

Run:

```powershell
cargo fmt --all -- --check
cargo test -p storage
cargo check --workspace --locked
```

Commit:

```powershell
git add migrations/0001_init.sql crates/storage
git commit -m "feat: persist run status errors"
```

---

### Task 2: API Run Status and Cancel

**Files:**
- Modify: `crates/api/src/api.rs`
- Modify: `crates/api/tests/backtest_api_tests.rs`

- [x] **Step 1: Add failing API tests**

Add tests that:

```rust
POST /api/v1/paper-runs
GET /api/v1/runs/sample-ma-cross/status
POST /api/v1/runs/sample-ma-cross/cancel
GET /api/v1/runs/sample-ma-cross/status
```

Assert:

- first status response is `200 OK` and contains `"completed"`;
- cancel response is `200 OK`;
- second status response contains `"cancelled"`.

- [x] **Step 2: Run API test and verify RED**

Run:

```powershell
cargo test -p api
```

Expected: FAIL with `404` for status/cancel routes.

- [x] **Step 3: Add response type**

In `crates/api/src/api.rs`, add:

```rust
#[derive(Serialize)]
struct RunStatusResponse {
    run_id: String,
    status: String,
    error: Option<String>,
}
```

- [x] **Step 4: Implement routes**

Add:

```rust
.route("/api/v1/runs/{run_id}/status", get(get_run_status))
.route("/api/v1/runs/{run_id}/cancel", post(cancel_run))
```

Implement:

- `get_run_status`: return 404 when missing, else `RunStatusResponse`;
- `cancel_run`: call `update_strategy_run_status(run_id, "cancelled", Some(now_ms), None)` and return updated status.

Use `chrono::Utc::now().timestamp_millis()` for `now_ms`; add `chrono.workspace = true` to `crates/api/Cargo.toml` if needed.

- [x] **Step 5: Verify and commit**

Run:

```powershell
cargo fmt --all -- --check
cargo test -p api
cargo check --workspace --locked
```

Commit:

```powershell
git add crates/api Cargo.lock
git commit -m "feat: expose run status controls"
```

---

### Task 3: Failed Run Recording

**Files:**
- Modify: `crates/api/src/api.rs`
- Modify: `crates/api/tests/backtest_api_tests.rs`

- [x] **Step 1: Add failing test for failed paper run**

Create an API test with a config path that points to a missing CSV:

```rust
let app = router_with_state(AppState::new(db, "configs/backtest/missing-bars.toml".into()));
```

Instead of relying on a file that does not exist, create a temporary config string is not available through current `AppState`; therefore add a test-only config file path under `configs/backtest/missing-bars.toml` in this task if needed.

Assert:

- `POST /api/v1/paper-runs` returns `500 INTERNAL_SERVER_ERROR`;
- `GET /api/v1/runs/sample-missing-bars/status` returns `failed`;
- response contains non-empty `error`.

- [x] **Step 2: Run API test and verify RED**

Run:

```powershell
cargo test -p api
```

Expected: FAIL because failed run state is not inserted/updated when execution fails before completion.

- [x] **Step 3: Implement failure recording**

In `run_paper`, insert a `strategy_runs` record with status `running` before loading bars/running runtime:

```rust
state.db.insert_strategy_run(storage::NewStrategyRun {
    id: settings.run_id.clone(),
    name: settings.strategy_name.clone(),
    mode: "paper".to_string(),
    status: "running".to_string(),
    started_at_ms,
    ended_at_ms: None,
    config_json: "{}".to_string(),
    error: None,
}).await?;
```

On error, call:

```rust
state.db
    .update_strategy_run_status(&settings.run_id, "failed", Some(now_ms), Some(&error.to_string()))
    .await?;
```

Then return the original error.

- [x] **Step 4: Verify and commit**

Run:

```powershell
cargo fmt --all -- --check
cargo test -p api
cargo check --workspace --locked
```

Commit:

```powershell
git add crates/api configs/backtest/missing-bars.toml
git commit -m "feat: record failed paper runs"
```

---

### Task 4: Server Smoke Script with Build Workaround

**Files:**
- Create: `scripts/server-smoke.ps1`
- Modify: `README.md`

- [x] **Step 1: Create script**

Create `scripts/server-smoke.ps1`:

```powershell
$ErrorActionPreference = "Stop"

$targetDir = $env:TRADER_SMOKE_TARGET_DIR
if (-not $targetDir) {
    $targetDir = Join-Path $env:TEMP "trader-smoke-target"
}

$env:CARGO_TARGET_DIR = $targetDir
$env:TRADER_DATABASE_URL = "sqlite://data/server-smoke.sqlite"

$server = Start-Process -FilePath "cargo" `
    -ArgumentList @("run", "-p", "trader-server") `
    -WorkingDirectory (Get-Location) `
    -PassThru `
    -WindowStyle Hidden

try {
    $ready = $false
    for ($i = 0; $i -lt 80; $i++) {
        Start-Sleep -Milliseconds 500
        try {
            Invoke-RestMethod "http://127.0.0.1:8080/api/v1/health" | Out-Null
            $ready = $true
            break
        } catch {}
    }
    if (-not $ready) { throw "trader-server did not become ready" }

    powershell -ExecutionPolicy Bypass -File ".\scripts\rest-smoke.ps1"
} finally {
    if ($server -and -not $server.HasExited) {
        Stop-Process -Id $server.Id -Force
    }
}
```

- [x] **Step 2: Run script**

Run:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\server-smoke.ps1
```

Expected:

- If Windows permits temp target writes, script prints smoke summary.
- If Windows still blocks cargo artifact writes, capture the exact failure in the plan and keep API integration tests as fallback evidence.

Result: Passed locally with `signals=1`, `orders=1`, `fills=1`, `balances=1`, `snapshots=3`.

- [x] **Step 3: Document script**

Update README to include:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\server-smoke.ps1
```

- [x] **Step 4: Verify and commit**

Run:

```powershell
cargo fmt --all -- --check
cargo test --workspace
```

Commit:

```powershell
git add scripts/server-smoke.ps1 README.md docs/superpowers/plans/2026-06-02-trader-runtime-control-plan.md
git commit -m "test: add server smoke script"
```

---

### Task 5: Final Verification and Documentation

**Files:**
- Modify: `README.md`
- Modify: `tech.md`
- Modify: `docs/superpowers/plans/2026-06-02-trader-runtime-control-plan.md`

- [x] **Step 1: Full verification**

Run:

```powershell
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace
cargo run -p trader-cli -- paper-run --config configs/backtest/ma_cross.toml
```

Expected CLI output includes:

```text
paper completed: signals=1 orders=1
```

- [x] **Step 2: Naming and dependency checks**

Run:

```powershell
Get-ChildItem crates -Directory | ForEach-Object { Join-Path $_.FullName 'src\lib.rs' } | Where-Object { Test-Path $_ }
rg "= \{ path =" apps crates -g Cargo.toml
```

Expected: both commands produce no matches.

- [x] **Step 3: Update docs**

README must include:

- run status route;
- cancel route;
- server smoke script.

`tech.md` must include:

- run lifecycle statuses;
- error persistence;
- cancel semantics for current synchronous runtime.

- [x] **Step 4: Mark plan complete and commit**

Commit:

```powershell
git add README.md tech.md docs/superpowers/plans/2026-06-02-trader-runtime-control-plan.md
git commit -m "docs: document runtime control workflow"
```

---

## Acceptance Criteria

This phase is complete when:

- `cargo fmt --all -- --check` passes.
- `cargo check --workspace --locked` passes.
- `cargo test --workspace` passes.
- `trader paper-run --config configs/backtest/ma_cross.toml` prints `paper completed: signals=1 orders=1`.
- Run records persist `status`, `ended_at_ms`, and `error`.
- REST exposes:
  - `GET /api/v1/runs/{run_id}/status`;
  - `POST /api/v1/runs/{run_id}/cancel`.
- Failed paper runs record status `failed` and a non-empty error.
- Server smoke script exists and either runs successfully or records the exact local cargo/Windows artifact blocker.
- Crate root naming convention remains satisfied: no library crate uses default `src/lib.rs`.
- Member crates do not use direct internal `{ path = ... }` dependencies.

## Self-Review

Spec coverage:

- Server smoke issue: Task 4.
- Runtime status/control API: Tasks 1 and 2.
- Failed run recording: Task 3.
- Documentation and verification: Task 5.

Placeholder scan:

- No open-ended implementation steps remain.
- Each task lists files, commands, expected behavior, and commit message.

Type consistency:

- `StrategyRunRecord.error` is introduced before API status responses use it.
- `update_strategy_run_status` is introduced before cancel/failure recording uses it.
