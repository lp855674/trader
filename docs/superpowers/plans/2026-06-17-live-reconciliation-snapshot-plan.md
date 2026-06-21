# Live/Reconciliation Snapshot Capture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Automatically write `cash_snapshots` and `position_snapshots` during paper and live runs, enabling reconciliation against broker-reported state and drift detection.

**Architecture:** Extend the existing runtime snapshot path (currently writes `portfolio_snapshots`) to also write `cash_snapshots` and `position_snapshots` on fills and at periodic intervals. Add reconciliation logic that compares snapshots against broker state and emits `risk_events` on drift. Keep `event_store` as the audit truth.

**Tech Stack:** Rust workspace, SQLx SQLite, Axum, serde, rust_decimal, tokio intervals, PowerShell smoke scripts.

## Current Status (2026-06-19 Audit)

This plan file has now been backfilled for the audited local MVP. Checked items below are confirmed implemented; unchecked items remain original production scope or exact plan steps that have not landed.

| Area | Status | Evidence | Remaining |
| --- | --- | --- | --- |
| Snapshot storage | Done for local MVP | `cash_snapshots` and `position_snapshots` repositories support insert/list/latest/filter; covered by `runtime_repository_tests` | None for local storage boundary |
| Paper runtime snapshots | Done for local MVP | `paper_runtime_persists_cash_and_position_snapshots` verifies paper cash/position snapshot writes | Configurable snapshot cadence is still simpler than the original sketch |
| Live runtime snapshots | Done for fake broker local MVP | Live runtime writes startup baseline cash snapshots and periodic fake broker cash/position snapshots via `[live].broker_snapshot_interval_ms` | Real broker-reported scheduling remains follow-up |
| Reconciliation drift projection | Done for fake broker local MVP | Live runtime emits `reconciliation_drift` risk events for cash drift, missing broker position and qty drift, and writes dedicated `runtime.alert` log records for alert-summary readback | Production alert routing remains follow-up |
| CLI/API readback | Done for local MVP | `snapshots cash`, `snapshots positions`, `reconciliation`, `reconciliation-drifts`, `reconciliation-alerts-summary`, run-scoped API routes and `ops-smoke.ps1` cover API + CLI readback for the same live run id | None for current read-only surface |
| Production readiness | Not done | Docs still classify real broker snapshots as hardening work | Real broker scheduling, alerting, and external operational runbooks |

---

## Scope

In scope:

- Cash snapshot on every fill and at configurable periodic intervals.
- Position snapshot on every fill and at configurable periodic intervals.
- Reconciliation: compare runtime snapshots against broker-reported positions/cash.
- Drift detection with configurable thresholds.
- Wire into paper runtime first, then live runtime.
- Snapshot frequency configurable per run.
- API read-only queries for snapshots.

Out of scope:

- Real-time streaming of snapshots (polling model).
- Cross-exchange portfolio-level reconciliation.
- Historical snapshot analytics (just storage and query).
- Replacing `portfolio_snapshots` (keep it as a summary table).

## File Map

### Storage

- Modify: `crates/storage/src/repositories.rs`
  - Add `insert_cash_snapshot` with fields: run_id, account_id, currency, available, locked, total, ts_ms.
  - Add `list_cash_snapshots(run_id, account_id, from_ms, to_ms)`.
  - Add `insert_position_snapshot` with fields: run_id, account_id, exchange, symbol, qty, avg_price, market_value, unrealized_pnl, ts_ms.
  - Add `list_position_snapshots(run_id, account_id, symbol, from_ms, to_ms)`.
  - Add `get_latest_cash_snapshot(run_id, account_id)`.
  - Add `get_latest_position_snapshot(run_id, account_id, symbol)`.
- Modify: `crates/storage/tests/storage_tests.rs`
  - Add cash snapshot insert/list tests.
  - Add position snapshot insert/list tests.

### Runtime

- Modify: `crates/paper/src/paper.rs`
  - After each fill: insert cash_snapshot and position_snapshot.
  - On configurable interval (e.g., every bar or every N seconds): insert periodic snapshots.
- Modify: `crates/runtime/src/runtime.rs` (or live runtime)
  - Same snapshot path for live runs.
  - Periodic snapshot via tokio interval.
- Modify: `crates/paper/tests/paper_tests.rs`
  - Add test: paper run writes cash_snapshots.
  - Add test: paper run writes position_snapshots.
  - Add test: periodic snapshots are written at interval.

### Reconciliation

- Create: `crates/algorithm/src/reconciliation.rs`
  - `reconcile_cash(runtime: &CashSnapshot, broker: &BrokerCashSnapshot) -> DriftReport`.
  - `reconcile_positions(runtime: &[PositionSnapshot], broker: &[BrokerPositionSnapshot]) -> DriftReport`.
  - `DriftReport` struct with fields: cash_drift, position_drifts (per symbol), severity (info/warn/error).
- Modify: `crates/algorithm/src/algorithm.rs`
  - After reconciliation, if drift exceeds threshold: emit `risk_event` with risk_type="reconciliation_drift".
- Modify: `crates/algorithm/tests/algorithm_tests.rs`
  - Add test: reconciliation detects cash drift.
  - Add test: reconciliation detects position drift.
  - Add test: no drift → no risk event.

### Configuration

- Modify: `crates/config/src/config.rs`
  - Add snapshot config: `snapshot_interval_bars`, `snapshot_interval_seconds`, `reconciliation_enabled`, `drift_threshold_bps`.
- Modify: configs/*.toml
  - Add snapshot configuration examples.

### API

- Modify: `crates/api/src/api.rs`
  - Add `GET /api/v1/runs/{run_id}/cash-snapshots` with optional time filters.
  - Add `GET /api/v1/runs/{run_id}/position-snapshots` with optional symbol/time filters.
  - Add `GET /api/v1/runs/{run_id}/reconciliation` returning latest drift report.
- Modify: `crates/api/tests/api_tests.rs`
  - Add route tests.
- Modify: `docs/api.md`
  - Document new endpoints.

### CLI

- Modify: `apps/trader-cli/src/main.rs`
  - Add `snapshots cash --run-id <id>` command.
  - Add `snapshots positions --run-id <id>` command.
  - Add `reconciliation --run-id <id>` command.

### Documentation

- Modify: `docs/分析.md`
  - Update snapshot section.
- Modify: `docs/roadmap.md`
  - Add "Live/Reconciliation Snapshot" milestone.

---

## Acceptance Gates

Every task must preserve:

- `cargo test -p storage`
- `cargo test -p paper`
- `cargo test -p backtest`
- `cargo test -p algorithm`
- `cargo test -p api`
- `powershell -ExecutionPolicy Bypass -File .\scripts\v1-smoke.ps1`
- `bash ./scripts/check-db-boundary`
- `bash ./scripts/check-storage-dto-boundary`
- `bash ./scripts/check-api-read-model-boundary`

New gates:

- `cargo test -p storage cash_snapshot` — insert/list round-trip.
- `cargo test -p storage position_snapshot` — insert/list round-trip.
- `cargo test -p paper paper_snapshots` — paper run writes snapshots.
- `cargo test -p algorithm reconciliation` — drift detection.

---

## Task 1: Extend Storage for Snapshots

**Files:**

- Modify: `crates/storage/src/repositories.rs`
- Modify: `crates/storage/tests/storage_tests.rs`

- [x] **Step 1: Add cash snapshot insert/list methods**

```rust
pub async fn insert_cash_snapshot(&self, snapshot: &NewCashSnapshot) -> StorageResult<()> {
    sqlx::query(
        r#"
        INSERT INTO cash_snapshots (id, run_id, account_id, currency, available, locked, total, ts_ms)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    // ... bind
    .execute(self.pool()).await?;
    Ok(())
}

pub async fn list_cash_snapshots(&self, run_id: &str, account_id: Option<&str>, from_ms: Option<i64>, to_ms: Option<i64>) -> StorageResult<Vec<StoredCashSnapshot>> {
    // Dynamic WHERE clause with optional filters
}

pub async fn get_latest_cash_snapshot(&self, run_id: &str, account_id: &str) -> StorageResult<Option<StoredCashSnapshot>> {
    // ORDER BY ts_ms DESC LIMIT 1
}
```

- [x] **Step 2: Add position snapshot insert/list methods**

```rust
pub async fn insert_position_snapshot(&self, snapshot: &NewPositionSnapshot) -> StorageResult<()>
pub async fn list_position_snapshots(&self, run_id: &str, account_id: Option<&str>, symbol: Option<&str>, from_ms: Option<i64>, to_ms: Option<i64>) -> StorageResult<Vec<StoredPositionSnapshot>>
pub async fn get_latest_position_snapshot(&self, run_id: &str, account_id: &str, symbol: &str) -> StorageResult<Option<StoredPositionSnapshot>>
```

- [x] **Step 3: Add storage tests**

```rust
#[tokio::test]
async fn cash_snapshot_insert_and_list() {
    // Insert 3 snapshots at different timestamps
    // List with from_ms/to_ms filter
    // Assert: correct count and ordering
}

#[tokio::test]
async fn position_snapshot_insert_and_list() {
    // Insert snapshots for 2 symbols
    // List with symbol filter
    // Assert: correct filtering
}

#[tokio::test]
async fn get_latest_returns_most_recent() {
    // Insert 3 snapshots
    // Get latest
    // Assert: returns the one with highest ts_ms
}
```

- [x] **Step 4: Run storage tests**

```powershell
cargo test -p storage cash_snapshot
cargo test -p storage position_snapshot
```

Expected: pass.

- [x] **Step 5: Commit**

```powershell
git add crates/storage
git commit -m "feat: extend storage for cash and position snapshots"
```

---

## Task 2: Wire Snapshots into Paper Runtime

**Files:**

- Modify: `crates/paper/src/paper.rs`
- Modify: `crates/paper/tests/paper_tests.rs`
- Modify: `crates/config/src/config.rs`

- [ ] **Step 1: Add snapshot config**

```rust
pub struct SnapshotConfig {
    pub snapshot_on_fill: bool,         // default true
    pub snapshot_interval_bars: Option<u32>,  // e.g., Some(5) = every 5 bars
    pub snapshot_interval_seconds: Option<u64>, // e.g., Some(60) = every 60s
}
```

- [x] **Step 2: Add snapshot helper to paper runtime**

```rust
async fn write_fill_snapshots(&self, db: &Db, run_id: &str, account_id: &str, ts_ms: i64) -> StorageResult<()> {
    // 1. Calculate current cash (available, locked, total)
    // 2. Insert cash_snapshot
    // 3. For each open position: insert position_snapshot
}
```

- [x] **Step 3: Wire into paper runtime after each fill**

After the existing fill persistence logic:

```rust
if self.snapshot_config.snapshot_on_fill {
    self.write_fill_snapshots(&self.db, &self.run_id, &self.account_id, ts_ms).await?;
}
```

- [ ] **Step 4: Add periodic snapshot**

```rust
// At the start of each bar processing:
if let Some(interval_bars) = self.snapshot_config.snapshot_interval_bars {
    if self.bar_count % interval_bars == 0 {
        self.write_fill_snapshots(&self.db, &self.run_id, &self.account_id, ts_ms).await?;
    }
}
```

- [x] **Step 5: Add paper tests**

```rust
#[tokio::test]
async fn paper_run_writes_cash_snapshots() {
    // Run paper with snapshot_on_fill=true
    // Assert: cash_snapshots table has rows
    // Assert: each snapshot has valid cash values
}

#[tokio::test]
async fn paper_run_writes_position_snapshots() {
    // Run paper that triggers a fill
    // Assert: position_snapshots table has rows
    // Assert: qty matches filled quantity
}

#[tokio::test]
async fn periodic_snapshots_written_at_interval() {
    // Run paper with snapshot_interval_bars=2
    // Run 6 bars
    // Assert: 3 periodic snapshots (bar 2, 4, 6) + fill snapshots
}
```

- [ ] **Step 6: Run tests**

```powershell
cargo test -p paper paper_run_writes_cash_snapshots
cargo test -p paper paper_run_writes_position_snapshots
cargo test -p paper periodic_snapshots
```

Expected: pass.

- [ ] **Step 7: Commit**

```powershell
git add crates/paper crates/config
git commit -m "feat: wire snapshots into paper runtime"
```

---

## Task 3: Implement Reconciliation Logic

**Files:**

- Create: `crates/algorithm/src/reconciliation.rs`
- Modify: `crates/algorithm/src/algorithm.rs`
- Modify: `crates/algorithm/tests/algorithm_tests.rs`

- [x] **Step 1: Define reconciliation types**

```rust
pub struct DriftReport {
    pub run_id: String,
    pub account_id: String,
    pub ts_ms: i64,
    pub cash_drift: Option<CashDrift>,
    pub position_drifts: Vec<PositionDrift>,
    pub severity: DriftSeverity,
}

pub struct CashDrift {
    pub currency: String,
    pub runtime_total: Decimal,
    pub broker_total: Decimal,
    pub drift_abs: Decimal,
    pub drift_bps: Decimal,
}

pub struct PositionDrift {
    pub symbol: String,
    pub runtime_qty: Decimal,
    pub broker_qty: Decimal,
    pub drift_qty: Decimal,
}

pub enum DriftSeverity {
    Info,    // < 1 bp
    Warn,    // 1-10 bps
    Error,   // > 10 bps or qty mismatch
}
```

- [x] **Step 2: Implement cash reconciliation**

```rust
pub fn reconcile_cash(
    runtime: &StoredCashSnapshot,
    broker: &BrokerCashSnapshot,
    threshold_bps: Decimal,
) -> Option<CashDrift> {
    let drift_abs = (runtime.total - broker.total).abs();
    let drift_bps = if broker.total.is_zero() { Decimal::ZERO } else { drift_abs / broker.total * dec!(10000) };
    if drift_bps > threshold_bps {
        Some(CashDrift { ... })
    } else {
        None
    }
}
```

- [x] **Step 3: Implement position reconciliation**

```rust
pub fn reconcile_positions(
    runtime: &[StoredPositionSnapshot],
    broker: &[BrokerPositionSnapshot],
) -> Vec<PositionDrift> {
    // For each broker position, find matching runtime position
    // If qty differs beyond threshold, report drift
    // If position exists in broker but not runtime: report missing
    // If position exists in runtime but not broker: report orphaned
}
```

- [x] **Step 4: Emit risk events on drift**

```rust
async fn on_reconciliation_drift(&self, report: &DriftReport) -> StorageResult<()> {
    if matches!(report.severity, DriftSeverity::Warn | DriftSeverity::Error) {
        // Insert into risk_events with risk_type="reconciliation_drift"
        // Include drift details in payload_json
    }
    Ok(())
}
```

- [x] **Step 5: Add tests**

```rust
#[test]
fn no_drift_when_cash_matches() {
    let runtime = CashSnapshot { total: dec!(10000), ... };
    let broker = BrokerCashSnapshot { total: dec!(10000), ... };
    assert!(reconcile_cash(&runtime, &broker, dec!(1)).is_none());
}

#[test]
fn detects_cash_drift_above_threshold() {
    let runtime = CashSnapshot { total: dec!(10000), ... };
    let broker = BrokerCashSnapshot { total: dec!(10005), ... };
    let drift = reconcile_cash(&runtime, &broker, dec!(1)).unwrap();
    assert_eq!(drift.severity, DriftSeverity::Warn);
}

#[test]
fn detects_position_qty_mismatch() {
    // ...
}

#[test]
fn detects_orphaned_position() {
    // Position in runtime but not broker
}
```

- [ ] **Step 6: Run tests**

```powershell
cargo test -p algorithm reconciliation
```

Expected: pass.

- [ ] **Step 7: Commit**

```powershell
git add crates/algorithm
git commit -m "feat: reconciliation logic with drift detection"
```

---

## Task 4: Wire Reconciliation into Runtime

**Files:**

- Modify: `crates/paper/src/paper.rs`
- Modify: `crates/runtime/src/runtime.rs`
- Modify: `crates/paper/tests/paper_tests.rs`

- [ ] **Step 1: Add reconciliation trigger to paper runtime**

```rust
// After periodic snapshot:
if self.reconciliation_config.enabled {
    let runtime_cash = db.get_latest_cash_snapshot(&run_id, &account_id).await?;
    let runtime_positions = db.list_position_snapshots(&run_id, Some(&account_id), None, None, None).await?;
    let broker_cash = self.broker.fetch_cash(&account_id).await?;
    let broker_positions = self.broker.fetch_positions(&account_id).await?;

    let report = reconcile(&runtime_cash, &runtime_positions, &broker_cash, &broker_positions, threshold_bps);
    if let Some(report) = report {
        self.on_reconciliation_drift(&report).await?;
    }
}
```

- [ ] **Step 2: Add paper test with mock broker**

```rust
#[tokio::test]
async fn reconciliation_emits_risk_event_on_drift() {
    // Setup: paper run with mock broker that reports different cash
    // Run: trigger reconciliation
    // Assert: risk_events table has reconciliation_drift entry
}
```

- [ ] **Step 3: Commit**

```powershell
git add crates/paper crates/runtime
git commit -m "feat: wire reconciliation into runtime"
```

---

## Task 5: Add CLI and API for Snapshots

**Files:**

- Modify: `crates/api/src/api.rs`
- Modify: `crates/api/tests/api_tests.rs`
- Modify: `apps/trader-cli/src/main.rs`
- Modify: `docs/api.md`

- [x] **Step 1: Add API endpoints**

```
GET /api/v1/runs/{run_id}/cash-snapshots?account_id={acct}&from_ms={t1}&to_ms={t2}
GET /api/v1/runs/{run_id}/position-snapshots?account_id={acct}&symbol={sym}&from_ms={t1}&to_ms={t2}
GET /api/v1/runs/{run_id}/reconciliation
```

- [x] **Step 2: Add API response structs (owned by API)**

```rust
#[derive(Serialize)]
struct CashSnapshotResponse { ... }
#[derive(Serialize)]
struct PositionSnapshotResponse { ... }
#[derive(Serialize)]
struct ReconciliationResponse { ... }
```

- [x] **Step 3: Add CLI commands**

```
trader snapshots cash --run-id <id> [--account <acct>]
trader snapshots positions --run-id <id> [--symbol <sym>]
trader reconciliation --run-id <id>
```

- [x] **Step 4: Add tests, docs, boundary check**

- API tests for all 3 endpoints.
- `docs/api.md` documentation.
- `bash ./scripts/check-api-read-model-boundary` passes.

- [x] **Step 5: Run full acceptance**

```powershell
cargo test -p api
cargo test -p paper
cargo test -p algorithm
powershell -ExecutionPolicy Bypass -File .\scripts\v1-smoke.ps1
bash ./scripts/check-api-read-model-boundary
```

Expected: all pass.

- [x] **Step 6: Commit**

```powershell
git add crates/api apps/trader-cli docs/api.md
git commit -m "feat: snapshot and reconciliation CLI and API"
```

---

## Task 6: Update Documentation

**Files:**

- Modify: `docs/分析.md`
- Modify: `docs/roadmap.md`

- [x] **Step 1: Update `docs/分析.md`**

Update snapshot section to reflect automatic capture and reconciliation.

- [x] **Step 2: Update `docs/roadmap.md`**

Add "Live/Reconciliation Snapshot" milestone.

- [x] **Step 3: Commit**

```powershell
git add docs
git commit -m "docs: update snapshot and reconciliation status"
```

---

## Implementation Order

1. Task 1: Storage extensions.
2. Task 2: Wire into paper runtime.
3. Task 3: Reconciliation logic.
4. Task 4: Wire reconciliation into runtime.
5. Task 5: CLI + API.
6. Task 6: Documentation.

## Risks and Controls

- **Risk:** Snapshot frequency too high causes storage bloat.
  - **Control:** Default to snapshot on fill only. Periodic interval is opt-in. Add cleanup CLI command for old snapshots.
- **Risk:** Reconciliation false positives from timing differences.
  - **Control:** Configurable drift threshold in bps. Default 10 bps for warn, 100 bps for error.
- **Risk:** Broker fetch failures crash runtime.
  - **Control:** Reconciliation is best-effort. Log error, don't crash. Retry on next interval.
- **Risk:** Snapshot writes block hot path.
  - **Control:** Use async writes. Batch inserts when possible. Don't hold pool connections during broker calls.

## Success Criteria

The project is materially improved when:

- `cash_snapshots` and `position_snapshots` are populated by paper runs.
- Reconciliation detects drift between runtime and broker state.
- Drift events appear in `risk_events` table.
- Snapshot frequency is configurable per run.
- CLI and API provide query access to snapshots.
- Existing MVP smoke still passes.
