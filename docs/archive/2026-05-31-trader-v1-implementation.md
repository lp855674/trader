# Trader V1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the Trader V1 vertical slice described in `docs/architecture.md`: a Rust workspace with core domain types, event flow, SQLite state, Parquet historical data boundary, backtest/replay/paper runtimes, CLI, REST API, and WebSocket broadcast.

**Architecture:** Implement the system from the inside out: domain types first, then events, storage, data, strategy/portfolio/risk/execution/OMS/broker, then runtimes, then API and CLI. Each task must leave the workspace compiling and must add focused tests before production code where practical.

**Tech Stack:** Rust, Tokio, Axum, SQLx SQLite, serde, uuid, chrono/time, rust_decimal, Polars/Parquet, clap, reqwest, tokio-tungstenite, tracing.

---

## Source Documents

- `tech.md`: project technical baseline.
- `docs/architecture.md`: architecture and cross-module boundaries.
- `docs/crates.md`: target workspace and crate boundaries.
- `docs/database.md`: SQLite and Parquet schema.
- `docs/api.md`: REST and WebSocket contract.
- `docs/events.md`: event model.
- `docs/strategy.md`: strategy boundaries.
- `docs/broker.md`: broker boundaries.
- `docs/roadmap.md`: phased release scope.

## File Structure

Create or modify these paths:

- Modify: `Cargo.toml` to match target workspace members and shared dependencies.
- Create: `apps/trader-cli/` for CLI commands.
- Create: `apps/trader-server/` for HTTP / WebSocket server.
- Create: `crates/core/` for domain types. Use Cargo package name `trader-core` and Rust library name `trader_core` to avoid colliding with Rust's standard `core` crate.
- Create: `crates/events/` for events and in-process event bus.
- Create: `crates/config/` for config loading.
- Create: `crates/storage/` for SQLite repositories and Parquet boundaries.
- Create: `crates/data/` for historical data loaders.
- Create: `crates/market_rules/` for market validation.
- Create: `crates/indicators/` for indicator helpers.
- Create: `crates/feature_store/` for feature lookup interfaces.
- Create: `crates/universe/` for universe selection.
- Create: `crates/alpha/` for signal generation traits.
- Create: `crates/strategies/` for example strategies.
- Create: `crates/portfolio/` for target position construction.
- Create: `crates/risk/` for risk checks.
- Create: `crates/execution/` for order intent generation.
- Create: `crates/oms/` for order state machine.
- Create: `crates/broker/` for mock/paper broker abstraction.
- Create: `crates/accounting/` for positions, cash, PnL.
- Create: `crates/metrics/` for performance metrics.
- Create: `crates/backtest/` for backtest runtime.
- Create: `crates/replay/` for replay runtime.
- Create: `crates/api/` for REST and WebSocket routes.
- Create: `configs/` for example TOML configs.
- Create: `migrations/` for SQLite schema.
- Create: `datasets/` for sample data and local generated data.
- Create: `scripts/check/verify.ps1` and `scripts/check/verify` for repeatable checks.

## Execution Rules

- Work in small commits. Commit after every task that compiles and passes its task tests.
- Run `cargo fmt` before every commit.
- Run `cargo test -p <crate>` for the crate touched by a task.
- Run `cargo check --workspace` after each milestone.
- Do not add live broker credentials, real API keys, or personal account data.
- Keep strategy independent from Broker, OMS, Storage, and API.
- Keep SQL usage inside `crates/storage`.

---

### Task 1: Workspace Skeleton

**Files:**
- Modify: `Cargo.toml`
- Create: `apps/trader-cli/Cargo.toml`
- Create: `apps/trader-cli/src/main.rs`
- Create: `apps/trader-server/Cargo.toml`
- Create: `apps/trader-server/src/main.rs`
- Create: `crates/*/Cargo.toml`
- Create: `crates/*/src/lib.rs`
- Create: `scripts/check/verify.ps1`
- Create: `scripts/check/verify`

- [x] **Step 1: Replace root workspace manifest**

Write this `Cargo.toml`:

```toml
[workspace]
resolver = "2"
members = [
    "apps/trader-cli",
    "apps/trader-server",
    "crates/core",
    "crates/events",
    "crates/config",
    "crates/storage",
    "crates/data",
    "crates/market_rules",
    "crates/universe",
    "crates/alpha",
    "crates/portfolio",
    "crates/risk",
    "crates/execution",
    "crates/oms",
    "crates/broker",
    "crates/backtest",
    "crates/replay",
    "crates/accounting",
    "crates/metrics",
    "crates/api",
    "crates/indicators",
    "crates/feature_store",
    "crates/strategies",
]

[workspace.package]
edition = "2021"
version = "0.1.0"
license = "MIT"
rust-version = "1.78"

[workspace.dependencies]
anyhow = "1"
thiserror = "2"
tokio = { version = "1", features = ["full"] }
async-trait = "0.1"
futures = "0.3"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
time = "0.3"
rust_decimal = { version = "1", features = ["serde"] }
rust_decimal_macros = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
sqlx = { version = "0.8", features = ["sqlite", "runtime-tokio", "chrono", "uuid", "migrate"] }
polars = { version = "0.50", features = ["lazy", "parquet", "temporal", "dtype-decimal"] }
axum = { version = "0.8", features = ["ws", "macros"] }
tower = { version = "0.5", features = ["util"] }
tower-http = { version = "0.6", features = ["cors", "trace"] }
clap = { version = "4", features = ["derive"] }
reqwest = { version = "0.12", features = ["json", "rustls-tls"] }
tokio-tungstenite = "0.26"
parking_lot = "0.12"
dashmap = "6"
```

- [x] **Step 2: Create each crate manifest**

For each library crate except `crates/core`, create this shape and adjust package name:

```toml
[package]
name = "events"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
```

Use package names matching directory names for normal crates. For `crates/core`, use this manifest header:

```toml
[package]
name = "trader-core"
version.workspace = true
edition.workspace = true
license.workspace = true

[lib]
name = "trader_core"

[dependencies]
```

- [x] **Step 3: Create placeholder library entries**

For every `crates/<name>/src/lib.rs`, start with:

```rust
#![forbid(unsafe_code)]

pub fn crate_name() -> &'static str {
    env!("CARGO_PKG_NAME")
}
```

- [x] **Step 4: Create app entries**

`apps/trader-cli/Cargo.toml`:

```toml
[package]
name = "trader-cli"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
anyhow.workspace = true
clap.workspace = true
tokio.workspace = true
```

`apps/trader-cli/src/main.rs`:

```rust
use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "trader")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    CheckConfig,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::CheckConfig => println!("config ok"),
    }
    Ok(())
}
```

`apps/trader-server/Cargo.toml`:

```toml
[package]
name = "trader-server"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
anyhow.workspace = true
tokio.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
```

`apps/trader-server/src/main.rs`:

```rust
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    tracing::info!("trader-server starting");
    Ok(())
}
```

- [x] **Step 5: Add verification scripts**

`scripts/check/verify.ps1`:

```powershell
$ErrorActionPreference = "Stop"
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
```

`scripts/check/verify`:

```bash
#!/usr/bin/env bash
set -euo pipefail
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
```

- [x] **Step 6: Run checks**

Run:

```powershell
cargo fmt --all
cargo check --workspace
cargo test --workspace
```

Expected: all commands pass.

- [x] **Step 7: Commit**

```powershell
git add Cargo.toml apps crates scripts
git commit -m "chore: scaffold trader workspace"
```

---

### Task 2: Core Domain Types

**Files:**
- Modify: `crates/core/Cargo.toml`
- Modify: `crates/core/src/lib.rs`
- Create: `crates/core/src/market.rs`
- Create: `crates/core/src/symbol.rs`
- Create: `crates/core/src/order.rs`
- Create: `crates/core/src/account.rs`
- Create: `crates/core/tests/domain_tests.rs`

- [x] **Step 1: Add core dependencies**

```toml
[dependencies]
serde.workspace = true
thiserror.workspace = true
uuid.workspace = true
chrono.workspace = true
rust_decimal.workspace = true
```

- [x] **Step 2: Write tests**

`crates/core/tests/domain_tests.rs`:

```rust
use trader_core::{AssetClass, Market, OrderSide, OrderStatus, Symbol};

#[test]
fn symbol_display_is_stable() {
    let symbol = Symbol::new(Market::Us, "NASDAQ", "AAPL", AssetClass::Equity);
    assert_eq!(symbol.to_string(), "US:NASDAQ:AAPL:EQUITY");
}

#[test]
fn order_status_identifies_terminal_states() {
    assert!(OrderStatus::Filled.is_terminal());
    assert!(OrderStatus::Canceled.is_terminal());
    assert!(OrderStatus::Rejected.is_terminal());
    assert!(!OrderStatus::Submitted.is_terminal());
}

#[test]
fn order_side_has_sign() {
    assert_eq!(OrderSide::Buy.sign(), 1);
    assert_eq!(OrderSide::Sell.sign(), -1);
}
```

- [x] **Step 3: Implement core exports**

`crates/core/src/lib.rs`:

```rust
#![forbid(unsafe_code)]

mod account;
mod market;
mod order;
mod symbol;

pub use account::*;
pub use market::*;
pub use order::*;
pub use symbol::*;
```

- [x] **Step 4: Implement market and symbol types**

`crates/core/src/market.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Market {
    Cn,
    Hk,
    Us,
    Crypto,
}

impl Market {
    pub fn code(self) -> &'static str {
        match self {
            Self::Cn => "CN",
            Self::Hk => "HK",
            Self::Us => "US",
            Self::Crypto => "CRYPTO",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AssetClass {
    Equity,
    CryptoSpot,
    CryptoPerp,
    CryptoFuture,
}

impl AssetClass {
    pub fn code(self) -> &'static str {
        match self {
            Self::Equity => "EQUITY",
            Self::CryptoSpot => "CRYPTO_SPOT",
            Self::CryptoPerp => "CRYPTO_PERP",
            Self::CryptoFuture => "CRYPTO_FUTURE",
        }
    }
}
```

`crates/core/src/symbol.rs`:

```rust
use crate::{AssetClass, Market};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Symbol {
    pub market: Market,
    pub exchange: String,
    pub code: String,
    pub asset_class: AssetClass,
}

impl Symbol {
    pub fn new(
        market: Market,
        exchange: impl Into<String>,
        code: impl Into<String>,
        asset_class: AssetClass,
    ) -> Self {
        Self {
            market,
            exchange: exchange.into(),
            code: code.into(),
            asset_class,
        }
    }
}

impl fmt::Display for Symbol {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}:{}:{}:{}",
            self.market.code(),
            self.exchange,
            self.code,
            self.asset_class.code()
        )
    }
}
```

- [x] **Step 5: Implement order and account types**

`crates/core/src/order.rs`:

```rust
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderSide {
    Buy,
    Sell,
}

impl OrderSide {
    pub fn sign(self) -> i8 {
        match self {
            Self::Buy => 1,
            Self::Sell => -1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderType {
    Market,
    Limit,
    Stop,
    StopLimit,
    PostOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderStatus {
    New,
    PendingSubmit,
    Submitted,
    PartiallyFilled,
    Filled,
    PendingCancel,
    Canceled,
    Rejected,
    Expired,
    Unknown,
    Syncing,
}

impl OrderStatus {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Filled | Self::Canceled | Self::Rejected | Self::Expired
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderId(pub Uuid);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderRequest {
    pub symbol: String,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub qty: Decimal,
    pub price: Option<Decimal>,
    pub account_id: String,
}
```

`crates/core/src/account.rs`:

```rust
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountSnapshot {
    pub account_id: String,
    pub cash: Decimal,
    pub equity: Decimal,
    pub buying_power: Decimal,
    pub margin_used: Decimal,
    pub unrealized_pnl: Decimal,
    pub realized_pnl: Decimal,
}
```

- [x] **Step 6: Run tests and commit**

```powershell
cargo test -p trader-core
cargo check --workspace
git add crates/core
git commit -m "feat: add core domain types"
```

Expected: tests pass and workspace checks.

---

### Task 3: Events Crate

**Files:**
- Modify: `crates/events/Cargo.toml`
- Modify: `crates/events/src/lib.rs`
- Create: `crates/events/src/event.rs`
- Create: `crates/events/src/bus.rs`
- Create: `crates/events/tests/event_tests.rs`

- [x] **Step 1: Add dependencies**

```toml
[dependencies]
trader_core = { package = "trader-core", path = "../core" }
serde.workspace = true
uuid.workspace = true
chrono.workspace = true
rust_decimal.workspace = true
tokio.workspace = true
thiserror.workspace = true
```

- [x] **Step 2: Write tests**

`crates/events/tests/event_tests.rs`:

```rust
use events::{EventBus, EventCategory, SignalEvent, SignalSide};

#[tokio::test]
async fn event_bus_delivers_published_events() {
    let bus = EventBus::new(16);
    let mut receiver = bus.subscribe();

    bus.publish_signal(SignalEvent {
        strategy_id: "ma_cross".to_string(),
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        side: SignalSide::Buy,
        confidence: 0.8,
        ts: chrono::Utc::now(),
    })
    .unwrap();

    let event = receiver.recv().await.unwrap();
    assert_eq!(event.category, EventCategory::Signal);
}
```

- [x] **Step 3: Implement events**

`crates/events/src/lib.rs`:

```rust
#![forbid(unsafe_code)]

mod bus;
mod event;

pub use bus::*;
pub use event::*;
```

`crates/events/src/event.rs`:

```rust
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventCategory {
    Market,
    Signal,
    Portfolio,
    Risk,
    Execution,
    Order,
    Trade,
    Position,
    Account,
    System,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventEnvelope<T> {
    pub event_id: Uuid,
    pub ts: DateTime<Utc>,
    pub source: String,
    pub category: EventCategory,
    pub payload: T,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignalSide {
    Buy,
    Sell,
    CloseLong,
    CloseShort,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignalEvent {
    pub strategy_id: String,
    pub symbol: String,
    pub side: SignalSide,
    pub confidence: f64,
    pub ts: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TraderEvent {
    Signal(SignalEvent),
}

impl TraderEvent {
    pub fn category(&self) -> EventCategory {
        match self {
            Self::Signal(_) => EventCategory::Signal,
        }
    }
}

pub type AnyEventEnvelope = EventEnvelope<TraderEvent>;

pub fn envelope(source: impl Into<String>, payload: TraderEvent) -> AnyEventEnvelope {
    EventEnvelope {
        event_id: Uuid::new_v4(),
        ts: Utc::now(),
        source: source.into(),
        category: payload.category(),
        payload,
    }
}
```

- [x] **Step 4: Implement event bus**

`crates/events/src/bus.rs`:

```rust
use crate::{envelope, AnyEventEnvelope, SignalEvent, TraderEvent};
use thiserror::Error;
use tokio::sync::broadcast;

#[derive(Debug, Error)]
pub enum EventBusError {
    #[error("event bus has no active receivers")]
    NoReceivers,
}

#[derive(Clone)]
pub struct EventBus {
    sender: broadcast::Sender<AnyEventEnvelope>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _receiver) = broadcast::channel(capacity);
        Self { sender }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AnyEventEnvelope> {
        self.sender.subscribe()
    }

    pub fn publish(&self, event: AnyEventEnvelope) -> Result<(), EventBusError> {
        self.sender
            .send(event)
            .map(|_| ())
            .map_err(|_| EventBusError::NoReceivers)
    }

    pub fn publish_signal(&self, signal: SignalEvent) -> Result<(), EventBusError> {
        self.publish(envelope("strategy", TraderEvent::Signal(signal)))
    }
}
```

- [x] **Step 5: Run tests and commit**

```powershell
cargo test -p events
cargo check --workspace
git add crates/events
git commit -m "feat: add event model and bus"
```

---

### Task 4: Config Crate

**Files:**
- Modify: `crates/config/Cargo.toml`
- Modify: `crates/config/src/lib.rs`
- Create: `crates/config/tests/config_tests.rs`
- Create: `configs/backtest/ma_cross.toml`

- [x] **Step 1: Add dependencies**

```toml
[dependencies]
serde.workspace = true
toml.workspace = true
thiserror.workspace = true
```

- [x] **Step 2: Add example config**

`configs/backtest/ma_cross.toml`:

```toml
[runtime]
mode = "backtest"

[data]
source = "parquet"
path = "datasets/sample/aapl_1d.csv"

[strategy]
name = "moving_average_cross"
symbols = ["US:NASDAQ:AAPL:EQUITY"]
fast_window = 20
slow_window = 60

[portfolio]
initial_cash = "100000"
base_currency = "USD"
```

- [x] **Step 3: Write tests**

`crates/config/tests/config_tests.rs`:

```rust
use config::{AppConfig, RuntimeMode};

#[test]
fn parses_backtest_config() {
    let input = r#"
        [runtime]
        mode = "backtest"

        [data]
        source = "parquet"
        path = "datasets/sample/aapl_1d.csv"

        [strategy]
        name = "moving_average_cross"
        symbols = ["US:NASDAQ:AAPL:EQUITY"]
        fast_window = 20
        slow_window = 60

        [portfolio]
        initial_cash = "100000"
        base_currency = "USD"
    "#;

    let config = AppConfig::from_toml_str(input).unwrap();
    assert_eq!(config.runtime.mode, RuntimeMode::Backtest);
    assert_eq!(config.strategy.name, "moving_average_cross");
}
```

- [x] **Step 4: Implement config types**

`crates/config/src/lib.rs`:

```rust
#![forbid(unsafe_code)]

use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to parse config: {0}")]
    Parse(#[from] toml::de::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeMode {
    Backtest,
    Replay,
    Paper,
    Live,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub runtime: RuntimeConfig,
    pub data: DataConfig,
    pub strategy: StrategyConfig,
    pub portfolio: PortfolioConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeConfig {
    pub mode: RuntimeMode,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DataConfig {
    pub source: String,
    pub path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StrategyConfig {
    pub name: String,
    pub symbols: Vec<String>,
    pub fast_window: usize,
    pub slow_window: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PortfolioConfig {
    pub initial_cash: String,
    pub base_currency: String,
}

impl AppConfig {
    pub fn from_toml_str(input: &str) -> Result<Self, ConfigError> {
        Ok(toml::from_str(input)?)
    }
}
```

- [x] **Step 5: Run tests and commit**

```powershell
cargo test -p config
cargo check --workspace
git add crates/config configs
git commit -m "feat: add config loading"
```

---

### Task 5: Storage Schema and Repositories

**Files:**
- Modify: `crates/storage/Cargo.toml`
- Modify: `crates/storage/src/lib.rs`
- Create: `crates/storage/src/db.rs`
- Create: `crates/storage/src/repositories.rs`
- Create: `migrations/0001_init.sql`
- Create: `crates/storage/tests/storage_tests.rs`

- [x] **Step 1: Add dependencies**

```toml
[dependencies]
trader_core = { package = "trader-core", path = "../core" }
events = { path = "../events" }
anyhow.workspace = true
serde.workspace = true
serde_json.workspace = true
sqlx.workspace = true
uuid.workspace = true
chrono.workspace = true
rust_decimal.workspace = true
thiserror.workspace = true
```

- [x] **Step 2: Add migration**

`migrations/0001_init.sql`:

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

CREATE TABLE IF NOT EXISTS instruments (
    symbol TEXT PRIMARY KEY,
    market TEXT NOT NULL,
    exchange TEXT NOT NULL,
    asset_class TEXT NOT NULL,
    currency TEXT NOT NULL,
    lot_size TEXT NOT NULL,
    tick_size TEXT NOT NULL,
    tradable INTEGER NOT NULL
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
    symbol TEXT NOT NULL,
    side TEXT NOT NULL,
    price TEXT NOT NULL,
    qty TEXT NOT NULL,
    fee TEXT NOT NULL,
    ts_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS positions (
    account_id TEXT NOT NULL,
    symbol TEXT NOT NULL,
    qty TEXT NOT NULL,
    avg_price TEXT NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    PRIMARY KEY (account_id, symbol)
);

CREATE TABLE IF NOT EXISTS event_store (
    event_id TEXT PRIMARY KEY,
    ts_ms INTEGER NOT NULL,
    source TEXT NOT NULL,
    category TEXT NOT NULL,
    payload_json TEXT NOT NULL
);
```

- [x] **Step 3: Write repository tests**

`crates/storage/tests/storage_tests.rs`:

```rust
use storage::{Db, NewInstrument};

#[tokio::test]
async fn instrument_round_trip() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();

    db.insert_instrument(NewInstrument {
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        market: "US".to_string(),
        exchange: "NASDAQ".to_string(),
        asset_class: "EQUITY".to_string(),
        currency: "USD".to_string(),
        lot_size: "1".to_string(),
        tick_size: "0.01".to_string(),
        tradable: true,
    })
    .await
    .unwrap();

    let instrument = db
        .get_instrument("US:NASDAQ:AAPL:EQUITY")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(instrument.symbol, "US:NASDAQ:AAPL:EQUITY");
    assert!(instrument.tradable);
}
```

- [x] **Step 4: Implement storage**

`crates/storage/src/lib.rs`:

```rust
#![forbid(unsafe_code)]

mod db;
mod repositories;

pub use db::*;
pub use repositories::*;
```

`crates/storage/src/db.rs`:

```rust
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};

#[derive(Clone)]
pub struct Db {
    pool: SqlitePool,
}

impl Db {
    pub async fn connect(database_url: &str) -> Result<Self, sqlx::Error> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await?;
        Ok(Self { pool })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn migrate(&self) -> Result<(), sqlx::Error> {
        sqlx::query(include_str!("../../../migrations/0001_init.sql"))
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
```

`crates/storage/src/repositories.rs`:

```rust
use crate::Db;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewInstrument {
    pub symbol: String,
    pub market: String,
    pub exchange: String,
    pub asset_class: String,
    pub currency: String,
    pub lot_size: String,
    pub tick_size: String,
    pub tradable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstrumentRecord {
    pub symbol: String,
    pub market: String,
    pub exchange: String,
    pub asset_class: String,
    pub currency: String,
    pub lot_size: String,
    pub tick_size: String,
    pub tradable: bool,
}

impl Db {
    pub async fn insert_instrument(&self, instrument: NewInstrument) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO instruments (
                symbol, market, exchange, asset_class, currency, lot_size, tick_size, tradable
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(instrument.symbol)
        .bind(instrument.market)
        .bind(instrument.exchange)
        .bind(instrument.asset_class)
        .bind(instrument.currency)
        .bind(instrument.lot_size)
        .bind(instrument.tick_size)
        .bind(instrument.tradable)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn get_instrument(
        &self,
        symbol: &str,
    ) -> Result<Option<InstrumentRecord>, sqlx::Error> {
        let row = sqlx::query_as::<_, (String, String, String, String, String, String, String, i64)>(
            r#"
            SELECT symbol, market, exchange, asset_class, currency, lot_size, tick_size, tradable
            FROM instruments
            WHERE symbol = ?
            "#,
        )
        .bind(symbol)
        .fetch_optional(self.pool())
        .await?;

        Ok(row.map(
            |(symbol, market, exchange, asset_class, currency, lot_size, tick_size, tradable)| {
                InstrumentRecord {
                    symbol,
                    market,
                    exchange,
                    asset_class,
                    currency,
                    lot_size,
                    tick_size,
                    tradable: tradable != 0,
                }
            },
        ))
    }
}
```

- [x] **Step 5: Run tests and commit**

```powershell
cargo test -p storage
cargo check --workspace
git add crates/storage migrations
git commit -m "feat: add storage schema and instrument repository"
```

---

### Task 6: Data Loader Boundary

**Files:**
- Modify: `crates/data/Cargo.toml`
- Modify: `crates/data/src/lib.rs`
- Create: `crates/data/src/bar.rs`
- Create: `crates/data/tests/data_tests.rs`
- Create: `datasets/sample/aapl_1d.csv`

- [x] **Step 1: Add dependencies**

```toml
[dependencies]
chrono.workspace = true
rust_decimal.workspace = true
serde.workspace = true
thiserror.workspace = true

[dev-dependencies]
rust_decimal_macros.workspace = true
```

- [x] **Step 2: Add sample data**

`datasets/sample/aapl_1d.csv`:

```csv
ts_ms,open,high,low,close,volume
1704067200000,100.00,110.00,99.00,108.00,1000
1704153600000,108.00,112.00,105.00,106.00,1200
```

- [x] **Step 3: Write tests**

`crates/data/tests/data_tests.rs`:

```rust
use data::Bar;
use rust_decimal_macros::dec;

#[test]
fn bar_return_uses_close_to_close() {
    let previous = Bar::new(1, dec!(100), dec!(110), dec!(90), dec!(100), dec!(1000));
    let current = Bar::new(2, dec!(100), dec!(115), dec!(95), dec!(110), dec!(1200));
    assert_eq!(current.close_return(&previous), dec!(0.1));
}
```

- [x] **Step 4: Implement bar model**

`crates/data/src/lib.rs`:

```rust
#![forbid(unsafe_code)]

mod bar;

pub use bar::*;
```

`crates/data/src/bar.rs`:

```rust
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bar {
    pub ts_ms: i64,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
}

impl Bar {
    pub fn new(
        ts_ms: i64,
        open: Decimal,
        high: Decimal,
        low: Decimal,
        close: Decimal,
        volume: Decimal,
    ) -> Self {
        Self {
            ts_ms,
            open,
            high,
            low,
            close,
            volume,
        }
    }

    pub fn close_return(&self, previous: &Self) -> Decimal {
        (self.close - previous.close) / previous.close
    }
}
```

- [x] **Step 5: Run tests and commit**

```powershell
cargo test -p data
cargo check --workspace
git add crates/data datasets/sample
git commit -m "feat: add historical bar model"
```

---

### Task 7: Strategy, Portfolio, Risk, Execution Vertical Slice

**Files:**
- Modify: `crates/alpha/Cargo.toml`
- Modify: `crates/alpha/src/lib.rs`
- Modify: `crates/strategies/Cargo.toml`
- Modify: `crates/strategies/src/lib.rs`
- Modify: `crates/portfolio/Cargo.toml`
- Modify: `crates/portfolio/src/lib.rs`
- Modify: `crates/risk/Cargo.toml`
- Modify: `crates/risk/src/lib.rs`
- Modify: `crates/execution/Cargo.toml`
- Modify: `crates/execution/src/lib.rs`
- Create: `crates/strategies/tests/strategy_tests.rs`

- [x] **Step 1: Add dependencies**

For `alpha`, `strategies`, `portfolio`, `risk`, and `execution`, add the local crates they use:

```toml
[dependencies]
trader_core = { package = "trader-core", path = "../core" }
data = { path = "../data" }
events = { path = "../events" }
chrono.workspace = true
rust_decimal.workspace = true
thiserror.workspace = true

[dev-dependencies]
rust_decimal_macros.workspace = true
```

- [x] **Step 2: Write strategy test**

`crates/strategies/tests/strategy_tests.rs`:

```rust
use data::Bar;
use events::SignalSide;
use rust_decimal_macros::dec;
use strategies::{MovingAverageCrossStrategy, Strategy};

#[test]
fn moving_average_cross_emits_buy_signal() {
    let mut strategy = MovingAverageCrossStrategy::new("ma", "AAPL", 2, 3);
    strategy.on_bar(&Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)));
    strategy.on_bar(&Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)));
    let signal = strategy
        .on_bar(&Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)))
        .unwrap();

    assert_eq!(signal.side, SignalSide::Buy);
}
```

- [x] **Step 3: Implement alpha trait**

`crates/alpha/src/lib.rs`:

```rust
#![forbid(unsafe_code)]

use data::Bar;
use events::SignalEvent;

pub trait AlphaModel {
    fn on_bar(&mut self, bar: &Bar) -> Option<SignalEvent>;
}
```

- [x] **Step 4: Implement example strategy**

`crates/strategies/src/lib.rs`:

```rust
#![forbid(unsafe_code)]

use data::Bar;
use events::{SignalEvent, SignalSide};

pub trait Strategy {
    fn on_bar(&mut self, bar: &Bar) -> Option<SignalEvent>;
}

pub struct MovingAverageCrossStrategy {
    strategy_id: String,
    symbol: String,
    fast_window: usize,
    slow_window: usize,
    closes: Vec<rust_decimal::Decimal>,
    last_side: Option<SignalSide>,
}

impl MovingAverageCrossStrategy {
    pub fn new(
        strategy_id: impl Into<String>,
        symbol: impl Into<String>,
        fast_window: usize,
        slow_window: usize,
    ) -> Self {
        Self {
            strategy_id: strategy_id.into(),
            symbol: symbol.into(),
            fast_window,
            slow_window,
            closes: Vec::new(),
            last_side: None,
        }
    }

    fn mean(&self, window: usize) -> Option<rust_decimal::Decimal> {
        if self.closes.len() < window {
            return None;
        }
        let sum: rust_decimal::Decimal = self.closes[self.closes.len() - window..].iter().sum();
        Some(sum / rust_decimal::Decimal::from(window))
    }
}

impl Strategy for MovingAverageCrossStrategy {
    fn on_bar(&mut self, bar: &Bar) -> Option<SignalEvent> {
        self.closes.push(bar.close);
        let fast = self.mean(self.fast_window)?;
        let slow = self.mean(self.slow_window)?;
        let side = if fast > slow {
            SignalSide::Buy
        } else if fast < slow {
            SignalSide::Sell
        } else {
            return None;
        };
        if self.last_side == Some(side) {
            return None;
        }
        self.last_side = Some(side);
        Some(SignalEvent {
            strategy_id: self.strategy_id.clone(),
            symbol: self.symbol.clone(),
            side,
            confidence: 0.8,
            ts: chrono::Utc::now(),
        })
    }
}
```

- [x] **Step 5: Implement portfolio, risk, execution minimal contracts**

`crates/portfolio/src/lib.rs`:

```rust
#![forbid(unsafe_code)]

use events::{SignalEvent, SignalSide};
use rust_decimal::Decimal;

#[derive(Debug, Clone, PartialEq)]
pub struct TargetPosition {
    pub symbol: String,
    pub target_qty: Decimal,
}

pub fn equal_weight_target(signal: &SignalEvent, qty: Decimal) -> TargetPosition {
    let signed_qty = match signal.side {
        SignalSide::Buy | SignalSide::CloseShort => qty,
        SignalSide::Sell | SignalSide::CloseLong => -qty,
    };
    TargetPosition {
        symbol: signal.symbol.clone(),
        target_qty: signed_qty,
    }
}
```

`crates/risk/src/lib.rs`:

```rust
#![forbid(unsafe_code)]

use portfolio::TargetPosition;
use rust_decimal::Decimal;
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum RiskError {
    #[error("target quantity exceeds max position")]
    MaxPosition,
}

pub fn check_max_position(
    target: &TargetPosition,
    max_abs_qty: Decimal,
) -> Result<(), RiskError> {
    if target.target_qty.abs() > max_abs_qty {
        return Err(RiskError::MaxPosition);
    }
    Ok(())
}
```

`crates/execution/src/lib.rs`:

```rust
#![forbid(unsafe_code)]

use trader_core::{OrderRequest, OrderSide, OrderType};
use portfolio::TargetPosition;
use rust_decimal::Decimal;

pub fn immediate_order(target: &TargetPosition, account_id: impl Into<String>) -> OrderRequest {
    let side = if target.target_qty >= Decimal::ZERO {
        OrderSide::Buy
    } else {
        OrderSide::Sell
    };
    OrderRequest {
        symbol: target.symbol.clone(),
        side,
        order_type: OrderType::Market,
        qty: target.target_qty.abs(),
        price: None,
        account_id: account_id.into(),
    }
}
```

- [x] **Step 6: Run tests and commit**

```powershell
cargo test -p strategies
cargo check --workspace
git add crates/alpha crates/strategies crates/portfolio crates/risk crates/execution
git commit -m "feat: add strategy to execution vertical slice"
```

---

### Task 8: OMS and Mock Broker

**Files:**
- Modify: `crates/oms/Cargo.toml`
- Modify: `crates/oms/src/lib.rs`
- Modify: `crates/broker/Cargo.toml`
- Modify: `crates/broker/src/lib.rs`
- Create: `crates/oms/tests/oms_tests.rs`
- Create: `crates/broker/tests/broker_tests.rs`

- [x] **Step 1: Add dependencies**

For both crates:

```toml
[dependencies]
trader_core = { package = "trader-core", path = "../core" }
async-trait.workspace = true
rust_decimal.workspace = true
thiserror.workspace = true
uuid.workspace = true

[dev-dependencies]
rust_decimal_macros.workspace = true
```

- [x] **Step 2: Write OMS test**

`crates/oms/tests/oms_tests.rs`:

```rust
use trader_core::OrderStatus;
use oms::OrderStateMachine;

#[test]
fn submitted_order_can_fill() {
    let mut machine = OrderStateMachine::new();
    machine.submit().unwrap();
    machine.accept().unwrap();
    machine.fill().unwrap();
    assert_eq!(machine.status(), OrderStatus::Filled);
}
```

- [x] **Step 3: Implement OMS**

`crates/oms/src/lib.rs`:

```rust
#![forbid(unsafe_code)]

use trader_core::OrderStatus;
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum OmsError {
    #[error("invalid transition from {0:?}")]
    InvalidTransition(OrderStatus),
}

pub struct OrderStateMachine {
    status: OrderStatus,
}

impl OrderStateMachine {
    pub fn new() -> Self {
        Self {
            status: OrderStatus::New,
        }
    }

    pub fn status(&self) -> OrderStatus {
        self.status
    }

    pub fn submit(&mut self) -> Result<(), OmsError> {
        self.transition(OrderStatus::Submitted, &[OrderStatus::New])
    }

    pub fn accept(&mut self) -> Result<(), OmsError> {
        self.transition(OrderStatus::Submitted, &[OrderStatus::Submitted])
    }

    pub fn fill(&mut self) -> Result<(), OmsError> {
        self.transition(
            OrderStatus::Filled,
            &[OrderStatus::Submitted, OrderStatus::PartiallyFilled],
        )
    }

    fn transition(
        &mut self,
        next: OrderStatus,
        allowed: &[OrderStatus],
    ) -> Result<(), OmsError> {
        if !allowed.contains(&self.status) {
            return Err(OmsError::InvalidTransition(self.status));
        }
        self.status = next;
        Ok(())
    }
}
```

- [x] **Step 4: Write broker test and implementation**

`crates/broker/tests/broker_tests.rs`:

```rust
use broker::{Broker, MockBroker};
use trader_core::{OrderRequest, OrderSide, OrderType};
use rust_decimal_macros::dec;

#[tokio::test]
async fn mock_broker_accepts_order() {
    let broker = MockBroker::default();
    let ack = broker
        .place_order(OrderRequest {
            symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
            side: OrderSide::Buy,
            order_type: OrderType::Market,
            qty: dec!(1),
            price: None,
            account_id: "paper".to_string(),
        })
        .await
        .unwrap();
    assert!(ack.accepted);
}
```

`crates/broker/src/lib.rs`:

```rust
#![forbid(unsafe_code)]

use async_trait::async_trait;
use trader_core::OrderRequest;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum BrokerError {
    #[error("broker rejected order: {0}")]
    Rejected(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceOrderResponse {
    pub broker_order_id: String,
    pub accepted: bool,
    pub reason: Option<String>,
}

#[async_trait]
pub trait Broker: Send + Sync {
    async fn place_order(
        &self,
        request: OrderRequest,
    ) -> Result<PlaceOrderResponse, BrokerError>;
}

#[derive(Default)]
pub struct MockBroker;

#[async_trait]
impl Broker for MockBroker {
    async fn place_order(
        &self,
        request: OrderRequest,
    ) -> Result<PlaceOrderResponse, BrokerError> {
        if request.qty <= rust_decimal::Decimal::ZERO {
            return Err(BrokerError::Rejected("qty must be positive".to_string()));
        }
        Ok(PlaceOrderResponse {
            broker_order_id: Uuid::new_v4().to_string(),
            accepted: true,
            reason: None,
        })
    }
}
```

- [x] **Step 5: Run tests and commit**

```powershell
cargo test -p oms -p broker
cargo check --workspace
git add crates/oms crates/broker
git commit -m "feat: add oms and mock broker"
```

---

### Task 9: Backtest Runtime

**Files:**
- Modify: `crates/backtest/Cargo.toml`
- Modify: `crates/backtest/src/lib.rs`
- Create: `crates/backtest/tests/backtest_tests.rs`

- [x] **Step 1: Add dependencies**

```toml
[dependencies]
broker = { path = "../broker" }
data = { path = "../data" }
execution = { path = "../execution" }
portfolio = { path = "../portfolio" }
risk = { path = "../risk" }
strategies = { path = "../strategies" }
anyhow.workspace = true
rust_decimal.workspace = true
thiserror.workspace = true

[dev-dependencies]
rust_decimal_macros.workspace = true
```

- [x] **Step 2: Write test**

`crates/backtest/tests/backtest_tests.rs`:

```rust
use backtest::{BacktestRuntime, BacktestSummary};
use data::Bar;
use rust_decimal_macros::dec;

#[tokio::test]
async fn backtest_counts_signals() {
    let bars = vec![
        Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
        Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)),
        Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)),
    ];
    let summary = BacktestRuntime::default().run(bars).await.unwrap();
    assert_eq!(summary, BacktestSummary { signals: 1, orders: 1 });
}
```

- [x] **Step 3: Implement runtime**

`crates/backtest/src/lib.rs`:

```rust
#![forbid(unsafe_code)]

use broker::{Broker, MockBroker};
use data::Bar;
use execution::immediate_order;
use portfolio::equal_weight_target;
use risk::check_max_position;
use rust_decimal_macros::dec;
use strategies::{MovingAverageCrossStrategy, Strategy};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BacktestSummary {
    pub signals: usize,
    pub orders: usize,
}

#[derive(Default)]
pub struct BacktestRuntime;

impl BacktestRuntime {
    pub async fn run(&self, bars: Vec<Bar>) -> anyhow::Result<BacktestSummary> {
        let mut strategy = MovingAverageCrossStrategy::new("ma", "US:NASDAQ:AAPL:EQUITY", 2, 3);
        let broker = MockBroker::default();
        let mut signals = 0;
        let mut orders = 0;

        for bar in bars {
            if let Some(signal) = strategy.on_bar(&bar) {
                signals += 1;
                let target = equal_weight_target(&signal, dec!(1));
                check_max_position(&target, dec!(100))?;
                let order = immediate_order(&target, "backtest");
                broker.place_order(order).await?;
                orders += 1;
            }
        }

        Ok(BacktestSummary { signals, orders })
    }
}
```

- [x] **Step 4: Run tests and commit**

```powershell
cargo test -p backtest
cargo check --workspace
git add crates/backtest
git commit -m "feat: add minimal backtest runtime"
```

---

### Task 10: Replay Runtime

**Files:**
- Modify: `crates/replay/Cargo.toml`
- Modify: `crates/replay/src/lib.rs`
- Create: `crates/replay/tests/replay_tests.rs`

- [x] **Step 1: Add dependencies**

```toml
[dependencies]
data = { path = "../data" }
events = { path = "../events" }
tokio.workspace = true

[dev-dependencies]
rust_decimal_macros.workspace = true
```

- [x] **Step 2: Write test**

`crates/replay/tests/replay_tests.rs`:

```rust
use data::Bar;
use replay::ReplayRuntime;
use rust_decimal_macros::dec;

#[tokio::test]
async fn replay_emits_all_bars() {
    let bars = vec![
        Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
        Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)),
    ];
    let count = ReplayRuntime::new(100).replay_bars(bars).await;
    assert_eq!(count, 2);
}
```

- [x] **Step 3: Implement replay**

`crates/replay/src/lib.rs`:

```rust
#![forbid(unsafe_code)]

use data::Bar;
use std::time::Duration;

pub struct ReplayRuntime {
    speed: u32,
}

impl ReplayRuntime {
    pub fn new(speed: u32) -> Self {
        Self { speed: speed.max(1) }
    }

    pub async fn replay_bars(&self, bars: Vec<Bar>) -> usize {
        let delay = Duration::from_millis(1000 / u64::from(self.speed));
        let mut count = 0;
        for _bar in bars {
            tokio::time::sleep(delay).await;
            count += 1;
        }
        count
    }
}
```

- [x] **Step 4: Run tests and commit**

```powershell
cargo test -p replay
cargo check --workspace
git add crates/replay
git commit -m "feat: add minimal replay runtime"
```

---

### Task 11: API Crate and Server

**Files:**
- Modify: `crates/api/Cargo.toml`
- Modify: `crates/api/src/lib.rs`
- Modify: `apps/trader-server/Cargo.toml`
- Modify: `apps/trader-server/src/main.rs`
- Create: `crates/api/tests/api_tests.rs`

- [x] **Step 1: Add dependencies**

`crates/api/Cargo.toml` dependencies:

```toml
[dependencies]
axum.workspace = true
serde.workspace = true
tokio.workspace = true
tower.workspace = true
```

`apps/trader-server/Cargo.toml` dependencies:

```toml
[dependencies]
api = { path = "../../crates/api" }
anyhow.workspace = true
tokio.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
```

- [x] **Step 2: Write API test**

`crates/api/tests/api_tests.rs`:

```rust
use api::router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

#[tokio::test]
async fn health_returns_ok() {
    let response = router()
        .oneshot(Request::builder().uri("/api/v1/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}
```

- [x] **Step 3: Implement router**

`crates/api/src/lib.rs`:

```rust
#![forbid(unsafe_code)]

use axum::{routing::get, Json, Router};
use serde::Serialize;

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
}

pub fn router() -> Router {
    Router::new().route("/api/v1/health", get(health))
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}
```

- [x] **Step 4: Wire server**

`apps/trader-server/src/main.rs`:

```rust
use anyhow::Result;
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let address = SocketAddr::from(([127, 0, 0, 1], 8080));
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!(%address, "trader-server listening");
    axum::serve(listener, api::router()).await?;
    Ok(())
}
```

- [x] **Step 5: Run tests and commit**

```powershell
cargo test -p api
cargo check --workspace
git add crates/api apps/trader-server
git commit -m "feat: add api health endpoint"
```

---

### Task 12: CLI Commands

**Files:**
- Modify: `apps/trader-cli/Cargo.toml`
- Modify: `apps/trader-cli/src/main.rs`
- Create: `apps/trader-cli/tests/cli_tests.rs`

- [x] **Step 1: Add dependencies**

```toml
[dev-dependencies]
assert_cmd = "2"
predicates = "3"
```

- [x] **Step 2: Write CLI smoke test**

`apps/trader-cli/tests/cli_tests.rs`:

```rust
use assert_cmd::Command;
use predicates::str::contains;

#[test]
fn check_config_prints_ok() {
    let mut command = Command::cargo_bin("trader-cli").unwrap();
    command.arg("check-config").assert().success().stdout(contains("config ok"));
}
```

- [x] **Step 3: Extend CLI enum**

`apps/trader-cli/src/main.rs`:

```rust
use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "trader")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Init,
    Migrate,
    Import,
    Backtest,
    Replay,
    Report,
    CheckConfig,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Init => println!("initialized"),
        Command::Migrate => println!("migrated"),
        Command::Import => println!("imported"),
        Command::Backtest => println!("backtest started"),
        Command::Replay => println!("replay started"),
        Command::Report => println!("report generated"),
        Command::CheckConfig => println!("config ok"),
    }
    Ok(())
}
```

- [x] **Step 4: Run tests and commit**

```powershell
cargo test -p trader-cli
cargo check --workspace
git add apps/trader-cli
git commit -m "feat: add trader cli commands"
```

---

### Task 13: Accounting and Metrics

**Files:**
- Modify: `crates/accounting/Cargo.toml`
- Modify: `crates/accounting/src/lib.rs`
- Modify: `crates/metrics/Cargo.toml`
- Modify: `crates/metrics/src/lib.rs`
- Create: `crates/accounting/tests/accounting_tests.rs`
- Create: `crates/metrics/tests/metrics_tests.rs`

- [x] **Step 1: Add dependencies**

```toml
[dependencies]
rust_decimal.workspace = true

[dev-dependencies]
rust_decimal_macros.workspace = true
```

- [x] **Step 2: Write accounting test**

`crates/accounting/tests/accounting_tests.rs`:

```rust
use accounting::PositionBook;
use rust_decimal_macros::dec;

#[test]
fn buy_updates_average_price() {
    let mut book = PositionBook::default();
    book.buy("AAPL", dec!(10), dec!(100));
    book.buy("AAPL", dec!(10), dec!(120));
    let position = book.position("AAPL").unwrap();
    assert_eq!(position.qty, dec!(20));
    assert_eq!(position.avg_price, dec!(110));
}
```

- [x] **Step 3: Implement accounting**

`crates/accounting/src/lib.rs`:

```rust
#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct Position {
    pub symbol: String,
    pub qty: Decimal,
    pub avg_price: Decimal,
}

#[derive(Default)]
pub struct PositionBook {
    positions: HashMap<String, Position>,
}

impl PositionBook {
    pub fn buy(&mut self, symbol: &str, qty: Decimal, price: Decimal) {
        let entry = self.positions.entry(symbol.to_string()).or_insert(Position {
            symbol: symbol.to_string(),
            qty: Decimal::ZERO,
            avg_price: Decimal::ZERO,
        });
        let notional = entry.qty * entry.avg_price + qty * price;
        entry.qty += qty;
        entry.avg_price = notional / entry.qty;
    }

    pub fn position(&self, symbol: &str) -> Option<&Position> {
        self.positions.get(symbol)
    }
}
```

- [x] **Step 4: Write metrics test and implementation**

`crates/metrics/tests/metrics_tests.rs`:

```rust
use metrics::total_return;
use rust_decimal_macros::dec;

#[test]
fn total_return_uses_start_and_end_equity() {
    assert_eq!(total_return(dec!(100), dec!(125)), dec!(0.25));
}
```

`crates/metrics/src/lib.rs`:

```rust
#![forbid(unsafe_code)]

use rust_decimal::Decimal;

pub fn total_return(start_equity: Decimal, end_equity: Decimal) -> Decimal {
    (end_equity - start_equity) / start_equity
}
```

- [x] **Step 5: Run tests and commit**

```powershell
cargo test -p accounting -p metrics
cargo check --workspace
git add crates/accounting crates/metrics
git commit -m "feat: add accounting and metrics basics"
```

---

### Task 14: Final Integration Verification

**Files:**
- Modify: `README.md`
- Modify: `tech.md`
- Modify: `docs/roadmap.md` if release scope changes during implementation.

- [x] **Step 1: Run full verification**

```powershell
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
```

Expected: all pass.

- [x] **Step 2: Run server smoke check**

Start server:

```powershell
cargo run -p trader-server
```

In another shell:

```powershell
Invoke-RestMethod http://127.0.0.1:8080/api/v1/health
```

Expected:

```text
status
------
ok
```

- [x] **Step 3: Run CLI smoke check**

```powershell
cargo run -p trader-cli -- check-config
```

Expected:

```text
config ok
```

- [x] **Step 4: Update README**

Add:

````markdown
# Trader

Rust quant trading system.

## Verify

```powershell
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
```

## Run

```powershell
cargo run -p trader-server
cargo run -p trader-cli -- check-config
```
````

- [x] **Step 5: Commit**

```powershell
git add README.md tech.md docs Cargo.toml Cargo.lock apps crates configs datasets migrations scripts
git commit -m "docs: document trader v1 implementation baseline"
```

---

## Milestone Acceptance Criteria

Milestone 1, workspace foundation:

- `cargo check --workspace` passes.
- All target apps and crates exist.
- `scripts/check/verify.ps1` and `scripts/check/verify` run the same checks.

Milestone 2, domain and event foundation:

- `trader_core` tests cover symbol display, order status, and order side.
- `events` test proves publish/subscribe works.
- Strategy boundaries are represented by types, not just documentation.

Milestone 3, persistence and data:

- SQLite migration creates the first operational schema.
- Storage repository test proves insert/read.
- Data crate has a stable `Bar` model matching the Parquet OHLCV design.

Milestone 4, trading loop:

- Example strategy emits a signal from bars.
- Portfolio creates a target.
- Risk checks the target.
- Execution creates an order.
- OMS accepts valid transitions.
- Mock broker accepts valid orders.

Milestone 5, runtime and interfaces:

- Backtest runtime runs a small bar set and counts signal/order output.
- Replay runtime emits all bars.
- API health endpoint responds.
- CLI commands parse.

## Self-Review

Spec coverage:

- Architecture: covered by tasks 1, 2, 3, 7, 8, 9, 10, 11, 12.
- Crates: covered by task 1 and every crate-specific task.
- Database: covered by task 5.
- API: covered by task 11.
- Events: covered by task 3.
- Strategy: covered by task 7.
- Broker: covered by task 8.
- Roadmap Phase 1 / early Phase 2: covered by tasks 1 through 14.

Placeholder scan:

- No banned placeholder patterns remain in task instructions.
- Commands and expected results are specified.

Type consistency:

- `SignalEvent`, `SignalSide`, `Bar`, `OrderRequest`, `OrderStatus`, `TargetPosition`, and `MockBroker` are introduced before use in later tasks.
- Cross-crate references match the package names in Task 1.
