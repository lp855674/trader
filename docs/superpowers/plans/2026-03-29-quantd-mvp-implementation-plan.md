# quantd MVP（四市场最薄闭环）实现计划

> **For agentic workers:** 建议按任务顺序执行；每步可用 checkbox（`- [ ]`）勾选。实现时遵守架构规格：`docs/specs/2026-03-29-quant-backend-architecture-design.md`。

**Goal:** 在空仓库中建立 Rust workspace，落地 **SQLite 权威台账 + 多数据源/多执行接口边界 + HTTP 与 WebSocket**，并用 **集成测试** 验证 **美股/港股/币/Polymarket** 四类 `Venue` 各跑通一条 **ingest → signal → risk → paper 执行 → 订单落库** 的最薄闭环。

**Architecture:** `domain` 纯类型；唯 `db` 使用 `sqlx`；`ingest`/`exec` 仅 trait + 注册表 + MVP 的 mock/paper 实现；`strategy` 内置极简规则策略；`api` 提供 HTTP 管理与 WS 推送；`quantd` 组装 Tokio 运行时与可选后台 tick。MVP **不**接真实券商/交易所；`live` 适配器占位返回明确错误码 `execution_not_configured`。

**Tech Stack:** Rust 2021、`tokio`、`sqlx`（sqlite + runtime-tokine + migrate）、`axum`（HTTP + WebSocket）、`tracing`/`tracing-subscriber`、`serde`/`serde_json`、`uuid`、`thiserror`。

**MVP 收口假设（规格 §9 未决项的默认选择）：**  
- 对外：**HTTP 必备**；**WebSocket** 推送 **订单/成交** 事件；**gRPC 延后**。  
- 业务配置：首版可用 **TOML/YAML + 环境变量** 指定库路径与监听地址；是否在后续版本改为 **以 DB 为配置真源** 由 `tech.md` 另议。  
- 每 venue 的 **live** 网关：仅 **trait + 注册位**，不提供默认可用实现。

---

## 文件与 crate 结构（创建前总览）

| 路径 | 职责 |
|------|------|
| `Cargo.toml` | workspace members：`domain`, `config`, `db`, `ingest`, `exec`, `strategy`, `api`, `quantd` |
| `crates/domain/` | `Venue`, `InstrumentId`, `AccountMode`, `Signal`, `OrderIntent`, `NormalizedBar` 等 |
| `crates/config/` | `AppConfig`：数据库路径、`bind_addr`、日志级别 |
| `crates/db/` | `Db`、连接、migrations、`InstrumentRepository`、`OrderRepository` 等 |
| `crates/db/migrations/001_initial.sql` | 首版表：instruments、accounts、data_sources、execution_profiles、bars、signals、risk_decisions、orders、fills |
| `crates/ingest/` | `IngestAdapter` trait、`IngestRegistry`、各 venue 的 `MockBarsAdapter` |
| `crates/exec/` | `ExecutionAdapter`、`ExecutionRouter`、`PaperAdapter`、`LiveStubAdapter` |
| `crates/strategy/` | `Strategy` trait、`PassthroughSignalStrategy`（或「最后一根 bar 触发固定意向」） |
| `crates/api/` | `axum` Router：`GET /health`、`GET /v1/instruments`、`WS /v1/stream` |
| `crates/quantd/src/main.rs` | 读配置、连库、migrate、启动 HTTP、可选 `tokio::spawn` 跑一轮流水线 |
| `rules.md` / `tech.md` | 从 `bot` 裁剪：DB 边界、日志字段、禁止 unwrap 等 |

---

### Task 1: 初始化 Cargo workspace 与空 crate

**Files:**
- Create: `Cargo.toml`
- Create: `crates/domain/Cargo.toml`, `crates/domain/src/lib.rs`
- Create: `crates/config/Cargo.toml`, `crates/config/src/lib.rs`
- Create: `crates/db/Cargo.toml`, `crates/db/src/lib.rs`
- Create: `crates/ingest/Cargo.toml`, `crates/ingest/src/lib.rs`
- Create: `crates/exec/Cargo.toml`, `crates/exec/src/lib.rs`
- Create: `crates/strategy/Cargo.toml`, `crates/strategy/src/lib.rs`
- Create: `crates/api/Cargo.toml`, `crates/api/src/lib.rs`
- Create: `crates/quantd/Cargo.toml`, `crates/quantd/src/main.rs`

- [x] **Step 1: 写入根 workspace `Cargo.toml`**

```toml
[workspace]
resolver = "2"
members = [
    "crates/domain",
    "crates/config",
    "crates/db",
    "crates/ingest",
    "crates/exec",
    "crates/strategy",
    "crates/api",
    "crates/quantd",
]
```

- [x] **Step 2: 各 member 先设 `edition = "2021"` 与空 `lib.rs` / `quantd` 的 `main` 打印 `hello`**

- [x] **Step 3: 验证编译**

Run: `cargo check -q`  
Expected: 无 error（warnings 可暂忽略）。

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml crates/
git commit -m "chore: init quantd workspace crates"
```

---

### Task 2: `domain` 类型与单元测试

**Files:**
- Create: `crates/domain/src/venue.rs`
- Create: `crates/domain/src/ids.rs`
- Modify: `crates/domain/src/lib.rs`（`pub mod` 与重导出）
- Create: `crates/domain/src/lib.rs` 内 `#[cfg(test)] mod tests`

- [x] **Step 1: 定义 `Venue` 四枚举值 + `InstrumentId`（venue + symbol 字符串）**

`crates/domain/src/venue.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Venue {
    UsEquity,
    HkEquity,
    Crypto,
    Polymarket,
}
```

`crates/domain/src/ids.rs`:

```rust
use crate::venue::Venue;

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct InstrumentId {
    pub venue: Venue,
    pub symbol: String,
}
```

- [x] **Step 2: 增加 `AccountMode`、`Side`、`Signal`、`OrderIntent`（字段与规格一致：strategy_id、instrument、qty、side、ts）**

- [x] **Step 3: 写单元测试 `instrument_id_roundtrip_json`**

- [ ] **Step 4: `cargo test -p domain` 全绿后 commit**

```bash
git add crates/domain
git commit -m "feat(domain): venue, instrument id, signal types"
```

---

### Task 3: 数据库迁移与 `Db::connect`

**Files:**
- Create: `crates/db/migrations/001_initial.sql`
- Create: `crates/db/src/error.rs`
- Modify: `crates/db/src/lib.rs`

- [x] **Step 1: 编写 `001_initial.sql`（完整可执行）**

```sql
-- instruments: 全局可交易标的
CREATE TABLE IF NOT EXISTS instruments (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    venue TEXT NOT NULL,
    symbol TEXT NOT NULL,
    meta_json TEXT,
    UNIQUE(venue, symbol)
);

CREATE TABLE IF NOT EXISTS data_sources (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    config_json TEXT
);

CREATE TABLE IF NOT EXISTS execution_profiles (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    config_json TEXT
);

CREATE TABLE IF NOT EXISTS accounts (
    id TEXT PRIMARY KEY,
    mode TEXT NOT NULL,
    execution_profile_id TEXT NOT NULL REFERENCES execution_profiles(id),
    venue TEXT
);

CREATE TABLE IF NOT EXISTS bars (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    instrument_id INTEGER NOT NULL REFERENCES instruments(id),
    data_source_id TEXT NOT NULL REFERENCES data_sources(id),
    ts_ms INTEGER NOT NULL,
    o REAL NOT NULL,
    h REAL NOT NULL,
    l REAL NOT NULL,
    c REAL NOT NULL,
    volume REAL NOT NULL DEFAULT 0,
    UNIQUE(instrument_id, data_source_id, ts_ms)
);

CREATE TABLE IF NOT EXISTS signals (
    id TEXT PRIMARY KEY,
    instrument_id INTEGER NOT NULL REFERENCES instruments(id),
    strategy_id TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS risk_decisions (
    id TEXT PRIMARY KEY,
    signal_id TEXT NOT NULL REFERENCES signals(id),
    allow INTEGER NOT NULL,
    reason TEXT,
    created_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS orders (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts(id),
    instrument_id INTEGER NOT NULL REFERENCES instruments(id),
    side TEXT NOT NULL,
    qty REAL NOT NULL,
    status TEXT NOT NULL,
    idempotency_key TEXT,
    created_at_ms INTEGER NOT NULL,
    UNIQUE(account_id, idempotency_key)
);

CREATE TABLE IF NOT EXISTS fills (
    id TEXT PRIMARY KEY,
    order_id TEXT NOT NULL REFERENCES orders(id),
    qty REAL NOT NULL,
    price REAL NOT NULL,
    created_at_ms INTEGER NOT NULL
);
```

- [x] **Step 2: `db` crate 暴露 `pub struct Db { pool: SqlitePool }` 与 `pub async fn connect(database_url: &str) -> Result<Self, DbError>`，内部 `sqlx::sqlite::SqlitePoolOptions::new().max_connections(5).connect(database_url).await?` 与 `sqlx::migrate!("./migrations").run(&pool).await?`**

依赖：`sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite", "migrate"] }`，`tokio` 与 `thiserror`。

- [x] **Step 3: 集成测试 `crates/db/tests/connect_migrate.rs`：临时文件路径 `sqlite::memory:` 或 `tempfile` 文件**

```rust
#[tokio::test]
async fn migrate_runs_clean() {
    let db = db::Db::connect("sqlite::memory:").await.expect("migrate");
    drop(db);
}
```

Run: `cargo test -p db`  
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/db
git commit -m "feat(db): initial schema and connect+migrate"
```

---

### Task 4: `InstrumentRepository` 与 `OrderRepository`

**Files:**
- Create: `crates/db/src/instruments.rs`
- Create: `crates/db/src/orders.rs`
- Modify: `crates/db/src/lib.rs`

- [x] **Step 1: `insert_instrument_get_id(venue, symbol) -> i64` 与 `get_by_venue_symbol`**

- [x] **Step 2: `insert_order` / `list_orders_by_account`（MVP 查询即可）**

- [x] **Step 3: 测试：插入 instrument → 插入 order → 读出**

Run: `cargo test -p db`  
Expected: PASS

- [ ] **Step 4: Commit** `feat(db): instrument and order repositories`

---

### Task 5: `IngestAdapter` 与按 `Venue` 的 MockBars

**Files:**
- Create: `crates/ingest/src/adapter.rs`
- Create: `crates/ingest/src/mock_bars.rs`
- Create: `crates/ingest/src/registry.rs`
- Modify: `crates/ingest/Cargo.toml`（依赖 `domain`, `db`, `async-trait`）

- [x] **Step 1: 定义 trait**

```rust
#[async_trait::async_trait]
pub trait IngestAdapter: Send + Sync {
    fn data_source_id(&self) -> &'static str;
    fn venue(&self) -> domain::Venue;
    /// 写入至少一条 bar；幂等依赖 DB UNIQUE(instrument_id, data_source_id, ts_ms)
    async fn ingest_once(&self, db: &db::Db, instrument_db_id: i64) -> Result<(), IngestError>;
}
```

- [x] **Step 2: `MockBarsAdapter`：固定 `ts_ms = 1`，OHLC 全 100.0，`venue` 构造时传入**

- [x] **Step 3: `IngestRegistry`：`Vec<Arc<dyn IngestAdapter>>` + `for_venue(Venue) -> impl Iterator`**

- [x] **Step 4: 单测：内存库 + mock ingest 后 `bars` 表有 1 行**

Run: `cargo test -p ingest`  
Expected: PASS

- [ ] **Step 5: Commit** `feat(ingest): trait, registry, mock bars per venue`

---

### Task 6: `ExecutionAdapter`、`PaperAdapter`、`LiveStubAdapter`、`ExecutionRouter`

**Files:**
- Create: `crates/exec/src/adapter.rs`
- Create: `crates/exec/src/paper.rs`
- Create: `crates/exec/src/live_stub.rs`
- Create: `crates/exec/src/router.rs`
- Modify: `crates/exec/Cargo.toml`

- [x] **Step 1: `ExecutionAdapter`：`place_order(intent, idempotency_key) -> Result<OrderAck, ExecError>`，`OrderAck` 含 `exchange_order_id` 字符串（paper 用 `paper-uuid`）**

- [x] **Step 2: `PaperAdapter`：在 `orders`/`fills` 表写入（通过 `db` 新方法 `insert_fill`）— 成交价用 domain 传入或固定 100.0**

- [x] **Step 3: `LiveStubAdapter`：`Err(ExecError::NotConfigured)`，`error_code = execution_not_configured`（`thiserror` + 字段或关联常量）**

- [x] **Step 4: `ExecutionRouter`：`resolve(account_id) -> Arc<dyn ExecutionAdapter>`，由内存 `HashMap` 配置；`paper` 账户指向 `PaperAdapter`，`live` 测试账户指向 `LiveStubAdapter`**

- [x] **Step 5: 单测 paper 路径落库 order+fill**

Run: `cargo test -p exec`  
Expected: PASS

- [ ] **Step 6: Commit** `feat(exec): paper adapter, live stub, router`

---

### Task 7: `strategy` 极简策略

**Files:**
- Create: `crates/strategy/src/lib.rs`
- Create: `crates/strategy/src/fixed_signal.rs`

- [x] **Step 1: `trait Strategy`：`fn evaluate(&self, ctx: &StrategyContext) -> Option<Signal>`**

`StrategyContext` 含 `instrument_db_id`、`venue`、`last_bar_close`（f64）。

- [x] **Step 2: `AlwaysLongOne`：若有 bar 则返回 `qty = 1.0` 的买入意向**

- [x] **Step 3: 单测（mock context）**

Run: `cargo test -p strategy`  
Expected: PASS

- [ ] **Step 4: Commit** `feat(strategy): fixed long-one strategy`

---

### Task 8: 端到端流水线函数（供 `quantd` 与集成测试复用）

**Files:**
- Create: `crates/pipeline/src/lib.rs`（独立 crate，供 `quantd` 与 `api` 复用）
- Modify: `crates/pipeline/Cargo.toml` 依赖 `db`, `ingest`, `exec`, `strategy`, `domain`, `uuid`
- Modify: `crates/quantd/tests/four_venues_mvp.rs` 使用 `quantd` 重导出的 `pipeline` 接口（见 `crates/quantd/src/lib.rs`）

- [x] **Step 1: `pub async fn run_one_tick(...)` 顺序：`ingest_once` → 读 last bar → `strategy.evaluate` → 写 `signals` → 风控（MVP：`allow = true` 写 `risk_decisions`）→ `router.place_order`**

- [x] **Step 2: 集成测试 `tests/four_venues_mvp.rs`（位于 `crates/quantd/tests/`）**

伪代码流程：

```rust
// 对每个 Venue：注册 instrument、注册 mock ingest、注册 account paper、run_one_tick
// 断言 orders 表该 account 有 1 条 NEW 或 FILLED 状态订单
```

Run: `cargo test -p quantd`  
Expected: PASS

- [ ] **Step 3: Commit** `feat(quantd): pipeline + four-venue integration test`

---

### Task 9: `config` 与 CLI 参数

**Files:**
- Modify: `crates/config/src/lib.rs`
- Modify: `crates/quantd/src/main.rs`

- [x] **Step 1: `AppConfig { database_url: String, http_bind: SocketAddr }`，从 `figment`+`toml` 或最小 `std::env::var("QUANTD_DATABASE_URL")` 默认 `sqlite:quantd.db`**

- [x] **Step 2: `main`：`tracing_subscriber::fmt::init()`，加载配置，`Db::connect`，`axum::serve`**

- [ ] **Step 3: Commit** `feat(config): env-based app config`

---

### Task 10: `api` — HTTP `GET /health` 与 `GET /v1/instruments`

**Files:**
- Create: `crates/api/src/lib.rs`（`pub fn router(state: AppState) -> Router`）
- Create: `crates/api/src/handlers.rs`
- Create: `crates/api/tests/http_smoke.rs`（使用 `tower::ServiceExt::oneshot`）

- [x] **Step 1: `GET /health` 返回 `{"status":"ok"}`**

- [x] **Step 2: `GET /v1/instruments` 返回 DB 中列表（JSON 数组）**

- [x] **Step 3: 集成测试 `tower::ServiceExt` 调用 router（MVP 采用 `crates/api/tests/http_smoke.rs`）**

Run: `cargo test -p api`  
Expected: PASS

- [ ] **Step 4: Commit** `feat(api): health and instruments HTTP routes`

---

### Task 11: WebSocket `/v1/stream`（订单事件最小推送）

**Files:**
- Modify: `crates/api/src/lib.rs`
- Create: `crates/api/src/ws.rs`
- Create: `crates/api/src/error.rs`（HTTP JSON `error_code` 统一返回）

- [x] **Step 1: 使用 `axum::extract::ws::WebSocketUpgrade`，握手后发送一条 `{"kind":"hello","schema_version":1}`**

- [x] **Step 2: 在 `pipeline` 完成下单后 `tokio::sync::broadcast::Sender` 发事件 `{ event_id, kind: order_created, payload }`；`api` 订阅该 sender 并向所有连接广播（MVP 单进程足够）**

- [x] **Step 3: 文档字段与规格 §7.1 对齐：`event_id` UUID、`error_code` 仅用于 error 帧**

- [ ] **Step 4: Commit** `feat(api): websocket stream with order broadcast`

---

### Task 12: 工程规范文档与 README

**Files:**
- Create: `rules.md`
- Create: `tech.md`
- Modify: `README.md`

- [x] **Step 1: `rules.md` 摘录 `bot` 的 DB 边界、禁止 `unwrap`、结构化 tracing 字段**

- [x] **Step 2: `tech.md` 描述 crate 职责、配置来源、MVP 限制**

- [x] **Step 3: `README`：如何 `cargo run -p quantd`、环境变量、跑测试**

- [ ] **Step 4: Commit** `docs: rules, tech, readme for trader`

---

## 规格覆盖自检（计划作者自检）

| 规格章节 | 对应任务 |
|----------|----------|
| §2 最小闭环 | Task 3–8（台账、ingest、signal、risk、exec、集成测四 venue） |
| §3.2 workspace 划分 | Task 1 |
| §3.5 多数据源 | Task 5：`data_source_id`、注册表、预留多源 |
| §3.6 多执行接口 | Task 6：Router、多 profile 映射（内存 HashMap） |
| §7.1 错误码 / WS 信封 | Task 11：`event_id`、错误帧约定 |
| §7.2 可观测性 | Task 9：`tracing` 初始化；流水线关键步骤 `info!` 带 `venue`、`account_id` |
| §8 与 bot 哲学对齐 | Task 12 文档固化 |

**已知延后（非 MVP）：** 真实 `live` 适配器、gRPC、Qlib 离线管道、冷数据 Parquet、`MarketDataView` 多源合并逻辑、生产级风控。

---

## 执行交接

计划文件：`docs/superpowers/plans/2026-03-29-quantd-mvp-implementation-plan.md`。

1. **分任务执行**：按 Task 1→12 顺序实现，每 Task 末尾 commit。  
2. **联调**：Task 8 通过后，四市场闭环即满足规格 Draft 的 MVP 定义；Task 10–11 补齐对外接口。

若要我在当前环境 **直接开始改代码**（从 Task 1 脚手架起），回复 **「开始实现 Task 1」** 即可。
