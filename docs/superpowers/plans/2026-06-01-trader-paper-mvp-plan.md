# Trader Paper MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a locally usable Paper Trading MVP that can load a config, migrate SQLite, import sample bars, run a backtest/paper simulation, persist runs/orders/fills/positions/events, and expose results through CLI and REST API.

**Architecture:** Keep strategy independent from storage, broker, OMS, and API. Runtime crates orchestrate the existing V1 components: config -> data -> strategy -> portfolio -> risk -> execution -> OMS -> broker -> accounting -> storage -> API/CLI. SQLite remains the state truth; CSV is the Phase 2 ingest format, while Parquet remains a documented boundary for a later data task.

**Tech Stack:** Rust 2024, Tokio, Axum, SQLx SQLite, serde, serde_json, uuid, chrono, rust_decimal, clap, tower, tracing.

---

## Source Documents

- `tech.md`: current technical baseline.
- `docs/architecture.md`: cross-module boundaries.
- `docs/database.md`: SQLite state model.
- `docs/api.md`: REST / WebSocket contract direction.
- `docs/events.md`: event envelope and event categories.
- `docs/strategy.md`: strategy boundary.
- `docs/broker.md`: broker boundary.
- `docs/superpowers/plans/2026-05-31-trader-v1-implementation.md`: completed V1 skeleton.

## Execution Rules

- Use inline execution on branch `phase2-paper-mvp`; do not dispatch subagents.
- Work in small commits. Commit after each task passes its task tests.
- Run `cargo fmt --all -- --check` before every commit.
- Run `cargo test -p <crate>` for touched crates.
- Run `cargo check --workspace --locked` after each task.
- Do not add live broker credentials or real account data.
- Keep SQL usage inside `crates/storage`.
- Keep strategy independent from Broker, OMS, Storage, and API.

## File Structure

Create or modify these paths:

- Modify: `crates/config/src/lib.rs` for file loading and validation.
- Modify: `apps/trader-cli/src/main.rs` for `--config`, `migrate`, `import-bars`, and `backtest`.
- Modify: `apps/trader-cli/Cargo.toml` for config/storage/data/backtest dependencies.
- Modify: `crates/storage/src/lib.rs`, `crates/storage/src/db.rs`, `crates/storage/src/repositories.rs`.
- Modify: `migrations/0001_init.sql` to add missing Phase 2 query fields.
- Create: `crates/storage/tests/runtime_repository_tests.rs`.
- Modify: `crates/data/src/lib.rs`.
- Create: `crates/data/src/csv.rs`.
- Create: `crates/data/tests/csv_loader_tests.rs`.
- Modify: `crates/backtest/src/lib.rs` to persist run output.
- Create: `crates/backtest/tests/persistent_backtest_tests.rs`.
- Modify: `Cargo.toml` to add `crates/paper`.
- Create: `crates/paper/Cargo.toml`.
- Create: `crates/paper/src/lib.rs`.
- Create: `crates/paper/tests/paper_tests.rs`.
- Modify: `crates/api/src/lib.rs` for query and command routes.
- Create: `crates/api/src/state.rs`.
- Create: `crates/api/tests/backtest_api_tests.rs`.
- Modify: `apps/trader-server/Cargo.toml`.
- Modify: `apps/trader-server/src/main.rs` to load config and state.
- Modify: `configs/backtest/ma_cross.toml` to include database and runtime IDs.
- Modify: `README.md` and `tech.md` with Phase 2 commands.

---

### Task 1: Config File Loading and Validation

**Files:**
- Modify: `crates/config/Cargo.toml`
- Modify: `crates/config/src/lib.rs`
- Create: `crates/config/tests/file_config_tests.rs`
- Modify: `configs/backtest/ma_cross.toml`

- [x] **Step 1: Add path loading dependency**

Update `crates/config/Cargo.toml`:

```toml
[dependencies]
serde.workspace = true
toml.workspace = true
thiserror.workspace = true
```

No new dependency is required for file loading; use `std::fs`.

- [x] **Step 2: Expand example config**

Replace `configs/backtest/ma_cross.toml` with:

```toml
[runtime]
mode = "backtest"
run_id = "sample-ma-cross"

[database]
url = "sqlite:data/trader.sqlite"

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
```

- [x] **Step 3: Write config file loading tests**

Create `crates/config/tests/file_config_tests.rs`:

```rust
use config::{AppConfig, RuntimeMode};

#[test]
fn loads_config_from_file() {
    let config = AppConfig::from_toml_file("../../configs/backtest/ma_cross.toml").unwrap();

    assert_eq!(config.runtime.mode, RuntimeMode::Backtest);
    assert_eq!(config.runtime.run_id, "sample-ma-cross");
    assert_eq!(config.database.url, "sqlite:data/trader.sqlite");
    assert_eq!(config.data.source, "csv");
    assert_eq!(config.portfolio.order_qty, "1");
    assert_eq!(config.portfolio.max_abs_qty, "100");
}
```

- [x] **Step 4: Implement config fields and file loading**

Update `crates/config/src/lib.rs` to include:

```rust
use std::path::Path;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config file {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("failed to parse config: {0}")]
    Parse(#[from] toml::de::Error),
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeConfig {
    pub mode: RuntimeMode,
    pub run_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PortfolioConfig {
    pub initial_cash: String,
    pub base_currency: String,
    pub order_qty: String,
    pub max_abs_qty: String,
}

impl AppConfig {
    pub fn from_toml_file(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path_ref = path.as_ref();
        let input = std::fs::read_to_string(path_ref).map_err(|source| ConfigError::Read {
            path: path_ref.display().to_string(),
            source,
        })?;
        Self::from_toml_str(&input)
    }
}
```

Keep existing `from_toml_str` and add `pub database: DatabaseConfig` to `AppConfig`.

- [x] **Step 5: Run tests and commit**

Run:

```powershell
cargo test -p config
cargo check --workspace --locked
```

Expected: both commands pass.

Commit:

```powershell
git add crates/config configs/backtest/ma_cross.toml
git commit -m "feat: load trader config from file"
```

---

### Task 2: Storage Runtime Repositories

**Files:**
- Modify: `migrations/0001_init.sql`
- Modify: `crates/storage/src/repositories.rs`
- Create: `crates/storage/tests/runtime_repository_tests.rs`

- [x] **Step 1: Extend SQLite schema**

Update `migrations/0001_init.sql` so these tables exist with the listed columns:

```sql
CREATE TABLE IF NOT EXISTS strategy_runs (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    mode TEXT NOT NULL,
    status TEXT NOT NULL,
    started_at_ms INTEGER NOT NULL,
    ended_at_ms INTEGER,
    config_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS orders (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL,
    client_order_id TEXT NOT NULL UNIQUE,
    broker_order_id TEXT,
    account_id TEXT NOT NULL,
    symbol TEXT NOT NULL,
    side TEXT NOT NULL,
    order_type TEXT NOT NULL,
    price TEXT,
    qty TEXT NOT NULL,
    filled_qty TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS fills (
    id TEXT PRIMARY KEY,
    order_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    symbol TEXT NOT NULL,
    side TEXT NOT NULL,
    price TEXT NOT NULL,
    qty TEXT NOT NULL,
    fee TEXT NOT NULL,
    ts_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS positions (
    run_id TEXT NOT NULL,
    account_id TEXT NOT NULL,
    symbol TEXT NOT NULL,
    qty TEXT NOT NULL,
    avg_price TEXT NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    PRIMARY KEY (run_id, account_id, symbol)
);
```

- [x] **Step 2: Write repository round-trip test**

Create `crates/storage/tests/runtime_repository_tests.rs`:

```rust
use storage::{Db, NewFill, NewOrder, NewPosition, NewStrategyRun};

#[tokio::test]
async fn runtime_records_round_trip() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();

    db.insert_strategy_run(NewStrategyRun {
        id: "run-1".to_string(),
        name: "moving_average_cross".to_string(),
        mode: "backtest".to_string(),
        status: "completed".to_string(),
        started_at_ms: 1,
        ended_at_ms: Some(2),
        config_json: "{}".to_string(),
    })
    .await
    .unwrap();

    db.insert_order(NewOrder {
        id: "order-1".to_string(),
        run_id: "run-1".to_string(),
        client_order_id: "client-1".to_string(),
        broker_order_id: Some("broker-1".to_string()),
        account_id: "paper".to_string(),
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        side: "BUY".to_string(),
        order_type: "MARKET".to_string(),
        price: None,
        qty: "1".to_string(),
        filled_qty: "1".to_string(),
        status: "FILLED".to_string(),
        created_at_ms: 1,
        updated_at_ms: 2,
    })
    .await
    .unwrap();

    db.insert_fill(NewFill {
        id: "fill-1".to_string(),
        order_id: "order-1".to_string(),
        run_id: "run-1".to_string(),
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        side: "BUY".to_string(),
        price: "108".to_string(),
        qty: "1".to_string(),
        fee: "0".to_string(),
        ts_ms: 3,
    })
    .await
    .unwrap();

    db.upsert_position(NewPosition {
        run_id: "run-1".to_string(),
        account_id: "paper".to_string(),
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        qty: "1".to_string(),
        avg_price: "108".to_string(),
        updated_at_ms: 3,
    })
    .await
    .unwrap();

    assert_eq!(db.list_orders("run-1").await.unwrap().len(), 1);
    assert_eq!(db.list_fills("run-1").await.unwrap().len(), 1);
    assert_eq!(db.list_positions("run-1").await.unwrap().len(), 1);
}
```

- [x] **Step 3: Implement storage records**

Add these structs to `crates/storage/src/repositories.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewStrategyRun {
    pub id: String,
    pub name: String,
    pub mode: String,
    pub status: String,
    pub started_at_ms: i64,
    pub ended_at_ms: Option<i64>,
    pub config_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewOrder {
    pub id: String,
    pub run_id: String,
    pub client_order_id: String,
    pub broker_order_id: Option<String>,
    pub account_id: String,
    pub symbol: String,
    pub side: String,
    pub order_type: String,
    pub price: Option<String>,
    pub qty: String,
    pub filled_qty: String,
    pub status: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewFill {
    pub id: String,
    pub order_id: String,
    pub run_id: String,
    pub symbol: String,
    pub side: String,
    pub price: String,
    pub qty: String,
    pub fee: String,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPosition {
    pub run_id: String,
    pub account_id: String,
    pub symbol: String,
    pub qty: String,
    pub avg_price: String,
    pub updated_at_ms: i64,
}
```

Implement `insert_strategy_run`, `insert_order`, `insert_fill`, `upsert_position`, `list_orders`, `list_fills`, and `list_positions` on `Db` using `sqlx::query` and `query_as`.

- [x] **Step 4: Run tests and commit**

Run:

```powershell
cargo test -p storage
cargo check --workspace --locked
```

Expected: both commands pass.

Commit:

```powershell
git add crates/storage migrations/0001_init.sql
git commit -m "feat: persist runtime trading records"
```

---

### Task 3: CSV Bar Loader

**Files:**
- Modify: `crates/data/Cargo.toml`
- Modify: `crates/data/src/lib.rs`
- Create: `crates/data/src/csv.rs`
- Create: `crates/data/tests/csv_loader_tests.rs`

- [x] **Step 1: Add CSV dependencies**

Update `crates/data/Cargo.toml`:

```toml
[dependencies]
chrono.workspace = true
rust_decimal.workspace = true
serde.workspace = true
thiserror.workspace = true
csv = "1"
```

- [x] **Step 2: Write CSV loader test**

Create `crates/data/tests/csv_loader_tests.rs`:

```rust
use data::load_bars_from_csv;
use rust_decimal_macros::dec;

#[test]
fn loads_sample_bars_from_csv() {
    let bars = load_bars_from_csv("../../datasets/sample/aapl_1d.csv").unwrap();

    assert_eq!(bars.len(), 2);
    assert_eq!(bars[0].ts_ms, 1704067200000);
    assert_eq!(bars[0].close, dec!(108.00));
}
```

- [x] **Step 3: Implement loader**

Create `crates/data/src/csv.rs`:

```rust
use crate::Bar;
use rust_decimal::Decimal;
use serde::Deserialize;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DataError {
    #[error("failed to read csv: {0}")]
    Csv(#[from] csv::Error),
}

#[derive(Debug, Deserialize)]
struct CsvBar {
    ts_ms: i64,
    open: Decimal,
    high: Decimal,
    low: Decimal,
    close: Decimal,
    volume: Decimal,
}

pub fn load_bars_from_csv(path: impl AsRef<Path>) -> Result<Vec<Bar>, DataError> {
    let mut reader = csv::Reader::from_path(path)?;
    let mut bars = Vec::new();
    for row in reader.deserialize::<CsvBar>() {
        let row = row?;
        bars.push(Bar::new(row.ts_ms, row.open, row.high, row.low, row.close, row.volume));
    }
    Ok(bars)
}
```

Update `crates/data/src/lib.rs`:

```rust
mod bar;
mod csv;

pub use bar::*;
pub use csv::*;
```

- [x] **Step 4: Run tests and commit**

Run:

```powershell
cargo test -p data
cargo check --workspace --locked
```

Expected: both commands pass.

Commit:

```powershell
git add crates/data
git commit -m "feat: load bars from csv"
```

---

### Task 4: Persistent Backtest Runtime

**Files:**
- Modify: `crates/backtest/Cargo.toml`
- Modify: `crates/backtest/src/lib.rs`
- Create: `crates/backtest/tests/persistent_backtest_tests.rs`

- [ ] **Step 1: Add storage and uuid dependencies**

Update `crates/backtest/Cargo.toml`:

```toml
[dependencies]
broker = { path = "../broker" }
data = { path = "../data" }
execution = { path = "../execution" }
portfolio = { path = "../portfolio" }
risk = { path = "../risk" }
storage = { path = "../storage" }
strategies = { path = "../strategies" }
anyhow.workspace = true
chrono.workspace = true
rust_decimal.workspace = true
thiserror.workspace = true
tokio.workspace = true
uuid.workspace = true
```

- [ ] **Step 2: Write persistent backtest test**

Create `crates/backtest/tests/persistent_backtest_tests.rs`:

```rust
use backtest::{BacktestRuntime, BacktestSettings};
use data::Bar;
use rust_decimal_macros::dec;
use storage::Db;

#[tokio::test]
async fn backtest_persists_orders_and_positions() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let bars = vec![
        Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
        Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)),
        Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)),
    ];

    let summary = BacktestRuntime::new(db.clone(), BacktestSettings::sample()).run(bars).await.unwrap();

    assert_eq!(summary.signals, 1);
    assert_eq!(db.list_orders("sample-ma-cross").await.unwrap().len(), 1);
    assert_eq!(db.list_positions("sample-ma-cross").await.unwrap().len(), 1);
}
```

- [ ] **Step 3: Implement settings and persistence**

Add to `crates/backtest/src/lib.rs`:

```rust
#[derive(Debug, Clone)]
pub struct BacktestSettings {
    pub run_id: String,
    pub strategy_name: String,
    pub symbol: String,
    pub account_id: String,
    pub order_qty: Decimal,
    pub max_abs_qty: Decimal,
}

impl BacktestSettings {
    pub fn sample() -> Self {
        Self {
            run_id: "sample-ma-cross".to_string(),
            strategy_name: "moving_average_cross".to_string(),
            symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
            account_id: "backtest".to_string(),
            order_qty: Decimal::ONE,
            max_abs_qty: Decimal::from(100),
        }
    }
}
```

Change `BacktestRuntime` to store `Db` and `BacktestSettings`. In `run`, insert a strategy run before processing bars, persist a filled market order and fill after broker acceptance, update `PositionBook`, then upsert the final position. Use deterministic IDs derived from counters:

```rust
let order_id = format!("{}-order-{}", self.settings.run_id, orders + 1);
let fill_id = format!("{}-fill-{}", self.settings.run_id, orders + 1);
```

- [ ] **Step 4: Run tests and commit**

Run:

```powershell
cargo test -p backtest
cargo check --workspace --locked
```

Expected: both commands pass.

Commit:

```powershell
git add crates/backtest
git commit -m "feat: persist backtest results"
```

---

### Task 5: Paper Runtime Crate

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/paper/Cargo.toml`
- Create: `crates/paper/src/lib.rs`
- Create: `crates/paper/tests/paper_tests.rs`

- [ ] **Step 1: Add paper crate to workspace**

Add `"crates/paper"` to the root `Cargo.toml` workspace members after `"crates/replay"`.

- [ ] **Step 2: Create paper crate manifest**

Create `crates/paper/Cargo.toml`:

```toml
[package]
name = "paper"
version.workspace = true
edition.workspace = true
license.workspace = true
rust-version.workspace = true

[dependencies]
backtest = { path = "../backtest" }
data = { path = "../data" }
storage = { path = "../storage" }
anyhow.workspace = true
```

- [ ] **Step 3: Write paper runtime test**

Create `crates/paper/tests/paper_tests.rs`:

```rust
use backtest::BacktestSettings;
use data::Bar;
use paper::PaperRuntime;
use rust_decimal_macros::dec;
use storage::Db;

#[tokio::test]
async fn paper_runtime_uses_backtest_execution_path() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let bars = vec![
        Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
        Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)),
        Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)),
    ];

    let summary = PaperRuntime::new(db, BacktestSettings::sample()).run_bars(bars).await.unwrap();

    assert_eq!(summary.orders, 1);
}
```

- [ ] **Step 4: Implement paper runtime wrapper**

Create `crates/paper/src/lib.rs`:

```rust
#![forbid(unsafe_code)]

use backtest::{BacktestRuntime, BacktestSettings, BacktestSummary};
use data::Bar;
use storage::Db;

pub struct PaperRuntime {
    inner: BacktestRuntime,
}

impl PaperRuntime {
    pub fn new(db: Db, settings: BacktestSettings) -> Self {
        Self {
            inner: BacktestRuntime::new(db, settings),
        }
    }

    pub async fn run_bars(&self, bars: Vec<Bar>) -> anyhow::Result<BacktestSummary> {
        self.inner.run(bars).await
    }
}
```

- [ ] **Step 5: Run tests and commit**

Run:

```powershell
cargo test -p paper
cargo check --workspace --locked
```

Expected: both commands pass.

Commit:

```powershell
git add Cargo.toml Cargo.lock crates/paper
git commit -m "feat: add paper runtime wrapper"
```

---

### Task 6: CLI Migrate, Import, and Backtest

**Files:**
- Modify: `apps/trader-cli/Cargo.toml`
- Modify: `apps/trader-cli/src/main.rs`
- Modify: `apps/trader-cli/tests/cli_tests.rs`

- [ ] **Step 1: Add CLI dependencies**

Update `apps/trader-cli/Cargo.toml`:

```toml
[dependencies]
anyhow.workspace = true
backtest = { path = "../../crates/backtest" }
clap.workspace = true
config = { path = "../../crates/config" }
data = { path = "../../crates/data" }
rust_decimal.workspace = true
storage = { path = "../../crates/storage" }
tokio.workspace = true
```

- [ ] **Step 2: Write CLI tests**

Extend `apps/trader-cli/tests/cli_tests.rs`:

```rust
#[test]
fn backtest_accepts_config_argument() {
    let mut command = Command::cargo_bin("trader").unwrap();
    command
        .args(["backtest", "--config", "configs/backtest/ma_cross.toml"])
        .assert()
        .success()
        .stdout(contains("backtest completed"));
}
```

- [ ] **Step 3: Implement CLI arguments**

Replace `Command` in `apps/trader-cli/src/main.rs` with:

```rust
#[derive(Subcommand)]
enum Command {
    Init,
    Migrate {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
    },
    ImportBars {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
    },
    Backtest {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
    },
    Replay,
    Report,
    CheckConfig {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
    },
}
```

Implement handlers:

```rust
async fn load_db(config_path: &str) -> Result<(config::AppConfig, storage::Db)> {
    let app_config = config::AppConfig::from_toml_file(config_path)?;
    let db = storage::Db::connect(&app_config.database.url).await?;
    Ok((app_config, db))
}
```

For `Backtest`, load config, migrate DB, load bars from CSV, build `BacktestSettings`, run runtime, and print:

```text
backtest completed: signals=<N> orders=<N>
```

- [ ] **Step 4: Run tests and commit**

Run:

```powershell
cargo test -p trader-cli
cargo check --workspace --locked
```

Expected: both commands pass.

Commit:

```powershell
git add apps/trader-cli Cargo.lock
git commit -m "feat: add cli backtest workflow"
```

---

### Task 7: API Backtest and Query Routes

**Files:**
- Modify: `crates/api/Cargo.toml`
- Modify: `crates/api/src/lib.rs`
- Create: `crates/api/src/state.rs`
- Create: `crates/api/tests/backtest_api_tests.rs`
- Modify: `apps/trader-server/Cargo.toml`
- Modify: `apps/trader-server/src/main.rs`

- [ ] **Step 1: Add API dependencies**

Update `crates/api/Cargo.toml`:

```toml
[dependencies]
axum.workspace = true
backtest = { path = "../backtest" }
config = { path = "../config" }
data = { path = "../data" }
serde.workspace = true
storage = { path = "../storage" }
tokio.workspace = true
tower.workspace = true
```

- [ ] **Step 2: Write API integration test**

Create `crates/api/tests/backtest_api_tests.rs`:

```rust
use api::{AppState, router_with_state};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use storage::Db;
use tower::ServiceExt;

#[tokio::test]
async fn post_backtest_returns_created() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = router_with_state(AppState::new(db, "configs/backtest/ma_cross.toml".into()));

    let response = app
        .oneshot(Request::builder().method("POST").uri("/api/v1/backtests").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
}
```

- [ ] **Step 3: Implement API state**

Create `crates/api/src/state.rs`:

```rust
use storage::Db;

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub config_path: String,
}

impl AppState {
    pub fn new(db: Db, config_path: String) -> Self {
        Self { db, config_path }
    }
}
```

- [ ] **Step 4: Implement routes**

Update `crates/api/src/lib.rs`:

```rust
mod state;

pub use state::AppState;

pub fn router_with_state(state: AppState) -> Router {
    Router::new()
        .route("/api/v1/health", get(health))
        .route("/api/v1/backtests", post(run_backtest))
        .route("/api/v1/orders", get(list_orders))
        .route("/api/v1/positions", get(list_positions))
        .with_state(state)
}
```

`run_backtest` must load config, load CSV bars, run `BacktestRuntime`, and return `(StatusCode::CREATED, Json(summary))`. `list_orders` and `list_positions` read `run_id` from the loaded config and return JSON arrays from storage.

- [ ] **Step 5: Wire server state**

Update `apps/trader-server/src/main.rs` to read config path from `TRADER_CONFIG` or default to `configs/backtest/ma_cross.toml`, connect and migrate DB, then call `api::router_with_state`.

- [ ] **Step 6: Run tests and commit**

Run:

```powershell
cargo test -p api
cargo check --workspace --locked
```

Expected: both commands pass.

Commit:

```powershell
git add crates/api apps/trader-server Cargo.lock
git commit -m "feat: add backtest api routes"
```

---

### Task 8: Final Phase 2 Verification and Docs

**Files:**
- Modify: `README.md`
- Modify: `tech.md`
- Modify: `docs/superpowers/plans/2026-06-01-trader-paper-mvp-plan.md`

- [ ] **Step 1: Run full verification**

Run:

```powershell
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace
cargo run -p trader-cli -- check-config --config configs/backtest/ma_cross.toml
cargo run -p trader-cli -- backtest --config configs/backtest/ma_cross.toml
```

Expected:

```text
config ok
backtest completed: signals=1 orders=1
```

- [ ] **Step 2: Run server smoke**

Start:

```powershell
cargo run -p trader-server
```

In another shell:

```powershell
Invoke-RestMethod -Method Post http://127.0.0.1:8080/api/v1/backtests
Invoke-RestMethod http://127.0.0.1:8080/api/v1/orders
Invoke-RestMethod http://127.0.0.1:8080/api/v1/positions
```

Expected: POST returns a summary with `signals = 1` and `orders = 1`; query routes return non-empty arrays.

- [ ] **Step 3: Update README**

Add:

~~~markdown
## Paper MVP

```powershell
cargo run -p trader-cli -- migrate --config configs/backtest/ma_cross.toml
cargo run -p trader-cli -- backtest --config configs/backtest/ma_cross.toml
cargo run -p trader-server
```
~~~

- [ ] **Step 4: Update tech.md**

Add a short Phase 2 status section:

```markdown
## Phase 2 Paper MVP

Phase 2 turns the V1 skeleton into a local paper/backtest workflow: config loading, SQLite persistence, CSV bar loading, persistent backtest output, CLI commands, and REST query routes.
```

- [ ] **Step 5: Mark plan complete and commit**

Change all task checkboxes in this plan to `- [x]`.

Run:

```powershell
git status --short
```

Expected: only README, tech, and this plan file are modified.

Commit:

```powershell
git add README.md tech.md docs/superpowers/plans/2026-06-01-trader-paper-mvp-plan.md
git commit -m "docs: document phase 2 paper mvp"
```

---

## Milestone Acceptance Criteria

Phase 2 is complete when:

- `trader check-config --config configs/backtest/ma_cross.toml` validates and prints `config ok`.
- `trader migrate --config configs/backtest/ma_cross.toml` creates SQLite schema.
- `trader backtest --config configs/backtest/ma_cross.toml` persists one run, one order, one fill, and one position from sample bars.
- `POST /api/v1/backtests` runs the same workflow through Axum.
- `GET /api/v1/orders` returns persisted orders for the configured run.
- `GET /api/v1/positions` returns persisted positions for the configured run.
- `cargo fmt --all -- --check`, `cargo check --workspace --locked`, and `cargo test --workspace` pass.

## Self-Review

Spec coverage:

- Config-driven workflow: Tasks 1 and 6.
- SQLite persistence: Tasks 2 and 4.
- CSV data ingest: Task 3.
- Paper/backtest runtime: Tasks 4 and 5.
- API workflow: Task 7.
- CLI workflow: Task 6.
- Documentation and final verification: Task 8.

Placeholder scan:

- No `TBD`, `TODO`, `implement later`, or unspecified test commands are present.
- Each task has exact file paths, command lines, expected outcomes, and commit messages.

Type consistency:

- `BacktestSettings` is introduced before `PaperRuntime` and CLI/API use it.
- `Db` repository methods are introduced before backtest/API/CLI use them.
- `AppState` is introduced before server wiring uses it.
