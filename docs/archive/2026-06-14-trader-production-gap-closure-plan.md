# Trader Production Gap Closure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the documented implementation gaps without breaking the current local-verifiable MVP, by adding durable audit projections, versioned market-rule data, and a staged path to contract accounting.

**Architecture:** Keep `event_store` as the immutable audit truth and add structured projection tables for operational queries. Keep Decimal domain math in Rust and persist decimals as strings at the storage boundary. Add schema and repository capabilities in small migrations, then expose read-only API routes only after repository tests pass.

**Tech Stack:** Rust workspace, SQLx SQLite, Axum, serde, rust_decimal, Polars/Parquet, PowerShell smoke scripts.

---

## Scope

This is a staged production-gap closure plan. It deliberately does not try to implement the full `docs/database.md` target schema in one change.

In scope:

- Mark current SQLite schema as MVP and target schema as future design.
- Add structured audit projections for order and risk events.
- Persist strategy insights and portfolio targets for post-run analysis.
- Add versioned market reference/rule tables and read repositories.
- Extend snapshots and positions toward margin/funding visibility.
- Define the contract-ledger work needed before claiming full crypto perp/future support.
- Add verification scripts and docs so future reports do not drift.

Out of scope for the first pass:

- Real-money live trading.
- High-frequency order book engine.
- Complete tick/order book historical data lake.
- Multi-user authorization and production secret management.
- Replacing existing Parquet bars/feature support.

## File Map

### Migrations and Storage

- Modify: `migrations/0001_init.sql`
  - Keep current MVP schema stable.
  - Do not rewrite old tables destructively.
- Create: `migrations/0002_audit_projections.sql`
  - Add `order_events`, `risk_events`, `insights`, `portfolio_targets`.
- Create: `migrations/0003_market_rules.sql`
  - Add `market_calendars`, `trading_sessions`, `fee_rules`, `lot_size_rules`, `price_limit_rules`.
- Create: `migrations/0004_contract_accounting.sql`
  - Add `crypto_positions`, `funding_rates`, and contract fields needed by snapshots.
- Modify: `crates/storage/src/db.rs`
  - Ensure all migrations run in numeric order.
- Modify: `crates/storage/src/repositories.rs`
  - Add command/read models and repository methods for new tables.
- Modify: `crates/storage/tests/storage_tests.rs`
  - Add migration and repository tests for new tables.
- Modify: `crates/storage/tests/runtime_repository_tests.rs`
  - Add audit projection write/read tests.

### Event and Algorithm Projections

- Modify: `crates/algorithm/src/algorithm.rs`
  - Add typed risk rejection payloads and expose enough data to project `risk_events`.
  - Persist insight and target projection inputs when alpha/portfolio stages run.
- Modify: `crates/paper/src/paper.rs`
  - Write order/risk/insight/target projections through storage after the immutable event is written.
- Modify: `crates/backtest/src/backtest.rs`
  - Same projection path as paper for backtest runs.
- Modify: `crates/events/src/event.rs`
  - Keep `RuntimeEvent` compatible; avoid making projections the event truth.

### Market Rules

- Modify: `crates/market_rules/src/market_rules.rs`
  - Add an optional data-backed rule provider interface.
  - Preserve current hard-coded fallback rules for existing tests.
- Modify: `crates/market_rules/tests/market_rules_tests.rs`
  - Add tests for static fallback and repository-backed rules.
- Modify: `crates/config/src/config.rs`
  - Add config for market-rule source selection if needed.

### API

- Modify: `crates/api/src/api.rs`
  - Add read-only routes for audit projections and market-rule reference data.
  - Keep command routes from bypassing Runtime/Risk/OMS.
- Modify: `crates/api/tests/api_tests.rs`
  - Add route tests for list order events, risk events, insights, portfolio targets.
- Modify: `docs/api.md`
  - Document new read-only endpoints and response ownership.

### CLI and Verification

- Modify: `apps/trader-cli/src/main.rs`
  - Add read-only inspection commands only if they are useful for local verification.
- Modify: `apps/trader-cli/tests/cli_tests.rs`
  - Cover any new CLI inspection commands.
- Modify: `scripts/v1-smoke.ps1`
  - Verify existing MVP behavior still passes after migrations.
- Create: `scripts/schema-gap-check.ps1`
  - Report implemented vs target schema without claiming target schema is complete.
- Modify: `scripts/verify.ps1`
  - Add schema boundary checks after Rust tests if runtime remains acceptable.

### Documentation

- Modify: `docs/database.md`
  - Split "current implemented SQLite schema" from "target production schema".
- Modify: `docs/分析.md`
  - Keep it aligned with the actual migration state after each phase.
- Modify: `tech.md`
  - Keep high-level rules only; do not paste table definitions or phase logs.
- Modify: `docs/roadmap.md`
  - Add staged gap-closure milestones.

---

## Acceptance Gates

Every task must preserve these gates:

- `cargo test -p storage`
- `cargo test -p algorithm`
- `cargo test -p paper`
- `cargo test -p backtest`
- `cargo test -p api`
- `cargo test -p market_rules`
- `powershell -ExecutionPolicy Bypass -File .\scripts\v1-smoke.ps1`
- `bash ./scripts/check-db-boundary`
- `bash ./scripts/check-storage-dto-boundary`
- `bash ./scripts/check-api-read-model-boundary`

If a task only changes docs, run:

- stale-report grep checks against `docs/分析.md`
- `git diff --check`

---

## Task 1: Align Documentation With Current Schema

**Files:**

- Modify: `docs/database.md`
- Modify: `docs/roadmap.md`
- Modify: `docs/分析.md`
- Modify: `tech.md`

- [ ] **Step 1: Add a current-vs-target schema section to `docs/database.md`**

Add a section near the top of `docs/database.md`:

```markdown
## Current Implementation Status

`migrations/0001_init.sql` is the current MVP SQLite schema. It contains:

- `strategy_runs`
- `instruments`
- `orders`
- `fills`
- `positions`
- `account_balances`
- `portfolio_snapshots`
- `event_store`

The complete schema below is the target production schema. A table appearing in the target schema does not mean it is implemented in the current migration.
```

- [ ] **Step 2: Update `docs/roadmap.md`**

Add a milestone named `Schema Gap Closure` with these bullets:

```markdown
## Schema Gap Closure

- Keep `event_store` as immutable audit truth.
- Add `order_events` and `risk_events` as query projections.
- Add `insights` and `portfolio_targets` for research and post-run analysis.
- Add market-rule reference tables before claiming configurable multi-market support.
- Add `crypto_positions` and `funding_rates` before claiming full crypto derivative accounting.
```

- [ ] **Step 3: Verify docs no longer imply target schema is implemented**

Run:

```powershell
rg -n "24.*implemented|Parquet 未实现|data/parquet/目录不存在|137|98.5" docs tech.md -g "*.md" -g "!docs/archive/**" -g "!docs/superpowers/plans/**"
```

Expected: no matches except historical archive files, if archives are intentionally searched.

- [ ] **Step 4: Commit**

```powershell
git add docs/database.md docs/roadmap.md docs/分析.md tech.md
git commit -m "docs: clarify current and target trader schema"
```

---

## Task 2: Add Structured Audit Projection Tables

**Files:**

- Create: `migrations/0002_audit_projections.sql`
- Modify: `crates/storage/src/repositories.rs`
- Modify: `crates/storage/tests/storage_tests.rs`
- Modify: `crates/storage/tests/runtime_repository_tests.rs`
- Modify: `docs/database.md`

- [ ] **Step 1: Write migration test for audit projection tables**

Add to `crates/storage/tests/storage_tests.rs`:

```rust
#[tokio::test]
async fn migration_creates_audit_projection_tables() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let rows = sqlx::query_scalar::<_, String>(
        "SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name",
    )
    .fetch_all(db.pool())
    .await
    .unwrap();

    assert!(rows.contains(&"order_events".to_string()));
    assert!(rows.contains(&"risk_events".to_string()));
    assert!(rows.contains(&"insights".to_string()));
    assert!(rows.contains(&"portfolio_targets".to_string()));
}
```

- [ ] **Step 2: Run the failing storage test**

Run:

```powershell
cargo test -p storage migration_creates_audit_projection_tables
```

Expected: fail because the tables do not exist.

- [ ] **Step 3: Create `migrations/0002_audit_projections.sql`**

```sql
CREATE TABLE IF NOT EXISTS order_events (
    id TEXT PRIMARY KEY,
    event_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    order_id TEXT,
    client_order_id TEXT,
    broker_order_id TEXT,
    account_id TEXT,
    symbol TEXT,
    status TEXT NOT NULL,
    event_type TEXT NOT NULL,
    message TEXT,
    ts_ms INTEGER NOT NULL,
    payload_json TEXT NOT NULL,
    FOREIGN KEY(event_id) REFERENCES event_store(event_id),
    FOREIGN KEY(run_id) REFERENCES strategy_runs(id)
);

CREATE INDEX IF NOT EXISTS idx_order_events_run_id
ON order_events(run_id);

CREATE INDEX IF NOT EXISTS idx_order_events_order_id
ON order_events(order_id);

CREATE INDEX IF NOT EXISTS idx_order_events_ts
ON order_events(ts_ms);

CREATE TABLE IF NOT EXISTS risk_events (
    id TEXT PRIMARY KEY,
    event_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    account_id TEXT,
    symbol TEXT,
    risk_type TEXT NOT NULL,
    decision TEXT NOT NULL,
    reason TEXT,
    threshold TEXT,
    observed_value TEXT,
    ts_ms INTEGER NOT NULL,
    payload_json TEXT NOT NULL,
    FOREIGN KEY(event_id) REFERENCES event_store(event_id),
    FOREIGN KEY(run_id) REFERENCES strategy_runs(id)
);

CREATE INDEX IF NOT EXISTS idx_risk_events_run_id
ON risk_events(run_id);

CREATE INDEX IF NOT EXISTS idx_risk_events_symbol
ON risk_events(symbol);

CREATE INDEX IF NOT EXISTS idx_risk_events_ts
ON risk_events(ts_ms);

CREATE TABLE IF NOT EXISTS insights (
    id TEXT PRIMARY KEY,
    event_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    strategy TEXT NOT NULL,
    symbol TEXT NOT NULL,
    side TEXT NOT NULL,
    confidence TEXT NOT NULL,
    ts_ms INTEGER NOT NULL,
    payload_json TEXT NOT NULL,
    FOREIGN KEY(event_id) REFERENCES event_store(event_id),
    FOREIGN KEY(run_id) REFERENCES strategy_runs(id)
);

CREATE INDEX IF NOT EXISTS idx_insights_run_symbol_ts
ON insights(run_id, symbol, ts_ms);

CREATE TABLE IF NOT EXISTS portfolio_targets (
    id TEXT PRIMARY KEY,
    event_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    account_id TEXT NOT NULL,
    symbol TEXT NOT NULL,
    target_qty TEXT NOT NULL,
    ts_ms INTEGER NOT NULL,
    payload_json TEXT NOT NULL,
    FOREIGN KEY(event_id) REFERENCES event_store(event_id),
    FOREIGN KEY(run_id) REFERENCES strategy_runs(id)
);

CREATE INDEX IF NOT EXISTS idx_portfolio_targets_run_symbol_ts
ON portfolio_targets(run_id, symbol, ts_ms);
```

- [ ] **Step 4: Verify migration passes**

Run:

```powershell
cargo test -p storage migration_creates_audit_projection_tables
```

Expected: pass.

- [ ] **Step 5: Add storage command and read structs**

Add to `crates/storage/src/repositories.rs` near existing event structs:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewOrderEvent {
    pub id: String,
    pub event_id: String,
    pub run_id: String,
    pub order_id: Option<String>,
    pub client_order_id: Option<String>,
    pub broker_order_id: Option<String>,
    pub account_id: Option<String>,
    pub symbol: Option<String>,
    pub status: String,
    pub event_type: String,
    pub message: Option<String>,
    pub ts_ms: i64,
    pub payload_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredOrderEvent {
    pub id: String,
    pub event_id: String,
    pub run_id: String,
    pub order_id: Option<String>,
    pub client_order_id: Option<String>,
    pub broker_order_id: Option<String>,
    pub account_id: Option<String>,
    pub symbol: Option<String>,
    pub status: String,
    pub event_type: String,
    pub message: Option<String>,
    pub ts_ms: i64,
    pub payload_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewRiskEvent {
    pub id: String,
    pub event_id: String,
    pub run_id: String,
    pub account_id: Option<String>,
    pub symbol: Option<String>,
    pub risk_type: String,
    pub decision: String,
    pub reason: Option<String>,
    pub threshold: Option<String>,
    pub observed_value: Option<String>,
    pub ts_ms: i64,
    pub payload_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredRiskEvent {
    pub id: String,
    pub event_id: String,
    pub run_id: String,
    pub account_id: Option<String>,
    pub symbol: Option<String>,
    pub risk_type: String,
    pub decision: String,
    pub reason: Option<String>,
    pub threshold: Option<String>,
    pub observed_value: Option<String>,
    pub ts_ms: i64,
    pub payload_json: String,
}
```

- [ ] **Step 6: Add repository insert/list methods**

Add methods on the existing `impl Db` in `crates/storage/src/repositories.rs`:

```rust
pub async fn insert_order_event(&self, event: NewOrderEvent) -> StorageResult<()> {
    sqlx::query(
        r#"
        INSERT INTO order_events (
            id, event_id, run_id, order_id, client_order_id, broker_order_id,
            account_id, symbol, status, event_type, message, ts_ms, payload_json
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(event.id)
    .bind(event.event_id)
    .bind(event.run_id)
    .bind(event.order_id)
    .bind(event.client_order_id)
    .bind(event.broker_order_id)
    .bind(event.account_id)
    .bind(event.symbol)
    .bind(event.status)
    .bind(event.event_type)
    .bind(event.message)
    .bind(event.ts_ms)
    .bind(event.payload_json)
    .execute(self.pool())
    .await?;
    Ok(())
}

pub async fn list_order_events(&self, run_id: &str) -> StorageResult<Vec<StoredOrderEvent>> {
    sqlx::query_as::<_, (
        String, String, String, Option<String>, Option<String>, Option<String>,
        Option<String>, Option<String>, String, String, Option<String>, i64, String,
    )>(
        r#"
        SELECT id, event_id, run_id, order_id, client_order_id, broker_order_id,
               account_id, symbol, status, event_type, message, ts_ms, payload_json
        FROM order_events
        WHERE run_id = ?
        ORDER BY ts_ms, id
        "#,
    )
    .bind(run_id)
    .fetch_all(self.pool())
    .await
    .map(|rows| {
        rows.into_iter()
            .map(|row| StoredOrderEvent {
                id: row.0,
                event_id: row.1,
                run_id: row.2,
                order_id: row.3,
                client_order_id: row.4,
                broker_order_id: row.5,
                account_id: row.6,
                symbol: row.7,
                status: row.8,
                event_type: row.9,
                message: row.10,
                ts_ms: row.11,
                payload_json: row.12,
            })
            .collect()
    })
    .map_err(Into::into)
}
```

Add matching `insert_risk_event` and `list_risk_events` with the `risk_events` columns.

- [ ] **Step 7: Add repository tests**

Add tests that:

- Insert a `strategy_runs` row.
- Insert an `event_store` row.
- Insert one `order_events` row linked to the event.
- Insert one `risk_events` row linked to the event.
- Assert list methods return typed records sorted by `ts_ms`.

Run:

```powershell
cargo test -p storage audit_projection
```

Expected: pass.

- [ ] **Step 8: Commit**

```powershell
git add migrations/0002_audit_projections.sql crates/storage/src/repositories.rs crates/storage/tests
git commit -m "feat: add audit projection storage"
```

---

## Task 3: Project Runtime Events Into Audit Tables

**Files:**

- Modify: `crates/algorithm/src/algorithm.rs`
- Modify: `crates/backtest/src/backtest.rs`
- Modify: `crates/paper/src/paper.rs`
- Modify: `crates/paper/tests/paper_tests.rs`
- Modify: `crates/backtest/tests/persistent_backtest_tests.rs`

- [ ] **Step 1: Add failing paper test for order event projection**

Add to `crates/paper/tests/paper_tests.rs`:

```rust
#[tokio::test]
async fn paper_run_persists_order_event_projection() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let settings = PaperSettings::sample();
    let bars = vec![
        Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
        Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)),
        Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)),
    ];

    PaperRuntime::new(db.clone(), settings.clone())
        .run_bars(bars)
        .await
        .unwrap();

    let events = db.list_order_events(&settings.run_id).await.unwrap();

    assert!(events.iter().any(|event| event.event_type == "broker.order.submitted"));
    assert!(events.iter().any(|event| event.status == "FILLED"));
}
```

- [ ] **Step 2: Run failing test**

Run:

```powershell
cargo test -p paper paper_run_persists_order_event_projection
```

Expected: fail because projections are not written.

- [ ] **Step 3: Add projection helper**

Create a small helper in `crates/storage/src/repositories.rs` or a new focused module if the file is already too large:

```rust
pub fn order_event_from_runtime_event(
    event_id: String,
    source: &str,
    category: &str,
    payload_json: &str,
    ts_ms: i64,
) -> Option<NewOrderEvent> {
    if !category.starts_with("broker.order.") && !category.starts_with("algorithm.oms.") {
        return None;
    }

    let payload = serde_json::from_str::<serde_json::Value>(payload_json).ok()?;
    Some(NewOrderEvent {
        id: Uuid::new_v4().to_string(),
        event_id,
        run_id: payload
            .get("run_id")
            .and_then(|value| value.as_str())
            .unwrap_or(source)
            .to_string(),
        order_id: payload.get("order_id").and_then(|value| value.as_str()).map(str::to_string),
        client_order_id: payload
            .get("client_order_id")
            .and_then(|value| value.as_str())
            .map(str::to_string),
        broker_order_id: payload
            .get("broker_order_id")
            .and_then(|value| value.as_str())
            .map(str::to_string),
        account_id: payload.get("account_id").and_then(|value| value.as_str()).map(str::to_string),
        symbol: payload.get("symbol").and_then(|value| value.as_str()).map(str::to_string),
        status: payload
            .get("status")
            .and_then(|value| value.as_str())
            .unwrap_or("UNKNOWN")
            .to_string(),
        event_type: category.to_string(),
        message: payload.get("message").and_then(|value| value.as_str()).map(str::to_string),
        ts_ms,
        payload_json: payload_json.to_string(),
    })
}
```

- [ ] **Step 4: Write projections after `event_store` insert**

In paper/backtest persistence, after each immutable event is saved to `event_store`, call:

```rust
if let Some(order_event) = storage::order_event_from_runtime_event(
    stored_event_id,
    &run_id,
    &runtime_event.category,
    &runtime_event.payload_json,
    ts_ms,
) {
    repository.insert_order_event(order_event).await?;
}
```

Do the same for `risk_event_from_runtime_event`.

- [ ] **Step 5: Run focused tests**

```powershell
cargo test -p paper paper_run_persists_order_event_projection
cargo test -p backtest persistent_backtest
cargo test -p storage audit_projection
```

Expected: pass.

- [ ] **Step 6: Commit**

```powershell
git add crates/algorithm crates/backtest crates/paper crates/storage
git commit -m "feat: project runtime audit events"
```

---

## Task 4: Add Read-Only Audit API Routes

**Files:**

- Modify: `crates/api/src/api.rs`
- Modify: `crates/api/tests/api_tests.rs`
- Modify: `docs/api.md`

- [ ] **Step 1: Add failing API test**

Add to `crates/api/tests/api_tests.rs`:

```rust
#[tokio::test]
async fn lists_order_events_for_run() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    db.insert_strategy_run(storage::NewStrategyRun {
        id: "run-a".to_string(),
        name: "sample".to_string(),
        mode: "paper".to_string(),
        status: "completed".to_string(),
        started_at_ms: 1,
        ended_at_ms: Some(2),
        error: None,
        config_json: "{}".to_string(),
    })
    .await
    .unwrap();
    db.insert_event(storage::NewEventRecord {
        event_id: "event-a".to_string(),
        ts_ms: 1,
        source: "run-a".to_string(),
        category: "broker.order.submitted".to_string(),
        payload_json: r#"{"run_id":"run-a","status":"SUBMITTED"}"#.to_string(),
    })
    .await
    .unwrap();
    db.insert_order_event(storage::NewOrderEvent {
        id: "order-event-a".to_string(),
        event_id: "event-a".to_string(),
        run_id: "run-a".to_string(),
        order_id: Some("order-a".to_string()),
        client_order_id: Some("client-a".to_string()),
        broker_order_id: None,
        account_id: Some("paper".to_string()),
        symbol: Some("US:NASDAQ:AAPL:EQUITY".to_string()),
        status: "SUBMITTED".to_string(),
        event_type: "broker.order.submitted".to_string(),
        message: None,
        ts_ms: 1,
        payload_json: r#"{"run_id":"run-a","status":"SUBMITTED"}"#.to_string(),
    })
    .await
    .unwrap();
    let app = api::router_with_state(api::AppState::new(
        db,
        "configs/backtest/ma_cross.toml".into(),
    ));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/runs/run-a/order-events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();

    assert!(body.contains("\"run_id\":\"run-a\""));
    assert!(body.contains("\"event_type\":\"broker.order.submitted\""));
}
```

- [ ] **Step 2: Add API response structs**

Add to `crates/api/src/api.rs`:

```rust
#[derive(Serialize)]
struct OrderEventResponse {
    id: String,
    event_id: String,
    run_id: String,
    order_id: Option<String>,
    client_order_id: Option<String>,
    broker_order_id: Option<String>,
    account_id: Option<String>,
    symbol: Option<String>,
    status: String,
    event_type: String,
    message: Option<String>,
    ts_ms: i64,
    payload: serde_json::Value,
}

#[derive(Serialize)]
struct RiskEventResponse {
    id: String,
    event_id: String,
    run_id: String,
    account_id: Option<String>,
    symbol: Option<String>,
    risk_type: String,
    decision: String,
    reason: Option<String>,
    threshold: Option<String>,
    observed_value: Option<String>,
    ts_ms: i64,
    payload: serde_json::Value,
}
```

- [ ] **Step 3: Add routes**

Add to `router_with_state`:

```rust
.route("/api/v1/runs/{run_id}/order-events", get(list_run_order_events))
.route("/api/v1/runs/{run_id}/risk-events", get(list_run_risk_events))
```

- [ ] **Step 4: Implement handlers**

```rust
async fn list_run_order_events(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<Json<Vec<OrderEventResponse>>, ApiError> {
    let rows = state.db.list_order_events(&run_id).await?;
    let responses = rows
        .into_iter()
        .map(|row| {
            let payload = serde_json::from_str(&row.payload_json).unwrap_or(serde_json::Value::Null);
            OrderEventResponse {
                id: row.id,
                event_id: row.event_id,
                run_id: row.run_id,
                order_id: row.order_id,
                client_order_id: row.client_order_id,
                broker_order_id: row.broker_order_id,
                account_id: row.account_id,
                symbol: row.symbol,
                status: row.status,
                event_type: row.event_type,
                message: row.message,
                ts_ms: row.ts_ms,
                payload,
            }
        })
        .collect();
    Ok(Json(responses))
}
```

Add matching `list_run_risk_events`.

- [ ] **Step 5: Document routes**

Add to `docs/api.md`:

```markdown
GET /api/v1/runs/{run_id}/order-events
GET /api/v1/runs/{run_id}/risk-events

These are read-only audit projection routes. They do not replace `event_store`; they expose structured query views derived from runtime events.
```

- [ ] **Step 6: Run tests**

```powershell
cargo test -p api order_events
cargo test -p api risk_events
bash ./scripts/check-api-read-model-boundary
```

Expected: pass.

- [ ] **Step 7: Commit**

```powershell
git add crates/api docs/api.md
git commit -m "feat: expose audit projection queries"
```

---

## Task 5: Add Market Rule Reference Tables

**Files:**

- Create: `migrations/0003_market_rules.sql`
- Modify: `crates/storage/src/repositories.rs`
- Modify: `crates/storage/tests/storage_tests.rs`
- Modify: `crates/market_rules/src/market_rules.rs`
- Modify: `crates/market_rules/tests/market_rules_tests.rs`
- Modify: `docs/database.md`

- [ ] **Step 1: Create migration**

```sql
CREATE TABLE IF NOT EXISTS market_calendars (
    id TEXT PRIMARY KEY,
    market TEXT NOT NULL,
    trading_day TEXT NOT NULL,
    is_open INTEGER NOT NULL,
    session_template TEXT,
    UNIQUE(market, trading_day)
);

CREATE TABLE IF NOT EXISTS trading_sessions (
    id TEXT PRIMARY KEY,
    market TEXT NOT NULL,
    trading_day TEXT NOT NULL,
    session_name TEXT NOT NULL,
    open_time TEXT NOT NULL,
    close_time TEXT NOT NULL,
    timezone TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS fee_rules (
    id TEXT PRIMARY KEY,
    market TEXT NOT NULL,
    exchange TEXT NOT NULL,
    asset_class TEXT NOT NULL,
    maker_bps TEXT NOT NULL,
    taker_bps TEXT NOT NULL,
    effective_from_ms INTEGER NOT NULL,
    effective_to_ms INTEGER
);

CREATE TABLE IF NOT EXISTS lot_size_rules (
    id TEXT PRIMARY KEY,
    market TEXT NOT NULL,
    exchange TEXT NOT NULL,
    asset_class TEXT NOT NULL,
    symbol TEXT,
    lot_size TEXT NOT NULL,
    min_qty TEXT NOT NULL,
    min_notional TEXT NOT NULL,
    effective_from_ms INTEGER NOT NULL,
    effective_to_ms INTEGER
);

CREATE TABLE IF NOT EXISTS price_limit_rules (
    id TEXT PRIMARY KEY,
    market TEXT NOT NULL,
    exchange TEXT NOT NULL,
    asset_class TEXT NOT NULL,
    symbol TEXT,
    tick_size TEXT NOT NULL,
    limit_up_bps TEXT,
    limit_down_bps TEXT,
    effective_from_ms INTEGER NOT NULL,
    effective_to_ms INTEGER
);
```

- [ ] **Step 2: Add repository read models**

Add storage structs:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredLotSizeRule {
    pub market: String,
    pub exchange: String,
    pub asset_class: String,
    pub symbol: Option<String>,
    pub lot_size: String,
    pub min_qty: String,
    pub min_notional: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredPriceLimitRule {
    pub market: String,
    pub exchange: String,
    pub asset_class: String,
    pub symbol: Option<String>,
    pub tick_size: String,
    pub limit_up_bps: Option<String>,
    pub limit_down_bps: Option<String>,
}
```

Add insert/list methods with exact SQL ordered by most specific `symbol` first.

- [ ] **Step 3: Add market rule provider trait**

In `crates/market_rules/src/market_rules.rs`:

```rust
pub trait MarketRuleProvider {
    fn rules_for_symbol(&self, symbol: &str) -> Result<MarketRuleSet, MarketRuleError>;
}

pub struct StaticMarketRuleProvider;

impl MarketRuleProvider for StaticMarketRuleProvider {
    fn rules_for_symbol(&self, symbol: &str) -> Result<MarketRuleSet, MarketRuleError> {
        MarketRuleSet::for_symbol(symbol)
    }
}
```

Do not remove `MarketRuleSet::for_symbol`; existing callers keep working.

- [ ] **Step 4: Add tests**

Run:

```powershell
cargo test -p storage market_rule
cargo test -p market_rules
```

Expected: pass.

- [ ] **Step 5: Commit**

```powershell
git add migrations/0003_market_rules.sql crates/storage crates/market_rules docs/database.md
git commit -m "feat: add market rule reference schema"
```

---

## Task 6: Add Insight and Portfolio Target Persistence

**Files:**

- Modify: `crates/algorithm/src/algorithm.rs`
- Modify: `crates/storage/src/repositories.rs`
- Modify: `crates/backtest/src/backtest.rs`
- Modify: `crates/paper/src/paper.rs`
- Modify: `crates/backtest/tests/persistent_backtest_tests.rs`
- Modify: `crates/paper/tests/persistent_paper_tests.rs`

- [ ] **Step 1: Add persistent backtest test**

Add assertion to persistent backtest test:

```rust
let insights = db.list_insights(&settings.run_id).await.unwrap();
assert!(!insights.is_empty());
assert_eq!(insights[0].run_id, settings.run_id);

let targets = db.list_portfolio_targets(&settings.run_id).await.unwrap();
assert!(!targets.is_empty());
assert_eq!(targets[0].run_id, settings.run_id);
```

- [ ] **Step 2: Implement `insert_insight`, `list_insights`, `insert_portfolio_target`, `list_portfolio_targets`**

Use Decimal strings for `confidence` and `target_qty`.

- [ ] **Step 3: Persist from Algorithm events**

Map:

- `algorithm.alpha.generated` -> `insights`
- `algorithm.portfolio.target` -> `portfolio_targets`

Keep `event_store` insert first.

- [ ] **Step 4: Run focused tests**

```powershell
cargo test -p backtest persistent_backtest
cargo test -p paper persistent_paper
cargo test -p storage insight
```

Expected: pass.

- [ ] **Step 5: Commit**

```powershell
git add crates/algorithm crates/storage crates/backtest crates/paper
git commit -m "feat: persist strategy decisions"
```

---

## Task 7: Start Contract Accounting Schema Without Claiming Full Derivatives Support

**Files:**

- Create: `migrations/0004_contract_accounting.sql`
- Modify: `crates/storage/src/repositories.rs`
- Modify: `crates/storage/tests/storage_tests.rs`
- Modify: `docs/database.md`
- Modify: `docs/分析.md`

- [ ] **Step 1: Add contract accounting migration**

```sql
CREATE TABLE IF NOT EXISTS crypto_positions (
    run_id TEXT NOT NULL,
    account_id TEXT NOT NULL,
    exchange TEXT NOT NULL,
    symbol TEXT NOT NULL,
    asset_class TEXT NOT NULL,
    margin_mode TEXT NOT NULL,
    position_side TEXT NOT NULL,
    leverage TEXT NOT NULL,
    qty TEXT NOT NULL,
    avg_price TEXT NOT NULL,
    margin_used TEXT NOT NULL,
    funding_fee TEXT NOT NULL DEFAULT '0',
    realized_pnl TEXT NOT NULL DEFAULT '0',
    unrealized_pnl TEXT NOT NULL DEFAULT '0',
    updated_at_ms INTEGER NOT NULL,
    PRIMARY KEY (run_id, account_id, exchange, symbol, position_side),
    FOREIGN KEY(run_id) REFERENCES strategy_runs(id)
);

CREATE TABLE IF NOT EXISTS funding_rates (
    id TEXT PRIMARY KEY,
    exchange TEXT NOT NULL,
    symbol TEXT NOT NULL,
    funding_time_ms INTEGER NOT NULL,
    funding_rate TEXT NOT NULL,
    mark_price TEXT,
    source TEXT NOT NULL,
    UNIQUE(exchange, symbol, funding_time_ms)
);

CREATE INDEX IF NOT EXISTS idx_funding_rates_symbol_time
ON funding_rates(exchange, symbol, funding_time_ms);
```

- [ ] **Step 2: Add storage models**

Add `NewCryptoPosition`, `StoredCryptoPosition`, `NewFundingRate`, `StoredFundingRate`.

All numeric fields are strings at storage boundary.

- [ ] **Step 3: Add repository tests**

Tests must assert:

- Insert/update crypto position preserves `position_side`.
- Insert duplicate funding rate for same exchange/symbol/time upserts or fails deterministically.
- Decimal strings round-trip exactly.

- [ ] **Step 4: Do not wire runtime accounting yet**

This task only creates schema and repository boundaries. Do not change `AccountingBook` to write these tables yet. That deserves a separate ledger task with broker-specific test cases.

- [ ] **Step 5: Update docs**

In `docs/分析.md`, change the contract section from "not implemented" to:

```markdown
The storage boundary exists for contract positions and funding rates, but runtime accounting is not yet wired. The system still must not claim full derivative accounting until funding settlement and broker reconciliation write these records.
```

- [ ] **Step 6: Run tests**

```powershell
cargo test -p storage crypto_position
cargo test -p storage funding_rate
```

Expected: pass.

- [ ] **Step 7: Commit**

```powershell
git add migrations/0004_contract_accounting.sql crates/storage docs/database.md docs/分析.md
git commit -m "feat: add contract accounting storage boundary"
```

---

## Task 8: Add Schema Gap Verification Script

**Files:**

- Create: `scripts/schema-gap-check.ps1`
- Modify: `scripts/verify.ps1`
- Modify: `docs/分析.md`

- [ ] **Step 1: Create `scripts/schema-gap-check.ps1`**

```powershell
$ErrorActionPreference = "Stop"

$migrationFiles = Get-ChildItem -LiteralPath "migrations" -Filter "*.sql" | Sort-Object Name
$implemented = New-Object System.Collections.Generic.HashSet[string]

foreach ($file in $migrationFiles) {
    $content = Get-Content -LiteralPath $file.FullName -Raw
    $matches = [regex]::Matches($content, "CREATE TABLE IF NOT EXISTS\s+([a-zA-Z0-9_]+)")
    foreach ($match in $matches) {
        [void]$implemented.Add($match.Groups[1].Value)
    }
}

$target = @(
    "strategy_runs",
    "instruments",
    "market_calendars",
    "trading_sessions",
    "fee_rules",
    "lot_size_rules",
    "price_limit_rules",
    "crypto_market_meta",
    "funding_rates",
    "corporate_actions_meta",
    "orders",
    "order_events",
    "fills",
    "positions",
    "crypto_positions",
    "account_balances",
    "cash_snapshots",
    "position_snapshots",
    "portfolio_snapshots",
    "risk_events",
    "insights",
    "portfolio_targets",
    "configs",
    "system_logs"
)

$missing = @($target | Where-Object { -not $implemented.Contains($_) })

Write-Host "Implemented tables: $($implemented.Count)"
Write-Host "Target tables: $($target.Count)"
if ($missing.Count -gt 0) {
    Write-Host "Missing target tables:"
    foreach ($table in $missing) {
        Write-Host "  - $table"
    }
} else {
    Write-Host "All target tables are implemented."
}
```

- [ ] **Step 2: Run script**

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\schema-gap-check.ps1
```

Expected: prints implemented and missing target tables. It must not fail just because target schema is incomplete.

- [ ] **Step 3: Add to `scripts/verify.ps1`**

Call schema-gap-check near the docs/schema verification section:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\schema-gap-check.ps1
```

- [ ] **Step 4: Commit**

```powershell
git add scripts/schema-gap-check.ps1 scripts/verify.ps1 docs/分析.md
git commit -m "chore: add schema gap verification"
```

---

## Task 9: Final Verification

**Files:**

- No code changes expected.

- [ ] **Step 1: Run storage and boundary tests**

```powershell
cargo test -p storage
bash ./scripts/check-db-boundary
bash ./scripts/check-storage-dto-boundary
bash ./scripts/check-api-read-model-boundary
```

Expected: all pass.

- [ ] **Step 2: Run impacted crate tests**

```powershell
cargo test -p algorithm
cargo test -p backtest
cargo test -p paper
cargo test -p api
cargo test -p market_rules
```

Expected: all pass.

- [ ] **Step 3: Run local smoke**

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\v1-smoke.ps1
```

Expected: validates CLI, REST, WebSocket, SQLite, Parquet, Replay control, reports, fake broker/live surface.

- [ ] **Step 4: Run schema gap script**

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\schema-gap-check.ps1
```

Expected: any remaining missing tables are intentional and documented in `docs/分析.md` / `docs/roadmap.md`.

- [ ] **Step 5: Commit final docs**

```powershell
git add docs scripts
git commit -m "docs: document production gap closure status"
```

---

## Implementation Order

Recommended order:

1. Task 1: docs truth alignment.
2. Task 2: audit projection schema.
3. Task 3: event-to-projection writes.
4. Task 4: read-only audit APIs.
5. Task 5: market-rule reference schema.
6. Task 6: insight and portfolio target persistence.
7. Task 7: contract accounting storage boundary.
8. Task 8: schema gap script.
9. Task 9: final verification.

Do not start Task 7 before Task 2 and Task 3 are complete. Contract accounting without strong audit projection will make reconciliation harder.

## Risks and Controls

- **Risk:** Projection tables become a second source of truth.
  - **Control:** Always write `event_store` first; projections reference `event_id`.
- **Risk:** Decimal precision loss at external SDK boundaries.
  - **Control:** Keep `Decimal` in domain/storage APIs; convert to f64 only inside broker adapters.
- **Risk:** Adding market-rule tables breaks current static rules.
  - **Control:** Keep `MarketRuleSet::for_symbol` and add provider interface as optional path.
- **Risk:** Contract schema creates false confidence.
  - **Control:** Do not wire runtime accounting until funding settlement and broker reconciliation tests exist.
- **Risk:** API leaks storage DTOs.
  - **Control:** Add API-owned response structs and run `check-api-read-model-boundary`.

## Success Criteria

The project is materially improved when:

- Current vs target schema is documented without ambiguity.
- Orders and risk decisions can be queried structurally by run id.
- Strategy insights and portfolio targets persist for post-run analysis.
- Market rules have a storage boundary for future versioned configuration.
- Contract position/funding storage exists, but docs still avoid claiming full derivatives support.
- Existing MVP smoke still passes.
- Boundary scripts still enforce storage/API separation.
