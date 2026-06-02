# Trader Runtime Manager Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Execute inline; do not dispatch subagents.

**Goal:** Move paper REST execution from synchronous request handling to a small background runtime manager with real active-run cancellation.

**Architecture:** Add a dedicated `runtime` crate for in-memory task registry and cancellation flags. Keep SQL ownership in `storage`; API owns request/response mapping and starts background jobs through `RuntimeManager`; `paper` checks cancellation during the bar loop and returns a cancellation error instead of writing `completed`.

**Tech Stack:** Rust 2024, Tokio, Axum, SQLx SQLite, serde, rust_decimal, PowerShell smoke scripts.

---

## Current Baseline

- `main` includes Phase 5 runtime control.
- `POST /api/v1/paper-runs` currently runs paper synchronously and returns `201 CREATED` with `BacktestSummary`.
- `GET /api/v1/runs/{run_id}/status` returns persisted status.
- `POST /api/v1/runs/{run_id}/cancel` currently marks an existing run `cancelled`, but cannot interrupt an active synchronous request.
- `PaperRuntime::run_bars` owns the bar loop and writes the final `completed` run record.
- `scripts/server-smoke.ps1` starts a real server with temp target/db and runs `scripts/rest-smoke.ps1`.

## Execution Rules

- Use inline execution only; no subagents.
- Use TDD: write failing tests, run them, implement, verify green.
- Work in small commits after each task passes.
- Keep SQL inside `crates/storage`.
- Keep production tests in `tests/`; no inline `#[cfg(test)] mod tests`.
- Keep explicit library entry files: every library crate must use `[lib] path = "src/<crate_name>.rs"`.
- Use workspace dependencies (`foo.workspace = true`) for internal crates.
- Do not use direct member `{ path = ... }` dependencies outside workspace root.

## File Structure

Create:

- `crates/runtime/Cargo.toml`: runtime crate manifest with `[lib] path = "src/runtime.rs"`.
- `crates/runtime/src/runtime.rs`: re-export modules.
- `crates/runtime/src/cancel.rs`: cancellation flag shared by manager and runtime loops.
- `crates/runtime/src/manager.rs`: in-memory active run registry and task spawning.
- `crates/runtime/tests/runtime_manager_tests.rs`: manager behavior tests.
- `crates/paper/tests/paper_cancellation_tests.rs`: paper runtime cancellation tests.
- `configs/backtest/slow-paper.toml`: slow paper fixture for API cancellation tests.

Modify:

- `Cargo.toml`: add `crates/runtime` workspace member and `runtime = { path = "crates/runtime" }` workspace dependency.
- `crates/paper/Cargo.toml`: add `runtime.workspace = true`.
- `crates/paper/src/paper.rs`: add cancellation-aware run path and optional per-bar delay.
- `crates/config/src/config.rs`: add optional paper `bar_delay_ms`.
- `crates/config/tests/config_tests.rs`: cover default and configured delay.
- `crates/api/Cargo.toml`: add `runtime.workspace = true`.
- `crates/api/src/state.rs`: store `RuntimeManager`.
- `crates/api/src/api.rs`: start paper in background, return accepted response, cancel active runs.
- `crates/api/tests/backtest_api_tests.rs`: update POST expectations and add active cancellation test.
- `scripts/rest-smoke.ps1`: poll status after accepted paper run.
- `README.md`: document async paper run start and status polling.
- `tech.md`: document runtime manager and cancellation semantics.
- `docs/superpowers/plans/2026-06-02-trader-runtime-manager-plan.md`: track execution.

---

### Task 1: Runtime Manager Crate

**Files:**
- Create: `crates/runtime/Cargo.toml`
- Create: `crates/runtime/src/runtime.rs`
- Create: `crates/runtime/src/cancel.rs`
- Create: `crates/runtime/src/manager.rs`
- Create: `crates/runtime/tests/runtime_manager_tests.rs`
- Modify: `Cargo.toml`

- [x] **Step 1: Add failing runtime manager tests**

Create `crates/runtime/tests/runtime_manager_tests.rs`:

```rust
use runtime::{RuntimeManager, RunSpawnError};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::Notify;

#[tokio::test]
async fn manager_tracks_active_run_and_cancels_it() {
    let manager = RuntimeManager::default();
    let started = Arc::new(Notify::new());
    let released = Arc::new(Notify::new());
    let observed_cancel = Arc::new(AtomicBool::new(false));

    let started_for_task = started.clone();
    let released_for_task = released.clone();
    let observed_for_task = observed_cancel.clone();
    manager
        .spawn("run-1".to_string(), move |cancel| async move {
            started_for_task.notify_one();
            released_for_task.notified().await;
            observed_for_task.store(cancel.is_cancelled(), Ordering::SeqCst);
        })
        .await
        .unwrap();

    started.notified().await;
    assert!(manager.is_active("run-1").await);
    assert!(manager.cancel("run-1").await);
    released.notify_one();
    manager.wait_for_idle("run-1").await;

    assert!(observed_cancel.load(Ordering::SeqCst));
    assert!(!manager.is_active("run-1").await);
}

#[tokio::test]
async fn manager_rejects_duplicate_active_run_id() {
    let manager = RuntimeManager::default();
    let released = Arc::new(Notify::new());
    let released_for_task = released.clone();

    manager
        .spawn("run-1".to_string(), move |_cancel| async move {
            released_for_task.notified().await;
        })
        .await
        .unwrap();

    let duplicate = manager.spawn("run-1".to_string(), |_cancel| async {}).await;
    assert_eq!(duplicate.unwrap_err(), RunSpawnError::AlreadyRunning);

    released.notify_one();
    manager.wait_for_idle("run-1").await;
}
```

- [x] **Step 2: Run runtime tests and verify RED**

Run:

```powershell
cargo test -p runtime
```

Expected: FAIL because the `runtime` crate and `RuntimeManager` do not exist.

- [x] **Step 3: Add workspace crate and manifest**

Modify root `Cargo.toml`:

```toml
members = [
    "apps/trader-cli",
    "apps/trader-server",
    ...
    "crates/replay",
    "crates/runtime",
    "crates/paper",
    ...
]

[workspace.dependencies]
runtime = { path = "crates/runtime" }
```

Create `crates/runtime/Cargo.toml`:

```toml
[package]
name = "runtime"
version.workspace = true
edition.workspace = true
license.workspace = true
rust-version.workspace = true

[lib]
path = "src/runtime.rs"

[dependencies]
tokio.workspace = true
```

- [x] **Step 4: Implement cancellation flag**

Create `crates/runtime/src/runtime.rs`:

```rust
#![forbid(unsafe_code)]

mod cancel;
mod manager;

pub use cancel::CancellationFlag;
pub use manager::{RunSpawnError, RuntimeManager};
```

Create `crates/runtime/src/cancel.rs`:

```rust
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Debug, Clone, Default)]
pub struct CancellationFlag {
    cancelled: Arc<AtomicBool>,
}

impl CancellationFlag {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}
```

- [x] **Step 5: Implement runtime manager**

Create `crates/runtime/src/manager.rs`:

```rust
use crate::CancellationFlag;
use std::{collections::HashMap, future::Future, sync::Arc};
use tokio::{sync::Mutex, task::JoinHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunSpawnError {
    AlreadyRunning,
}

#[derive(Clone, Default)]
pub struct RuntimeManager {
    inner: Arc<Mutex<HashMap<String, RunHandle>>>,
}

struct RunHandle {
    cancel: CancellationFlag,
    join: JoinHandle<()>,
}

impl RuntimeManager {
    pub async fn is_active(&self, run_id: &str) -> bool {
        self.inner.lock().await.contains_key(run_id)
    }

    pub async fn cancel(&self, run_id: &str) -> bool {
        let Some(handle) = self.inner.lock().await.get(run_id).map(|handle| handle.cancel.clone()) else {
            return false;
        };
        handle.cancel();
        true
    }

    pub async fn spawn<F, Fut>(&self, run_id: String, task: F) -> Result<(), RunSpawnError>
    where
        F: FnOnce(CancellationFlag) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let mut runs = self.inner.lock().await;
        if runs.contains_key(&run_id) {
            return Err(RunSpawnError::AlreadyRunning);
        }

        let manager = self.clone();
        let cancel = CancellationFlag::default();
        let task_cancel = cancel.clone();
        let task_run_id = run_id.clone();

        let join = tokio::spawn(async move {
            task(task_cancel).await;
            manager.inner.lock().await.remove(&task_run_id);
        });

        runs.insert(run_id, RunHandle { cancel, join });
        Ok(())
    }

    pub async fn wait_for_idle(&self, run_id: &str) {
        loop {
            let join = self.inner.lock().await.remove(run_id).map(|handle| handle.join);
            if let Some(join) = join {
                let _ = join.await;
                return;
            }
            if !self.is_active(run_id).await {
                return;
            }
        }
    }
}
```

- [x] **Step 6: Run runtime tests and fix spawn race if needed**

Run:

```powershell
cargo test -p runtime
```

Expected: PASS. If `manager_rejects_duplicate_active_run_id` exposes a spawn-before-register race, move insertion before `tokio::spawn` while keeping the same public API and rerun until PASS.

- [x] **Step 7: Verify and commit**

Run:

```powershell
cargo fmt --all -- --check
cargo test -p runtime
cargo check --workspace --locked
```

Commit:

```powershell
git add Cargo.toml Cargo.lock crates/runtime docs/superpowers/plans/2026-06-02-trader-runtime-manager-plan.md
git commit -m "feat: add runtime manager"
```

---

### Task 2: Paper Runtime Cancellation

**Files:**
- Modify: `crates/paper/Cargo.toml`
- Modify: `crates/paper/src/paper.rs`
- Create: `crates/paper/tests/paper_cancellation_tests.rs`
- Modify: `crates/config/src/config.rs`
- Modify: `crates/config/tests/config_tests.rs`

- [x] **Step 1: Add failing config test for optional paper delay**

In `crates/config/tests/config_tests.rs`, add:

```rust
#[test]
fn parses_optional_paper_bar_delay() {
    let config = AppConfig::from_toml_str(
        r#"
        [runtime]
        mode = "paper"
        run_id = "slow-paper"

        [database]
        url = "sqlite://data/trader.sqlite"

        [data]
        source = "csv"
        path = "datasets/sample/aapl_1d.csv"

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

        [paper]
        account_id = "paper"
        slippage_bps = "25"
        fee_bps = "10"
        bar_delay_ms = 50
        "#,
    )
    .unwrap();

    assert_eq!(config.paper.bar_delay_ms, Some(50));
}
```

- [x] **Step 2: Run config test and verify RED**

Run:

```powershell
cargo test -p config parses_optional_paper_bar_delay
```

Expected: FAIL because `PaperConfig.bar_delay_ms` is missing.

- [x] **Step 3: Add optional config field**

In `crates/config/src/config.rs`, update `PaperConfig`:

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct PaperConfig {
    pub account_id: String,
    pub slippage_bps: String,
    pub fee_bps: String,
    pub bar_delay_ms: Option<u64>,
}
```

- [x] **Step 4: Add failing paper cancellation test**

Create `crates/paper/tests/paper_cancellation_tests.rs`:

```rust
use data::Bar;
use paper::{PaperRuntime, PaperRunError, PaperSettings};
use rust_decimal::Decimal;
use runtime::CancellationFlag;
use storage::Db;

#[tokio::test]
async fn paper_runtime_stops_when_cancelled_before_next_bar() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let mut settings = PaperSettings::sample();
    settings.run_id = "cancelled-paper".to_string();
    settings.bar_delay_ms = 1;
    let cancel = CancellationFlag::default();
    cancel.cancel();

    let result = PaperRuntime::new(db.clone(), settings)
        .run_bars_with_cancel(bars(), cancel)
        .await;

    assert_eq!(result.unwrap_err(), PaperRunError::Cancelled);
    assert!(db.get_strategy_run("cancelled-paper").await.unwrap().is_none());
}

fn bars() -> Vec<Bar> {
    vec![
        Bar {
            ts_ms: 1,
            open: Decimal::from(100),
            high: Decimal::from(100),
            low: Decimal::from(100),
            close: Decimal::from(100),
            volume: Decimal::from(10),
        },
        Bar {
            ts_ms: 2,
            open: Decimal::from(101),
            high: Decimal::from(101),
            low: Decimal::from(101),
            close: Decimal::from(101),
            volume: Decimal::from(10),
        },
    ]
}
```

- [x] **Step 5: Run paper cancellation test and verify RED**

Run:

```powershell
cargo test -p paper paper_runtime_stops_when_cancelled_before_next_bar
```

Expected: FAIL because `runtime` dependency, `PaperRunError`, `PaperSettings.bar_delay_ms`, and `run_bars_with_cancel` are missing.

- [x] **Step 6: Add paper dependency and settings field**

Modify `crates/paper/Cargo.toml`:

```toml
runtime.workspace = true
```

In `crates/paper/src/paper.rs`, update `PaperSettings`:

```rust
pub bar_delay_ms: u64,
```

Update `PaperSettings::sample()`:

```rust
bar_delay_ms: 0,
```

- [x] **Step 7: Implement cancellation-aware paper run**

In `crates/paper/src/paper.rs`, add:

```rust
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PaperRunError {
    #[error("paper run cancelled")]
    Cancelled,
}
```

Then add:

```rust
pub async fn run_bars_with_cancel(
    &self,
    bars: Vec<Bar>,
    cancel: runtime::CancellationFlag,
) -> anyhow::Result<BacktestSummary> {
    let mut strategy = MovingAverageCrossStrategy::new(
        self.settings.strategy_name.clone(),
        self.settings.symbol.clone(),
        self.settings.fast_window,
        self.settings.slow_window,
    );
    let broker_settings = SimulatedBrokerSettings {
        slippage_bps: self.settings.slippage_bps,
        fee_bps: self.settings.fee_bps,
    };
    let mut account_book =
        AccountBook::new(self.settings.account_id.clone(), self.settings.initial_cash);
    let mut signals = 0;
    let mut orders = 0;
    let started_at_ms = bars.first().map_or(0, |bar| bar.ts_ms);
    let mut ended_at_ms = started_at_ms;

    for bar in bars {
        if cancel.is_cancelled() {
            return Err(PaperRunError::Cancelled.into());
        }
        if self.settings.bar_delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(self.settings.bar_delay_ms)).await;
        }
        if cancel.is_cancelled() {
            return Err(PaperRunError::Cancelled.into());
        }

        // Move the existing per-bar body from run_bars here unchanged.
    }

    // Move the existing completed run/account/position persistence here unchanged.
    Ok(BacktestSummary { signals, orders })
}
```

Refactor existing `run_bars` to call:

```rust
pub async fn run_bars(&self, bars: Vec<Bar>) -> anyhow::Result<BacktestSummary> {
    self.run_bars_with_cancel(bars, runtime::CancellationFlag::default())
        .await
}
```

- [x] **Step 8: Update API paper settings construction**

In `crates/api/src/api.rs`, update `paper_settings`:

```rust
bar_delay_ms: app_config.paper.bar_delay_ms.unwrap_or(0),
```

- [x] **Step 9: Verify and commit**

Run:

```powershell
cargo fmt --all -- --check
cargo test -p config parses_optional_paper_bar_delay
cargo test -p paper paper_runtime_stops_when_cancelled_before_next_bar
cargo check --workspace --locked
```

Commit:

```powershell
git add crates/config crates/paper crates/api Cargo.toml Cargo.lock docs/superpowers/plans/2026-06-02-trader-runtime-manager-plan.md
git commit -m "feat: make paper runtime cancellable"
```

---

### Task 3: Async Paper Start API

**Files:**
- Modify: `crates/api/Cargo.toml`
- Modify: `crates/api/src/state.rs`
- Modify: `crates/api/src/api.rs`
- Modify: `crates/api/tests/backtest_api_tests.rs`

- [x] **Step 1: Add failing API test for accepted paper start**

In `crates/api/tests/backtest_api_tests.rs`, update `post_paper_run_returns_created` into:

```rust
#[tokio::test]
async fn post_paper_run_returns_accepted_run_start() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = router_with_state(AppState::new(db, "configs/backtest/ma_cross.toml".into()));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/paper-runs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(
        bytes
            .as_ref()
            .windows("\"status\":\"running\"".len())
            .any(|window| window == b"\"status\":\"running\"")
    );
}
```

- [x] **Step 2: Run API test and verify RED**

Run:

```powershell
cargo test -p api post_paper_run_returns_accepted_run_start
```

Expected: FAIL because current endpoint returns `201 CREATED` with summary.

- [x] **Step 3: Add runtime manager to API state**

Modify `crates/api/Cargo.toml`:

```toml
runtime.workspace = true
```

Modify `crates/api/src/state.rs`:

```rust
use runtime::RuntimeManager;
use storage::Db;

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub config_path: String,
    pub runtime_manager: RuntimeManager,
}

impl AppState {
    pub fn new(db: Db, config_path: String) -> Self {
        Self {
            db,
            config_path,
            runtime_manager: RuntimeManager::default(),
        }
    }
}
```

- [x] **Step 4: Add run start response**

In `crates/api/src/api.rs`, add:

```rust
#[derive(Serialize)]
struct RunStartResponse {
    run_id: String,
    status: String,
}
```

- [x] **Step 5: Convert `run_paper` to background start**

Replace `run_paper` signature:

```rust
async fn run_paper(
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<RunStartResponse>), ApiError>
```

In `run_paper`, parse config/settings, insert `running`, load bars before spawning, then spawn:

```rust
let run_id = settings.run_id.clone();
let db = state.db.clone();
let task_settings = settings.clone();
state
    .runtime_manager
    .spawn(run_id.clone(), move |cancel| async move {
        let result = PaperRuntime::new(db.clone(), task_settings.clone())
            .run_bars_with_cancel(bars, cancel)
            .await;

        if let Err(error) = result {
            let status = if error
                .downcast_ref::<paper::PaperRunError>()
                .is_some_and(|error| error == &paper::PaperRunError::Cancelled)
            {
                "cancelled"
            } else {
                "failed"
            };
            let _ = db
                .update_strategy_run_status(
                    &task_settings.run_id,
                    status,
                    Some(chrono::Utc::now().timestamp_millis()),
                    Some(&error.to_string()),
                )
                .await;
        }
    })
    .await
    .map_err(|error| anyhow::anyhow!("{error:?}"))?;

Ok((
    StatusCode::ACCEPTED,
    Json(RunStartResponse {
        run_id,
        status: "running".to_string(),
    }),
))
```

Keep data-load failure handling before spawn: if `load_bars_from_csv` fails, update persisted status to `failed` and return the original error.

- [x] **Step 6: Update existing paper API tests**

Update tests that call `POST /api/v1/paper-runs`:

```rust
assert_eq!(response.status(), StatusCode::ACCEPTED);
wait_for_status(app.clone(), "sample-ma-cross", "completed").await;
```

Add helper at the bottom of `crates/api/tests/backtest_api_tests.rs`:

```rust
async fn wait_for_status(app: axum::Router, run_id: &str, expected_status: &str) {
    for _ in 0..50 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/v1/runs/{run_id}/status"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        if response.status() == StatusCode::OK {
            let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            if bytes
                .as_ref()
                .windows(expected_status.len())
                .any(|window| window == expected_status.as_bytes())
            {
                return;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("run {run_id} did not reach {expected_status}");
}
```

- [x] **Step 7: Verify and commit**

Run:

```powershell
cargo fmt --all -- --check
cargo test -p api
cargo check --workspace --locked
```

Commit:

```powershell
git add crates/api Cargo.toml Cargo.lock docs/superpowers/plans/2026-06-02-trader-runtime-manager-plan.md
git commit -m "feat: start paper runs asynchronously"
```

---

### Task 4: Active Run Cancellation API

**Files:**
- Create: `configs/backtest/slow-paper.toml`
- Modify: `crates/api/src/api.rs`
- Modify: `crates/api/tests/backtest_api_tests.rs`

- [x] **Step 1: Create slow paper fixture**

Create `configs/backtest/slow-paper.toml`:

```toml
[runtime]
mode = "paper"
run_id = "sample-slow-paper"

[database]
url = "sqlite://data/trader.sqlite"

[data]
source = "csv"
path = "datasets/sample/aapl_1d.csv"

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

[paper]
account_id = "paper"
slippage_bps = "25"
fee_bps = "10"
bar_delay_ms = 50
```

- [x] **Step 2: Add failing API cancellation test**

In `crates/api/tests/backtest_api_tests.rs`, add:

```rust
#[tokio::test]
async fn active_paper_run_can_be_cancelled() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = router_with_state(AppState::new(db, "configs/backtest/slow-paper.toml".into()));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/paper-runs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/runs/sample-slow-paper/cancel")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    wait_for_status(app.clone(), "sample-slow-paper", "cancelled").await;
}
```

- [x] **Step 3: Run API test and verify RED**

Run:

```powershell
cargo test -p api active_paper_run_can_be_cancelled
```

Expected: FAIL if cancel only updates storage and the background job later overwrites status to `completed`, or if runtime does not observe cancellation.

- [x] **Step 4: Update cancel route to signal active run**

In `crates/api/src/api.rs`, update `cancel_run`:

```rust
let active_cancelled = state.runtime_manager.cancel(&run_id).await;
if active_cancelled {
    state
        .db
        .update_strategy_run_status(
            &run_id,
            "cancelled",
            Some(chrono::Utc::now().timestamp_millis()),
            None,
        )
        .await?;
    return get_run_status(State(state), Path(run_id)).await;
}

let Some(run) = state.db.get_strategy_run(&run_id).await? else {
    return Ok(StatusCode::NOT_FOUND.into_response());
};
Ok(Json(RunStatusResponse {
    run_id: run.id,
    status: run.status,
    error: run.error,
})
.into_response())
```

This preserves completed/failed terminal status when no active run exists.

- [x] **Step 5: Prevent background task from overwriting cancelled**

In the background paper task, before writing `failed`, read the current run:

```rust
if let Ok(Some(existing)) = db.get_strategy_run(&task_settings.run_id).await
    && existing.status == "cancelled"
{
    return;
}
```

Cancelled `PaperRunError` should update `cancelled`; non-cancel errors update `failed`.

- [x] **Step 6: Verify and commit**

Run:

```powershell
cargo fmt --all -- --check
cargo test -p api active_paper_run_can_be_cancelled
cargo test -p api
cargo check --workspace --locked
```

Commit:

```powershell
git add crates/api configs/backtest/slow-paper.toml docs/superpowers/plans/2026-06-02-trader-runtime-manager-plan.md
git commit -m "feat: cancel active paper runs"
```

---

### Task 5: Smoke Scripts and Documentation

**Files:**
- Modify: `scripts/rest-smoke.ps1`
- Modify: `README.md`
- Modify: `tech.md`
- Modify: `docs/superpowers/plans/2026-06-02-trader-runtime-manager-plan.md`

- [ ] **Step 1: Update REST smoke for async paper start**

Modify `scripts/rest-smoke.ps1` after `POST /api/v1/paper-runs`:

```powershell
$paper = Invoke-RestMethod -Method Post "$baseUrl/api/v1/paper-runs"
if ($paper.status -ne "running") { throw "expected paper run to start as running" }

$status = $null
for ($i = 0; $i -lt 80; $i++) {
    Start-Sleep -Milliseconds 250
    $status = Invoke-RestMethod "$baseUrl/api/v1/runs/$($paper.run_id)/status"
    if ($status.status -eq "completed") { break }
}
if ($status.status -ne "completed") { throw "expected paper run to complete" }
```

Keep the existing fills, balances, snapshots, and metrics assertions after the polling block.

- [ ] **Step 2: Run server smoke**

Run:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\server-smoke.ps1
```

Expected: PASS with summary containing `orders = 1`, `fills = 1`, and non-empty snapshots.

- [ ] **Step 3: Update README**

In `README.md`, update the REST API section to state:

```markdown
`POST /api/v1/paper-runs` starts a background paper run and returns `{ run_id, status }`.
Poll `GET /api/v1/runs/{run_id}/status` until `completed`, `failed`, or `cancelled`.
Use `POST /api/v1/runs/{run_id}/cancel` to request cancellation of an active run.
```

- [ ] **Step 4: Update tech.md**

In `tech.md`, update Phase 5/6 notes:

```markdown
## Phase 6 Runtime Manager

Phase 6 introduces `crates/runtime` as the in-memory active run registry. API starts paper runs in background tasks, persists `running`, and returns immediately with `{ run_id, status }`. `RuntimeManager` owns cancellation flags for active tasks; `PaperRuntime` checks the flag between bars and after optional pacing delay. Cancellation is now best-effort active cancellation for running paper jobs, not just a database status override.
```

- [ ] **Step 5: Final verification**

Run:

```powershell
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace
cargo run -p trader-cli -- paper-run --config configs/backtest/ma_cross.toml
Get-ChildItem crates -Directory | ForEach-Object { Join-Path $_.FullName 'src\lib.rs' } | Where-Object { Test-Path $_ }
rg "= \{ path =" apps crates -g Cargo.toml
```

Expected:

- fmt/check/test pass.
- CLI output includes `paper completed: signals=1 orders=1`.
- naming check prints no files.
- direct member dependency check prints no matches.

- [ ] **Step 6: Commit**

Commit:

```powershell
git add scripts/rest-smoke.ps1 README.md tech.md docs/superpowers/plans/2026-06-02-trader-runtime-manager-plan.md
git commit -m "docs: document runtime manager workflow"
```

---

## Acceptance Criteria

This phase is complete when:

- `POST /api/v1/paper-runs` returns `202 ACCEPTED` with `{ run_id, status: "running" }`.
- Paper runs execute in a background task and eventually persist `completed` or `failed`.
- `POST /api/v1/runs/{run_id}/cancel` signals an active task and the paper loop observes cancellation.
- Cancelled active paper runs persist `cancelled` and are not overwritten to `completed`.
- Completed runs are not changed by a late cancel request.
- `scripts/server-smoke.ps1` still passes against a real `trader-server`.
- `cargo fmt --all -- --check` passes.
- `cargo check --workspace --locked` passes.
- `cargo test --workspace` passes.
- `trader paper-run --config configs/backtest/ma_cross.toml` still prints `paper completed: signals=1 orders=1`.
- Crate root naming convention remains satisfied: no library crate uses default `src/lib.rs`.
- Member crates do not use direct internal `{ path = ... }` dependencies.

## Self-Review

Spec coverage:

- Runtime manager: Task 1.
- Cancellation-aware runtime loop: Task 2.
- Async paper start API: Task 3.
- Real active cancellation route: Task 4.
- Smoke/docs/final verification: Task 5.

Placeholder scan:

- No `TBD`, `TODO`, or open-ended “add tests” steps remain.
- Each task lists exact files, test commands, expected failures, implementation snippets, verification, and commit message.

Type consistency:

- `RuntimeManager`, `CancellationFlag`, and `RunSpawnError` are defined before `api` and `paper` use them.
- `PaperSettings.bar_delay_ms` is introduced in config and paper before slow API fixture uses it.
- API tests use `StatusCode::ACCEPTED` after Task 3 changes the endpoint contract.
