# Trader Paper Runtime Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Execute inline; do not dispatch subagents.

**Goal:** Upgrade the Paper MVP from a backtest wrapper into a local paper runtime with account balances, portfolio snapshots, fill queries, run queries, metrics summary, CLI execution, and REST query coverage.

**Architecture:** Keep Strategy independent from Broker, OMS, Storage, and API. `paper` owns the paper execution loop; `broker` owns simulated broker behavior; `accounting` owns cash/position/equity math; `storage` owns all SQL; `api` and `trader-cli` only orchestrate and query. This phase still uses CSV bars as the local market-data source.

**Tech Stack:** Rust 2024, Tokio, Axum, SQLx SQLite, serde, rust_decimal, clap, chrono.

---

## Current Baseline

- Branch/base: `main` after Phase 2 merge.
- Phase 2 completed:
  - Config file loading.
  - SQLite migration.
  - CSV bar loading.
  - Persistent backtest orders/fills/positions.
  - Thin `paper` wrapper around `BacktestRuntime`.
  - CLI `migrate`, `import-bars`, `backtest`, `check-config`.
  - REST `POST /api/v1/backtests`, `GET /api/v1/orders`, `GET /api/v1/positions`.
- Current limitation:
  - `paper` does not own a runtime loop.
  - No persisted account balance.
  - No portfolio snapshot/equity curve.
  - No fill API route.
  - No run/metrics API route.
  - Simulated execution has no commission/slippage settings.

## Execution Rules

- Use inline execution only; no subagents.
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
- `crates/accounting/src/accounting.rs`
- `crates/accounting/tests/accounting_tests.rs`
- `crates/broker/src/broker.rs`
- `crates/broker/tests/broker_tests.rs`
- `crates/paper/src/paper.rs`
- `crates/paper/tests/paper_tests.rs`
- `apps/trader-cli/src/main.rs`
- `apps/trader-cli/tests/cli_tests.rs`
- `crates/api/src/api.rs`
- `crates/api/tests/backtest_api_tests.rs`
- `README.md`
- `tech.md`

Create:

- `crates/paper/tests/persistent_paper_tests.rs`

---

### Task 1: Storage Tables for Account and Portfolio Snapshots

**Files:**
- Modify: `migrations/0001_init.sql`
- Modify: `crates/storage/src/repositories.rs`
- Modify: `crates/storage/tests/runtime_repository_tests.rs`

- [x] **Step 1: Extend schema**

Add tables:

```sql
CREATE TABLE IF NOT EXISTS account_balances (
    run_id TEXT NOT NULL,
    account_id TEXT NOT NULL,
    asset TEXT NOT NULL,
    total TEXT NOT NULL,
    available TEXT NOT NULL,
    frozen TEXT NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    PRIMARY KEY (run_id, account_id, asset)
);

CREATE TABLE IF NOT EXISTS portfolio_snapshots (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL,
    account_id TEXT NOT NULL,
    ts_ms INTEGER NOT NULL,
    cash TEXT NOT NULL,
    market_value TEXT NOT NULL,
    equity TEXT NOT NULL,
    realized_pnl TEXT NOT NULL,
    unrealized_pnl TEXT NOT NULL
);
```

- [x] **Step 2: Add repository records**

Add:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewAccountBalance {
    pub run_id: String,
    pub account_id: String,
    pub asset: String,
    pub total: String,
    pub available: String,
    pub frozen: String,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewPortfolioSnapshot {
    pub id: String,
    pub run_id: String,
    pub account_id: String,
    pub ts_ms: i64,
    pub cash: String,
    pub market_value: String,
    pub equity: String,
    pub realized_pnl: String,
    pub unrealized_pnl: String,
}
```

Implement:

- `upsert_account_balance`
- `list_account_balances`
- `insert_portfolio_snapshot`
- `list_portfolio_snapshots`

- [x] **Step 3: Extend storage round-trip test**

In `runtime_records_round_trip`, insert one account balance and one portfolio snapshot, then assert both list methods return one row.

- [x] **Step 4: Verify and commit**

Run:

```powershell
cargo fmt --all -- --check
cargo test -p storage
cargo check --workspace --locked
```

Commit:

```powershell
git add migrations/0001_init.sql crates/storage
git commit -m "feat: persist account and portfolio snapshots"
```

---

### Task 2: Accounting Cash and Equity Model

**Files:**
- Modify: `crates/accounting/src/accounting.rs`
- Modify: `crates/accounting/tests/accounting_tests.rs`

- [x] **Step 1: Add failing accounting tests**

Add tests for:

- buying decreases cash and increases position;
- equity equals cash plus market value;
- average price remains decimal precise.

- [x] **Step 2: Implement account book**

Add:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct AccountBook {
    pub account_id: String,
    pub cash: Decimal,
    pub realized_pnl: Decimal,
    positions: PositionBook,
}
```

Methods:

- `new(account_id, initial_cash)`
- `buy(symbol, qty, price, fee)`
- `position(symbol)`
- `market_value(symbol, mark_price)`
- `equity(symbol, mark_price)`
- `cash()`

- [x] **Step 3: Verify and commit**

Run:

```powershell
cargo fmt --all -- --check
cargo test -p accounting
cargo check --workspace --locked
```

Commit:

```powershell
git add crates/accounting
git commit -m "feat: track paper account equity"
```

---

### Task 3: Simulated Broker Fees and Slippage

**Files:**
- Modify: `crates/broker/src/broker.rs`
- Modify: `crates/broker/tests/broker_tests.rs`

- [x] **Step 1: Add simulated broker test**

Add a test asserting a buy market order at mark price `100` with slippage `0.01` produces fill price `101` and fee based on notional.

- [x] **Step 2: Implement simulated execution**

Add:

```rust
#[derive(Debug, Clone)]
pub struct SimulatedBrokerSettings {
    pub slippage_bps: Decimal,
    pub fee_bps: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulatedFill {
    pub price: Decimal,
    pub qty: Decimal,
    pub fee: Decimal,
}
```

Implement `simulate_market_fill(request, mark_price, settings) -> Result<SimulatedFill, BrokerError>`.

- [x] **Step 3: Verify and commit**

Run:

```powershell
cargo fmt --all -- --check
cargo test -p broker
cargo check --workspace --locked
```

Commit:

```powershell
git add crates/broker
git commit -m "feat: simulate paper fills with fees"
```

---

### Task 4: Independent Paper Runtime

**Files:**
- Modify: `crates/paper/src/paper.rs`
- Create: `crates/paper/tests/persistent_paper_tests.rs`
- Modify: `crates/paper/tests/paper_tests.rs`

- [x] **Step 1: Write persistent paper test**

Test:

- create in-memory DB;
- migrate;
- run three sample bars;
- assert summary has one signal/order/fill;
- assert one account balance;
- assert at least one portfolio snapshot.

- [x] **Step 2: Replace thin wrapper with paper runtime**

Keep public API:

```rust
pub struct PaperRuntime { ... }

impl PaperRuntime {
    pub fn new(db: Db, settings: BacktestSettings) -> Self;
    pub async fn run_bars(&self, bars: Vec<Bar>) -> anyhow::Result<BacktestSummary>;
}
```

But implementation must:

- run strategy loop inside `paper`;
- use simulated fills from `broker`;
- update `AccountBook`;
- persist run, order, fill, position, account balance, portfolio snapshot.

- [x] **Step 3: Verify and commit**

Run:

```powershell
cargo fmt --all -- --check
cargo test -p paper
cargo check --workspace --locked
```

Commit:

```powershell
git add crates/paper
git commit -m "feat: run persistent paper runtime"
```

---

### Task 5: CLI Paper Command

**Files:**
- Modify: `apps/trader-cli/src/main.rs`
- Modify: `apps/trader-cli/tests/cli_tests.rs`

- [x] **Step 1: Add CLI test**

Add:

```rust
#[test]
fn paper_run_accepts_config_argument() {
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .current_dir(workspace_root())
        .args(["paper-run", "--config", "configs/backtest/ma_cross.toml"])
        .assert()
        .success()
        .stdout(contains("paper completed"));
}
```

- [x] **Step 2: Implement command**

Add `Command::PaperRun { config: String }`.

Handler:

- load config;
- migrate DB;
- load CSV bars;
- run `PaperRuntime`;
- print `paper completed: signals=<N> orders=<N>`.

- [x] **Step 3: Verify and commit**

Run:

```powershell
cargo fmt --all -- --check
cargo test -p trader-cli
cargo check --workspace --locked
```

Commit:

```powershell
git add apps/trader-cli
git commit -m "feat: add cli paper run"
```

---

### Task 6: REST Query Coverage

**Files:**
- Modify: `crates/api/src/api.rs`
- Modify: `crates/api/tests/backtest_api_tests.rs`

- [ ] **Step 1: Add API tests**

Extend API test to:

- `POST /api/v1/backtests`;
- `GET /api/v1/fills`;
- `GET /api/v1/account-balances`;
- `GET /api/v1/portfolio/snapshots`;
- assert all return `200 OK` with non-empty arrays.

- [ ] **Step 2: Implement routes**

Add:

- `GET /api/v1/fills`
- `GET /api/v1/account-balances`
- `GET /api/v1/portfolio/snapshots`

Each route reads configured `run_id` and queries storage.

- [ ] **Step 3: Verify and commit**

Run:

```powershell
cargo fmt --all -- --check
cargo test -p api
cargo check --workspace --locked
```

Commit:

```powershell
git add crates/api
git commit -m "feat: expose paper account queries"
```

---

### Task 7: Metrics Summary

**Files:**
- Modify: `crates/metrics/src/metrics.rs`
- Modify: `crates/metrics/tests/metrics_tests.rs`
- Modify: `crates/api/src/api.rs`

- [ ] **Step 1: Add metrics tests**

Test `total_return(initial_equity, final_equity)` and `paper_summary(order_count, fill_count, initial_equity, final_equity)`.

- [ ] **Step 2: Implement summary type**

Add:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MetricsSummary {
    pub total_return: String,
    pub order_count: usize,
    pub fill_count: usize,
}
```

- [ ] **Step 3: Add API route**

Add `GET /api/v1/metrics`, deriving:

- order count from `list_orders`;
- fill count from `list_fills`;
- total return from first/last portfolio snapshot.

- [ ] **Step 4: Verify and commit**

Run:

```powershell
cargo fmt --all -- --check
cargo test -p metrics
cargo test -p api
cargo check --workspace --locked
```

Commit:

```powershell
git add crates/metrics crates/api
git commit -m "feat: expose paper metrics summary"
```

---

### Task 8: Final Verification and Documentation

**Files:**
- Modify: `README.md`
- Modify: `tech.md`
- Modify: `docs/superpowers/plans/2026-06-02-trader-paper-runtime-plan.md`

- [ ] **Step 1: Full verification**

Run:

```powershell
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace
cargo run -p trader-cli -- paper-run --config configs/backtest/ma_cross.toml
```

Expected:

```text
paper completed: signals=1 orders=1
```

- [ ] **Step 2: REST smoke**

Start server with in-memory DB:

```powershell
$env:TRADER_DATABASE_URL = "sqlite::memory:"
cargo run -p trader-server
```

In another shell:

```powershell
Invoke-RestMethod -Method Post http://127.0.0.1:8080/api/v1/backtests
Invoke-RestMethod http://127.0.0.1:8080/api/v1/fills
Invoke-RestMethod http://127.0.0.1:8080/api/v1/account-balances
Invoke-RestMethod http://127.0.0.1:8080/api/v1/portfolio/snapshots
Invoke-RestMethod http://127.0.0.1:8080/api/v1/metrics
```

Expected: all query routes return non-empty results after POST.

- [ ] **Step 3: Update docs**

README: add `paper-run` and new REST routes.

tech.md: update Phase 2 status to say paper runtime is independent from backtest runtime and persists account/equity state.

- [ ] **Step 4: Mark plan complete and commit**

Commit:

```powershell
git add README.md tech.md docs/superpowers/plans/2026-06-02-trader-paper-runtime-plan.md
git commit -m "docs: document paper runtime phase"
```

---

## Acceptance Criteria

This phase is complete when:

- `cargo fmt --all -- --check` passes.
- `cargo check --workspace --locked` passes.
- `cargo test --workspace` passes.
- `trader paper-run --config configs/backtest/ma_cross.toml` prints `paper completed: signals=1 orders=1`.
- Paper runtime persists run, order, fill, position, account balance, and portfolio snapshot.
- REST exposes fills, account balances, portfolio snapshots, and metrics.
- `rules.md` crate root naming convention remains satisfied: no library crate uses default `src/lib.rs`.

## Self-Review

Spec coverage:

- Paper independent runtime: Task 4.
- Account/equity persistence: Tasks 1, 2, 4.
- Simulated paper fills: Task 3.
- CLI paper workflow: Task 5.
- REST query coverage: Tasks 6 and 7.
- Documentation and verification: Task 8.

Placeholder scan:

- No `TBD`, `TODO`, or open-ended implementation steps.
- Each task lists files, commands, expected behavior, and commit message.

Type consistency:

- `NewAccountBalance` and `NewPortfolioSnapshot` are introduced before API uses them.
- `AccountBook` is introduced before `PaperRuntime` uses it.
- `PaperRuntime::run_bars` preserves the current public API used by tests and CLI.
