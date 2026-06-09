# Unified Event-Driven Runtime Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor Trader toward a unified event-driven runtime while preserving local smoke and paper verification after each slice.

**Architecture:** The refactor keeps the existing Backtest/Paper/Replay/Broker loops runnable while moving database details behind the persistence boundary, stabilizing typed runtime events, and then wiring runtime control and strategy/broker extensions onto those contracts. Each slice changes one boundary and leaves a verifiable system behind.

**Tech Stack:** Rust workspace, Tokio, Axum, sqlx inside the persistence boundary, SQLite, EventBus, PowerShell verification scripts.

---

### Task 1: Enforce Database Boundary

**Files:**
- Create: `scripts/check-db-boundary.ps1`
- Modify: `crates/storage/src/db.rs`
- Modify: `crates/storage/src/repositories.rs`
- Modify: `crates/storage/src/storage.rs`
- Modify: `crates/api/src/api.rs`
- Modify: `crates/api/Cargo.toml`
- Modify: `crates/runtime/src/live.rs`
- Modify: `crates/runtime/Cargo.toml`
- Test: `scripts/check-db-boundary.ps1`

- [x] **Step 1: Write the failing boundary check**

Create `scripts/check-db-boundary.ps1`:

```powershell
$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repoRoot

$matches = rg -n "sqlx|SqlitePool|Pool<Sqlite>|SqliteConnection|Transaction<|Executor<" `
    crates `
    --glob "*.rs" `
    --glob "Cargo.toml" `
    --glob "!crates/storage/**"

if ($LASTEXITCODE -eq 0) {
    Write-Host "Database boundary violation found outside the persistence boundary:"
    Write-Host $matches
    exit 1
}

if ($LASTEXITCODE -gt 1) {
    exit $LASTEXITCODE
}

Write-Host "Database boundary check passed."
```

- [x] **Step 2: Run the boundary check to verify it fails**

Run: `powershell -ExecutionPolicy Bypass -File .\scripts\check-db-boundary.ps1`

Expected: FAIL, listing `crates/api` and `crates/runtime` references to `sqlx`.

- [x] **Step 3: Add storage-owned error type**

Add `StorageError` and `StorageResult<T>` in `crates/storage/src/db.rs`, and expose them from `crates/storage/src/storage.rs`. Convert public storage methods from `Result<_, sqlx::Error>` to `StorageResult<_>`.

- [x] **Step 4: Remove sqlx from upper crates**

Replace `sqlx::Error` signatures in `api` and `runtime` with `storage::StorageResult` or `ApiError`, remove `impl From<sqlx::Error> for ApiError`, and remove `sqlx.workspace = true` from `crates/api/Cargo.toml` and `crates/runtime/Cargo.toml`.

- [x] **Step 5: Run the boundary check to verify it passes**

Run: `powershell -ExecutionPolicy Bypass -File .\scripts\check-db-boundary.ps1`

Expected: PASS with `Database boundary check passed.`

- [x] **Step 6: Run compile checks**

Run: `cargo check -p storage -p runtime -p api`

Expected: PASS.

### Task 2: Stabilize Runtime Event Contract

**Files:**
- Modify: `crates/events/src/event.rs`
- Modify: `crates/algorithm/src/algorithm.rs`
- Modify: `crates/api/src/ws.rs`
- Test: `crates/api/tests/ws_tests.rs`
- Test: `crates/algorithm/tests/*`

- [x] **Step 1: Add tests for run-id filtering without payload parsing**

Add WebSocket/event tests proving run filtering uses envelope source or typed metadata instead of parsing arbitrary payload JSON.

- [x] **Step 2: Introduce typed runtime event payloads**

Add typed payload structs for algorithm and replay events while preserving JSON serialization for `event_store`.

- [x] **Step 3: Update producers and consumers**

Update AlgorithmEngine, ReplayRuntime, API and WebSocket to use the typed event contract.

- [x] **Step 4: Verify**

Run: `cargo test -p events -p algorithm -p api`.

### Task 3: Split Paper Order Lifecycle

**Files:**
- Modify: `crates/paper/src/paper.rs`
- Modify: `crates/paper/src/binance.rs`
- Modify: `crates/paper/src/ibkr.rs`
- Test: `crates/paper/tests/*`

- [ ] **Step 1: Add tests for submitted, unfilled, partial, filled and failed broker outcomes**

Write tests proving each outcome persists the expected order/fill/accounting/event records.

- [ ] **Step 2: Introduce a unified paper execution result**

Add a typed execution result shared by simulated, Binance and IBKR executors.

- [ ] **Step 3: Extract lifecycle persistence steps**

Split submitted-order persistence, broker execution, execution-result persistence and accounting snapshot updates into explicit methods.

- [ ] **Step 4: Verify**

Run: `cargo test -p paper`.

### Task 4: Close Replay Control Loop

**Files:**
- Modify: `crates/replay/src/replay.rs`
- Modify: `crates/api/src/api.rs`
- Modify: `crates/api/src/ws.rs`
- Test: `crates/replay/tests/*`
- Test: `crates/api/tests/*`

- [x] **Step 1: Add tests proving pause/resume/seek/speed affect a running replay**

Use a short bar stream and assert emitted `market.bar` events stop, resume, seek and speed up.

- [x] **Step 2: Wire ReplayRuntime to shared controller state**

Read controller state before each bar and publish replay state events.

- [ ] **Step 3: Verify**

Run: `cargo test -p replay -p api`.

### Task 5: Add Configurable Universe and Alpha Assembly

**Files:**
- Modify: `crates/config/src/*`
- Modify: `crates/universe/src/*`
- Modify: `crates/alpha/src/*`
- Modify: `crates/strategies/src/*`
- Test: related crate tests

- [ ] **Step 1: Add tests for selecting universe and alpha model from config**

Prove default single-symbol behavior stays intact and config can select a named universe/alpha.

- [ ] **Step 2: Implement config-driven assembly**

Add registry wiring without changing existing sample configs.

- [ ] **Step 3: Verify**

Run: `cargo test -p config -p universe -p alpha -p strategies`.

### Task 6: Broker Client Evolution Slice

**Files:**
- Modify only broker adapter internals selected for that slice
- Test: broker/paper smoke tests

- [ ] **Step 1: Pick one broker client migration at a time**

Choose Binance or IBKR, not both in one patch.

- [ ] **Step 2: Preserve adapter boundary**

Keep existing paper executor and broker adapter domain interfaces stable.

- [ ] **Step 3: Verify**

Run the broker-specific no-network smoke first, then gated real paper checks only when credentials/environment are explicitly present.
