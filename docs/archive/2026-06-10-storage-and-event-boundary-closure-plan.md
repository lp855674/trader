# Storage and Event Boundary Closure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Finish the remaining `problem.md` boundary work by moving runtime-facing persistence conversions into `storage` and making algorithm event payloads more typed.

**Architecture:** Runtime crates should call semantic storage methods such as `record_paper_order_submitted` and `record_backtest_filled_execution` instead of constructing SQLite-shaped records. The `storage` crate remains the persistence boundary: it owns decimal-to-string conversion, event IDs, record shape, and JSON payload serialization. Typed algorithm payload structs are introduced in `algorithm` and are serialized only at runtime/storage boundaries.

**Tech Stack:** Rust workspace, Tokio, sqlx inside `storage`, SQLite, serde/serde_json, PowerShell boundary checks, Cargo package tests and smoke scripts.

---

### Task 1: Add a Runtime DTO Boundary Check

**Files:**
- Create: `scripts/check/check-storage-dto-boundary.ps1`
- Test: `scripts/check/check-storage-dto-boundary.ps1`

- [x] **Step 1: Write the failing boundary check**

Create `scripts/check/check-storage-dto-boundary.ps1`:

```powershell
$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repoRoot

$patterns = @(
    "NewOrder",
    "NewFill",
    "NewPortfolioSnapshot",
    "NewEventRecord",
    "StoredRuntimeEvent",
    "BacktestExecutionRecord",
    "BacktestPositionRecord"
)

$pattern = ($patterns -join "|")
$matches = rg -n $pattern crates --glob "*.rs" --glob "!crates/storage/**"

if ($LASTEXITCODE -eq 0) {
    Write-Host "Storage DTO boundary violation found outside storage:"
    Write-Host $matches
    exit 1
}

if ($LASTEXITCODE -gt 1) {
    exit $LASTEXITCODE
}

Write-Host "Storage DTO boundary check passed."
```

- [x] **Step 2: Run the boundary check to verify it fails**

Run: `powershell -ExecutionPolicy Bypass -File .\scripts\check\check-storage-dto-boundary.ps1`

Expected: FAIL, listing `crates/backtest/src/backtest.rs` and `crates/paper/src/paper.rs`.

### Task 2: Move Paper Order and Event Persistence Commands into Storage

**Files:**
- Modify: `crates/storage/src/repositories.rs`
- Modify: `crates/paper/src/paper.rs`
- Test: `crates/paper/tests/paper_tests.rs`

- [x] **Step 1: Add semantic paper persistence command structs**

In `crates/storage/src/repositories.rs`, add public command structs named `PaperOrderCommand`, `PaperExecutionCommand`, `PaperFailedOrderCommand`, `PaperPortfolioSnapshotCommand`, and `RuntimeEventCommand`. These structs use domain-shaped fields such as `Decimal`, `Option<String>`, `status`, and `serde_json::Value`, not database string fields.

- [x] **Step 2: Add storage methods that convert commands into records**

Add methods on `Db`:

```rust
pub async fn record_paper_order_submitted(&self, command: PaperOrderCommand) -> StorageResult<()>;
pub async fn record_paper_order_failed(&self, command: PaperFailedOrderCommand) -> StorageResult<()>;
pub async fn record_paper_execution_result(&self, command: PaperExecutionCommand) -> StorageResult<()>;
pub async fn record_paper_portfolio_snapshot(&self, command: PaperPortfolioSnapshotCommand) -> StorageResult<()>;
pub async fn record_runtime_event(&self, command: RuntimeEventCommand) -> StorageResult<()>;
```

Each method internally calls existing `insert_order`, `insert_fill`, `insert_portfolio_snapshot`, or `insert_event`.

- [x] **Step 3: Update Paper runtime to use semantic commands**

Remove `NewOrder`, `NewFill`, `NewPortfolioSnapshot`, and `NewEventRecord` imports from `crates/paper/src/paper.rs`. Replace helper methods with calls to the new `Db::record_*` methods.

- [x] **Step 4: Run paper tests**

Run: `cargo test -p paper`

Expected: PASS.

### Task 3: Move Paper Final Account and Position Persistence into Storage

**Files:**
- Modify: `crates/storage/src/repositories.rs`
- Modify: `crates/paper/src/paper.rs`
- Test: `crates/paper/tests/persistent_paper_tests.rs`

- [x] **Step 1: Add final paper state command**

Add `PaperFinalStateCommand` with run id, strategy name, account id, symbol, base currency, started/ended timestamps, config JSON, and final snapshot decimal fields.

- [x] **Step 2: Add `Db::complete_paper_run`**

Implement `complete_paper_run(command)` in storage. It writes strategy run, account balance, position, and final portfolio snapshot using existing storage record types internally.

- [x] **Step 3: Update `PaperRunSession::finish`**

Remove `NewStrategyRun`, `NewAccountBalance`, and `NewPosition` imports from paper. Call `db.complete_paper_run(...)`.

- [x] **Step 4: Run paper tests and boundary check**

Run: `cargo test -p paper`

Run: `powershell -ExecutionPolicy Bypass -File .\scripts\check\check-storage-dto-boundary.ps1`

Expected: Paper no longer appears in the boundary check output.

### Task 4: Move Backtest Persistence Commands into Storage

**Files:**
- Modify: `crates/storage/src/repositories.rs`
- Modify: `crates/backtest/src/backtest.rs`
- Test: `crates/backtest/tests/*`

- [x] **Step 1: Add semantic backtest commands**

Add `BacktestFilledExecutionCommand`, `BacktestPositionCommand`, and reuse `RuntimeEventCommand`.

- [x] **Step 2: Add storage methods**

Add:

```rust
pub async fn record_backtest_filled_execution(&self, command: BacktestFilledExecutionCommand) -> StorageResult<()>;
pub async fn record_backtest_position(&self, command: BacktestPositionCommand) -> StorageResult<()>;
```

The methods convert Decimal fields to database strings and call the existing internal record methods.

- [x] **Step 3: Update Backtest runtime**

Remove `BacktestExecutionRecord`, `BacktestPositionRecord`, and `StoredRuntimeEvent` imports from `crates/backtest/src/backtest.rs`. Use `RuntimeEventCommand`, `BacktestFilledExecutionCommand`, and `BacktestPositionCommand`.

- [x] **Step 4: Run backtest tests and boundary check**

### Task 4.5: Remove API and Runtime Storage DTO Usage

**Files:**
- Modify: `crates/storage/src/repositories.rs`
- Modify: `crates/runtime/src/live.rs`
- Modify: `crates/api/src/api.rs`
- Modify: `crates/api/src/ws.rs`
- Modify: `crates/api/tests/ws_tests.rs`

- [x] **Step 1: Add semantic live run command**

- [x] **Step 2: Replace API/runtime event writes with `RuntimeEventCommand`**

- [x] **Step 3: Replace REST storage DTO response types with API response structs**

- [x] **Step 4: Run API/runtime tests and storage DTO boundary check**

Run: `cargo test -p backtest`

Run: `powershell -ExecutionPolicy Bypass -File .\scripts\check\check-storage-dto-boundary.ps1`

Expected: PASS with `Storage DTO boundary check passed.`

### Task 5: Type Algorithm Event Payloads

**Files:**
- Modify: `crates/algorithm/src/algorithm.rs`
- Test: `crates/algorithm/tests/algorithm_tests.rs`
- Test: `crates/events/tests/event_tests.rs`

- [x] **Step 1: Add typed payload structs**

Add typed payload structs for the current algorithm event families:

```rust
pub struct AlgorithmOrderPayload { ... }
pub struct AccountingUpdatedPayload { ... }
pub struct UniverseSelectedPayload { ... }
pub struct AlphaGeneratedPayload { ... }
```

They must derive `Serialize`, `Deserialize`, `Debug`, `Clone`, and `PartialEq`.

- [x] **Step 2: Build events from typed payloads**

Replace direct `serde_json::json!` calls in algorithm event construction with typed payload values serialized through `serde_json::to_value`.

- [x] **Step 3: Add schema tests**

Add tests proving key payloads serialize stable fields: `run_id`, `symbol`, `status`, `filled_qty`, `broker_order_id`, and accounting cash fields.

- [x] **Step 4: Run event and algorithm tests**

Run: `cargo test -p events -p algorithm`

Expected: PASS.

### Task 6: Final Verification

**Files:**
- Modify: `docs/superpowers/plans/2026-06-10-storage-and-event-boundary-closure-plan.md`

- [x] **Step 1: Run focused package checks**

Run: `cargo test -p storage -p backtest -p paper -p algorithm -p events`

- [x] **Step 2: Run boundary checks**

Run: `powershell -ExecutionPolicy Bypass -File .\scripts\check\check-db-boundary.ps1`

Run: `powershell -ExecutionPolicy Bypass -File .\scripts\check\check-storage-dto-boundary.ps1`

- [x] **Step 3: Run smoke scripts**

Run: `powershell -ExecutionPolicy Bypass -File .\scripts\smoke\mvp-smoke.ps1`

Run: `powershell -ExecutionPolicy Bypass -File .\scripts\smoke\paper-smoke.ps1`

- [x] **Step 4: Run diff check**

Run: `git diff --check`

Expected: exit 0. CRLF warnings are acceptable on this Windows workspace; whitespace errors are not.
