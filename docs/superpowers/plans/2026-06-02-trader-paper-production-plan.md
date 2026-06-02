# Trader Paper Production Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Execute inline; do not dispatch subagents.

**Goal:** Turn the current paper runtime into a clearer production-ready local paper workflow with config-driven cash, slippage, fees, sell-side accounting, explicit paper APIs, run queries, and repeatable smoke checks.

**Architecture:** Keep Strategy independent from Broker, OMS, Storage, and API. `config` owns user-provided paper settings; `paper` owns `PaperSettings` and the runtime loop; `accounting` owns buy/sell cash, position, realized PnL, and unrealized PnL math; `api` exposes explicit paper run commands and query routes; `storage` remains the only crate with SQL.

**Tech Stack:** Rust 2024, Tokio, Axum, SQLx SQLite, serde, rust_decimal, clap, PowerShell smoke script.

---

## Current Baseline

- `main` includes Phase 3:
  - independent `PaperRuntime`;
  - persisted run, order, fill, position, account balance, portfolio snapshot;
  - `trader paper-run --config configs/backtest/ma_cross.toml`;
  - REST query routes for fills, account balances, portfolio snapshots, metrics.
- Current limitations:
  - `PaperRuntime` uses `BacktestSettings` as a carrier type.
  - initial cash is hard-coded to `10000` inside `paper`.
  - slippage and fee are hard-coded to zero inside `paper`.
  - `AccountBook::buy` is used for both buy and sell by passing negative qty; realized PnL is not modeled.
  - `POST /api/v1/backtests` triggers paper behavior, which is misleading.
  - there is no run query route.
  - REST smoke is documented but not captured as a script.

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

- `configs/backtest/ma_cross.toml`
- `crates/config/src/config.rs`
- `crates/config/tests/config_tests.rs`
- `crates/config/tests/file_config_tests.rs`
- `crates/accounting/src/accounting.rs`
- `crates/accounting/tests/accounting_tests.rs`
- `crates/paper/src/paper.rs`
- `crates/paper/tests/paper_tests.rs`
- `crates/paper/tests/persistent_paper_tests.rs`
- `apps/trader-cli/src/main.rs`
- `apps/trader-cli/tests/cli_tests.rs`
- `crates/storage/src/repositories.rs`
- `crates/storage/tests/runtime_repository_tests.rs`
- `crates/api/src/api.rs`
- `crates/api/tests/backtest_api_tests.rs`
- `README.md`
- `tech.md`
- `docs/superpowers/plans/2026-06-02-trader-paper-production-plan.md`

Create:

- `scripts/rest-smoke.ps1`

---

### Task 1: Config-Driven Paper Settings

**Files:**
- Modify: `configs/backtest/ma_cross.toml`
- Modify: `crates/config/src/config.rs`
- Modify: `crates/config/tests/config_tests.rs`
- Modify: `crates/config/tests/file_config_tests.rs`

- [x] **Step 1: Add failing config test**

In `crates/config/tests/config_tests.rs`, extend the TOML input used by `parses_backtest_config` with:

```toml
[paper]
account_id = "paper"
slippage_bps = "25"
fee_bps = "10"
```

Add assertions:

```rust
assert_eq!(config.paper.account_id, "paper");
assert_eq!(config.paper.slippage_bps, "25");
assert_eq!(config.paper.fee_bps, "10");
```

- [x] **Step 2: Run test and verify RED**

Run:

```powershell
cargo test -p config
```

Expected: FAIL because `AppConfig` has no `paper` field.

- [x] **Step 3: Implement config model**

In `crates/config/src/config.rs`, add `paper` to `AppConfig`:

```rust
pub struct AppConfig {
    pub runtime: RuntimeConfig,
    pub database: DatabaseConfig,
    pub data: DataConfig,
    pub strategy: StrategyConfig,
    pub portfolio: PortfolioConfig,
    pub paper: PaperConfig,
}
```

Add:

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct PaperConfig {
    pub account_id: String,
    pub slippage_bps: String,
    pub fee_bps: String,
}
```

- [x] **Step 4: Update sample config**

In `configs/backtest/ma_cross.toml`, add:

```toml
[paper]
account_id = "paper"
slippage_bps = "25"
fee_bps = "10"
```

- [x] **Step 5: Verify and commit**

Run:

```powershell
cargo fmt --all -- --check
cargo test -p config
cargo check --workspace --locked
```

Commit:

```powershell
git add configs/backtest/ma_cross.toml crates/config
git commit -m "feat: configure paper execution settings"
```

---

### Task 2: Accounting Sell and PnL

**Files:**
- Modify: `crates/accounting/src/accounting.rs`
- Modify: `crates/accounting/tests/accounting_tests.rs`

- [x] **Step 1: Add failing sell tests**

Add tests:

```rust
#[test]
fn account_sell_decreases_position_and_increases_cash() {
    let mut book = AccountBook::new("paper", dec!(10000));
    book.buy("AAPL", dec!(2), dec!(100), dec!(1));

    book.sell("AAPL", dec!(1), dec!(110), dec!(0.5)).unwrap();
    let position = book.position("AAPL").unwrap();

    assert_eq!(book.cash(), dec!(9908.5));
    assert_eq!(position.qty, dec!(1));
    assert_eq!(position.avg_price, dec!(100));
    assert_eq!(book.realized_pnl(), dec!(9.5));
}

#[test]
fn account_unrealized_pnl_uses_mark_price() {
    let mut book = AccountBook::new("paper", dec!(10000));
    book.buy("AAPL", dec!(2), dec!(100), dec!(1));

    assert_eq!(book.unrealized_pnl("AAPL", dec!(110)), dec!(20));
}
```

- [x] **Step 2: Run test and verify RED**

Run:

```powershell
cargo test -p accounting
```

Expected: FAIL because `sell`, `realized_pnl`, and `unrealized_pnl` are missing.

- [x] **Step 3: Implement sell and PnL**

Add error type:

```rust
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AccountingError {
    #[error("position not found")]
    PositionNotFound,
    #[error("sell quantity exceeds position")]
    InsufficientPosition,
}
```

Add methods to `AccountBook`:

```rust
pub fn sell(
    &mut self,
    symbol: &str,
    qty: Decimal,
    price: Decimal,
    fee: Decimal,
) -> Result<(), AccountingError> {
    let position = self
        .positions
        .position_mut(symbol)
        .ok_or(AccountingError::PositionNotFound)?;
    if qty > position.qty {
        return Err(AccountingError::InsufficientPosition);
    }
    self.cash += qty * price - fee;
    self.realized_pnl += qty * (price - position.avg_price) - fee;
    position.qty -= qty;
    Ok(())
}

pub fn realized_pnl(&self) -> Decimal {
    self.realized_pnl
}

pub fn unrealized_pnl(&self, symbol: &str, mark_price: Decimal) -> Decimal {
    self.position(symbol).map_or(Decimal::ZERO, |position| {
        position.qty * (mark_price - position.avg_price)
    })
}
```

Add `position_mut` to `PositionBook`.

- [x] **Step 4: Verify and commit**

Run:

```powershell
cargo fmt --all -- --check
cargo test -p accounting
cargo check --workspace --locked
```

Commit:

```powershell
git add crates/accounting
git commit -m "feat: account for paper sells and pnl"
```

---

### Task 3: PaperSettings and Config-Driven Runtime

**Files:**
- Modify: `crates/paper/src/paper.rs`
- Modify: `crates/paper/tests/paper_tests.rs`
- Modify: `crates/paper/tests/persistent_paper_tests.rs`
- Modify: `apps/trader-cli/src/main.rs`
- Modify: `crates/api/src/api.rs`

- [x] **Step 1: Add failing paper settings test**

In `crates/paper/tests/persistent_paper_tests.rs`, add a test that constructs settings with initial cash and fee/slippage:

```rust
#[tokio::test]
async fn paper_runtime_uses_initial_cash_and_broker_settings() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let mut settings = PaperSettings::sample();
    settings.initial_cash = dec!(100000);
    settings.slippage_bps = dec!(100);
    settings.fee_bps = dec!(10);
    let bars = vec![
        Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
        Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)),
        Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)),
    ];

    PaperRuntime::new(db.clone(), settings.clone())
        .run_bars(bars)
        .await
        .unwrap();

    let balances = db.list_account_balances(&settings.run_id).await.unwrap();
    assert_eq!(balances[0].total, "99979.7798");
    let fills = db.list_fills(&settings.run_id).await.unwrap();
    assert_eq!(fills[0].price, "20.20");
    assert_eq!(fills[0].fee, "0.0202");
}
```

- [x] **Step 2: Run test and verify RED**

Run:

```powershell
cargo test -p paper
```

Expected: FAIL because `PaperSettings` is missing and runtime still hard-codes cash/fees/slippage.

- [x] **Step 3: Implement PaperSettings**

In `crates/paper/src/paper.rs`, add:

```rust
#[derive(Debug, Clone)]
pub struct PaperSettings {
    pub run_id: String,
    pub strategy_name: String,
    pub symbol: String,
    pub account_id: String,
    pub order_qty: Decimal,
    pub max_abs_qty: Decimal,
    pub initial_cash: Decimal,
    pub base_currency: String,
    pub slippage_bps: Decimal,
    pub fee_bps: Decimal,
    pub fast_window: usize,
    pub slow_window: usize,
}
```

Implement `PaperSettings::sample()` using the current sample values plus:

```rust
initial_cash: Decimal::from(100_000),
base_currency: "USD".to_string(),
slippage_bps: Decimal::ZERO,
fee_bps: Decimal::ZERO,
fast_window: 2,
slow_window: 3,
```

Change `PaperRuntime` to store `PaperSettings`, not `BacktestSettings`, and use:

- `settings.initial_cash` for `AccountBook::new`;
- `settings.base_currency` for `NewAccountBalance.asset`;
- `settings.slippage_bps` / `settings.fee_bps` for `SimulatedBrokerSettings`;
- `settings.fast_window` / `settings.slow_window` for `MovingAverageCrossStrategy`.

- [x] **Step 4: Update CLI and API constructors**

In `apps/trader-cli/src/main.rs` and `crates/api/src/api.rs`, replace `backtest_settings` usage for paper with a helper that builds `PaperSettings` from `AppConfig`:

```rust
fn paper_settings(app_config: &config::AppConfig) -> Result<PaperSettings> {
    Ok(PaperSettings {
        run_id: app_config.runtime.run_id.clone(),
        strategy_name: app_config.strategy.name.clone(),
        symbol: app_config
            .strategy
            .symbols
            .first()
            .cloned()
            .unwrap_or_else(|| "US:NASDAQ:AAPL:EQUITY".to_string()),
        account_id: app_config.paper.account_id.clone(),
        order_qty: Decimal::from_str(&app_config.portfolio.order_qty)?,
        max_abs_qty: Decimal::from_str(&app_config.portfolio.max_abs_qty)?,
        initial_cash: Decimal::from_str(&app_config.portfolio.initial_cash)?,
        base_currency: app_config.portfolio.base_currency.clone(),
        slippage_bps: Decimal::from_str(&app_config.paper.slippage_bps)?,
        fee_bps: Decimal::from_str(&app_config.paper.fee_bps)?,
        fast_window: app_config.strategy.fast_window,
        slow_window: app_config.strategy.slow_window,
    })
}
```

- [x] **Step 5: Verify and commit**

Run:

```powershell
cargo fmt --all -- --check
cargo test -p paper
cargo test -p trader-cli
cargo test -p api
cargo check --workspace --locked
```

Commit:

```powershell
git add crates/paper apps/trader-cli crates/api
git commit -m "feat: configure paper runtime settings"
```

---

### Task 4: Paper Runtime Sell Path

**Files:**
- Modify: `crates/paper/src/paper.rs`
- Modify: `crates/paper/tests/persistent_paper_tests.rs`

- [x] **Step 1: Add failing buy-then-sell paper test**

Add:

```rust
#[tokio::test]
async fn paper_runtime_persists_realized_and_unrealized_pnl() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let mut settings = PaperSettings::sample();
    settings.initial_cash = dec!(100000);
    let bars = vec![
        Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
        Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)),
        Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)),
        Bar::new(4, dec!(1), dec!(1), dec!(1), dec!(1), dec!(1)),
    ];

    let summary = PaperRuntime::new(db.clone(), settings.clone())
        .run_bars(bars)
        .await
        .unwrap();

    assert_eq!(summary.orders, 2);
    let snapshots = db.list_portfolio_snapshots(&settings.run_id).await.unwrap();
    let last = snapshots.last().unwrap();
    assert_eq!(last.realized_pnl, "-19");
    assert_eq!(last.unrealized_pnl, "0");
}
```

- [x] **Step 2: Run test and verify RED**

Run:

```powershell
cargo test -p paper
```

Expected: FAIL because paper still routes sell through `buy` with negative quantity and persists zero PnL.

- [x] **Step 3: Implement buy/sell branch in PaperRuntime**

Replace the current signed qty update with:

```rust
match order.side {
    OrderSide::Buy => account_book.buy(&order.symbol, fill.qty, fill.price, fill.fee),
    OrderSide::Sell => account_book.sell(&order.symbol, fill.qty, fill.price, fill.fee)?,
}
```

Persist:

```rust
let unrealized_pnl = account_book.unrealized_pnl(&self.settings.symbol, bar.close);
realized_pnl: account_book.realized_pnl().to_string(),
unrealized_pnl: unrealized_pnl.to_string(),
```

- [x] **Step 4: Verify and commit**

Run:

```powershell
cargo fmt --all -- --check
cargo test -p paper
cargo check --workspace --locked
```

Commit:

```powershell
git add crates/paper
git commit -m "feat: persist paper sell pnl"
```

---

### Task 5: Strategy Run Queries

**Files:**
- Modify: `crates/storage/src/repositories.rs`
- Modify: `crates/storage/tests/runtime_repository_tests.rs`
- Modify: `crates/api/src/api.rs`
- Modify: `crates/api/tests/backtest_api_tests.rs`

- [x] **Step 1: Add failing storage run query test**

In `runtime_records_round_trip`, after inserting the strategy run, assert:

```rust
let run = db.get_strategy_run("run-1").await.unwrap().unwrap();
assert_eq!(run.id, "run-1");
assert_eq!(run.status, "completed");
assert_eq!(db.list_strategy_runs().await.unwrap().len(), 1);
```

- [x] **Step 2: Run storage test and verify RED**

Run:

```powershell
cargo test -p storage
```

Expected: FAIL because `get_strategy_run` and `list_strategy_runs` are missing.

- [x] **Step 3: Implement run record and repository methods**

Add:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StrategyRunRecord {
    pub id: String,
    pub name: String,
    pub mode: String,
    pub status: String,
    pub started_at_ms: i64,
    pub ended_at_ms: Option<i64>,
    pub config_json: String,
}
```

Implement:

- `get_strategy_run(&self, run_id: &str) -> Result<Option<StrategyRunRecord>, sqlx::Error>`;
- `list_strategy_runs(&self) -> Result<Vec<StrategyRunRecord>, sqlx::Error>`.

Use `ORDER BY started_at_ms DESC, id` for list.

- [x] **Step 4: Add API test for runs**

After POSTing a paper run in `crates/api/tests/backtest_api_tests.rs`, call:

```rust
GET /api/v1/runs
GET /api/v1/runs/sample-ma-cross
```

Assert both return `200 OK` and non-empty JSON.

- [x] **Step 5: Implement API routes**

Add:

- `GET /api/v1/runs`;
- `GET /api/v1/runs/{run_id}`.

The by-id route returns `404 NOT_FOUND` when the run is missing.

- [x] **Step 6: Verify and commit**

Run:

```powershell
cargo fmt --all -- --check
cargo test -p storage
cargo test -p api
cargo check --workspace --locked
```

Commit:

```powershell
git add crates/storage crates/api
git commit -m "feat: expose strategy run queries"
```

---

### Task 6: Explicit Paper REST Command

**Files:**
- Modify: `crates/api/src/api.rs`
- Modify: `crates/api/tests/backtest_api_tests.rs`
- Modify: `README.md`
- Modify: `tech.md`

- [x] **Step 1: Add failing API test for paper-runs**

In `crates/api/tests/backtest_api_tests.rs`, add:

```rust
#[tokio::test]
async fn post_paper_run_returns_created_and_populates_queries() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = router_with_state(AppState::new(db, "configs/backtest/ma_cross.toml".into()));

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
    assert_eq!(response.status(), StatusCode::CREATED);
}
```

- [x] **Step 2: Run API test and verify RED**

Run:

```powershell
cargo test -p api
```

Expected: FAIL with `404` for `/api/v1/paper-runs`.

- [x] **Step 3: Implement paper-runs route**

Add:

```rust
.route("/api/v1/paper-runs", post(run_paper))
```

Move the current paper-running implementation out of `run_backtest` into `run_paper`.

Keep `/api/v1/backtests` running `BacktestRuntime` and returning backtest-only persisted output.

- [x] **Step 4: Update docs**

In README and `tech.md`, replace the paper trigger guidance:

- use `POST /api/v1/paper-runs` for paper;
- keep `POST /api/v1/backtests` for backtest.

- [x] **Step 5: Verify and commit**

Run:

```powershell
cargo fmt --all -- --check
cargo test -p api
cargo check --workspace --locked
```

Commit:

```powershell
git add crates/api README.md tech.md
git commit -m "feat: add explicit paper run api"
```

---

### Task 7: REST Smoke Script

**Files:**
- Create: `scripts/rest-smoke.ps1`
- Modify: `README.md`
- Modify: `docs/superpowers/plans/2026-06-02-trader-paper-production-plan.md`

- [ ] **Step 1: Create smoke script**

Create `scripts/rest-smoke.ps1`:

```powershell
$ErrorActionPreference = "Stop"

$baseUrl = $env:TRADER_BASE_URL
if (-not $baseUrl) {
    $baseUrl = "http://127.0.0.1:8080"
}

Invoke-RestMethod "$baseUrl/api/v1/health" | Out-Null
$paper = Invoke-RestMethod -Method Post "$baseUrl/api/v1/paper-runs"
$fills = Invoke-RestMethod "$baseUrl/api/v1/fills"
$balances = Invoke-RestMethod "$baseUrl/api/v1/account-balances"
$snapshots = Invoke-RestMethod "$baseUrl/api/v1/portfolio/snapshots"
$metrics = Invoke-RestMethod "$baseUrl/api/v1/metrics"

if (@($fills).Count -lt 1) { throw "expected at least one fill" }
if (@($balances).Count -lt 1) { throw "expected at least one account balance" }
if (@($snapshots).Count -lt 1) { throw "expected at least one portfolio snapshot" }
if ($metrics.fill_count -lt 1) { throw "expected metrics fill_count >= 1" }

[pscustomobject]@{
    signals = $paper.signals
    orders = $paper.orders
    fills = @($fills).Count
    balances = @($balances).Count
    snapshots = @($snapshots).Count
    total_return = $metrics.total_return
}
```

- [ ] **Step 2: Run script when server is available**

Start server manually:

```powershell
$env:TRADER_DATABASE_URL = "sqlite://data/rest-smoke.sqlite"
cargo run -p trader-server
```

In another shell:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\rest-smoke.ps1
```

Expected: prints an object with `fills`, `balances`, and `snapshots` all at least `1`.

- [ ] **Step 3: Document smoke workflow**

Add the script command to README.

- [ ] **Step 4: Verify and commit**

Run:

```powershell
cargo fmt --all -- --check
cargo test --workspace
```

Commit:

```powershell
git add scripts/rest-smoke.ps1 README.md docs/superpowers/plans/2026-06-02-trader-paper-production-plan.md
git commit -m "test: add rest smoke script"
```

---

### Task 8: Final Verification and Documentation

**Files:**
- Modify: `README.md`
- Modify: `tech.md`
- Modify: `docs/superpowers/plans/2026-06-02-trader-paper-production-plan.md`

- [ ] **Step 1: Full verification**

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

- [ ] **Step 2: Naming and dependency checks**

Run:

```powershell
Get-ChildItem crates -Directory | ForEach-Object { Join-Path $_.FullName 'src\lib.rs' } | Where-Object { Test-Path $_ }
rg "= \{ path =" apps crates -g Cargo.toml
```

Expected: both commands produce no matches.

- [ ] **Step 3: Update docs**

README must include:

- CLI `paper-run`;
- server start command;
- `scripts/rest-smoke.ps1`;
- query routes.

`tech.md` must say:

- paper uses `PaperSettings`, not `BacktestSettings`;
- initial cash, base currency, fee, and slippage come from config;
- paper sell path updates realized/unrealized PnL.

- [ ] **Step 4: Mark plan complete and commit**

Commit:

```powershell
git add README.md tech.md docs/superpowers/plans/2026-06-02-trader-paper-production-plan.md
git commit -m "docs: document paper production workflow"
```

---

## Acceptance Criteria

This phase is complete when:

- `cargo fmt --all -- --check` passes.
- `cargo check --workspace --locked` passes.
- `cargo test --workspace` passes.
- `trader paper-run --config configs/backtest/ma_cross.toml` prints `paper completed: signals=1 orders=1`.
- Paper runtime uses config-driven initial cash, base currency, slippage bps, and fee bps.
- Paper runtime has explicit buy and sell accounting paths.
- Portfolio snapshots persist realized and unrealized PnL.
- REST exposes explicit `POST /api/v1/paper-runs`.
- REST exposes run list/detail queries.
- `scripts/rest-smoke.ps1` validates health, paper run, fills, balances, snapshots, and metrics against a running server.
- Crate root naming convention remains satisfied: no library crate uses default `src/lib.rs`.
- Member crates do not use direct internal `{ path = ... }` dependencies.

## Self-Review

Spec coverage:

- Config-driven paper params: Tasks 1 and 3.
- Sell/PnL accounting: Tasks 2 and 4.
- Paper runtime no longer depends on `BacktestSettings`: Task 3.
- Explicit paper API: Task 6.
- Run queries: Task 5.
- REST smoke workflow: Task 7.
- Documentation and final verification: Task 8.

Placeholder scan:

- No `TBD`, `TODO`, or open-ended implementation steps.
- Each task lists files, commands, expected behavior, and commit message.

Type consistency:

- `PaperConfig` is introduced before CLI/API use it.
- `PaperSettings` is introduced before `PaperRuntime`, CLI, and API use it.
- `AccountBook::sell`, `realized_pnl`, and `unrealized_pnl` are introduced before paper persists PnL fields.
