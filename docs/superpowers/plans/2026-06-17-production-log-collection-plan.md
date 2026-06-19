# Production Log Collection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expand `system_logs` from API lifecycle logging to full-chain structured logging with correlation IDs, categories, levels, and query capabilities.

**Architecture:** Integrate with the `tracing` crate (Rust ecosystem standard) to capture structured logs across all crates. Logs are written to `system_logs` table via an async buffered writer that doesn't block the hot path. Each log entry includes correlation IDs (run_id, order_id, event_id) for cross-referencing. CLI and API provide query access.

**Tech Stack:** Rust workspace, tracing, tracing-subscriber, SQLx SQLite, Axum, tokio, serde, PowerShell CLI.

## Current Status (2026-06-19 Audit)

This plan was only partially implemented. The repository has an operational `system_logs` read/write surface, not the full async tracing collection architecture described below. Checked items below only cover exact pieces that landed; local-MVP readback that does not match the tracing plan is summarized in the status table.

| Area | Status | Evidence | Remaining |
| --- | --- | --- | --- |
| System log storage | Done for local MVP | `SystemLogCommand`, `SystemLogFilter`, `record_system_log`, `list_system_logs_filtered`, `purge_system_logs` | Batch insert/count/text search are not implemented |
| Runtime/API/ingestion log writes | Done for local MVP | API run lifecycle logs, live runtime source logs and ingestion tracker write `system_logs` | Full-chain logging across all crates is not implemented |
| CLI/API readback | Done for local MVP | `logs list`, `logs purge`, `GET /api/v1/system-logs`, `GET /api/v1/runs/{run_id}/system-logs` support run/level/target/time/limit filters; `ops-smoke.ps1` verifies run-scoped logs alongside snapshots, reconciliation and config-version readback | Tail/count/search endpoints are not implemented |
| Retention | Partially done | CLI purge supports retention-style cleanup by timestamp/target/run | Configured scheduled retention cleanup is not implemented |
| Async tracing writer | Not done | No `events::log_writer`, `SystemLogLayer`, buffered channel writer or batch flush implementation exists | Implement tracing layer and non-blocking buffered DB writer |
| External production collection | Not done | Docs classify external production log collectors and alert routing as follow-up | Add collector/shipper integration and alert routing |

---

## Scope

In scope:

- Structured log levels: TRACE, DEBUG, INFO, WARN, ERROR, FATAL.
- Log categories: system, trading, risk, data, api, broker, ingestion.
- Correlation IDs: link logs to run_id, order_id, event_id, config_name.
- Async buffered writes to system_logs (don't block hot path).
- Log rotation and retention policy (configurable).
- CLI commands for log query/filter.
- API endpoints for log search.
- Integration with `tracing` crate.

Out of scope:

- External log aggregation (ELK, Datadog, etc.).
- Log shipping to remote services.
- Real-time log streaming (polling model).
- Log encryption.
- Per-field access control on logs.

## File Map

### Logging Infrastructure

- Create: `crates/events/src/log_writer.rs`
  - Async buffered log writer that batches inserts to `system_logs`.
  - `tracing_subscriber::Layer` implementation that captures structured fields.
- Modify: `crates/events/Cargo.toml`
  - Add `tracing`, `tracing-subscriber` dependencies.
- Modify: `crates/events/src/event.rs`
  - Export log writer types.

### Storage

- Modify: `crates/storage/src/repositories.rs`
  - Add `insert_system_log` with full field set.
  - Add `list_system_logs` with rich filtering (level, category, run_id, time range, text search).
  - Add `count_system_logs` for pagination.
  - Add `cleanup_old_logs(before_ms)` for retention.
- Modify: `crates/storage/tests/storage_tests.rs`
  - Add system_logs insert/query tests.
  - Add retention cleanup test.

### Application Integration

- Modify: `crates/paper/src/paper.rs`
  - Initialize tracing subscriber with log writer at paper run start.
  - Add correlation IDs (run_id) to tracing spans.
- Modify: `crates/backtest/src/backtest.rs`
  - Same tracing initialization.
- Modify: `crates/runtime/src/runtime.rs`
  - Same for live runtime.
- Modify: `crates/algorithm/src/algorithm.rs`
  - Add structured logging at key decision points (alpha signal, portfolio target, risk check, order submission).
- Modify: `crates/api/src/api.rs`
  - Add request-scoped tracing span with request_id.
  - Log API request/response with status code and duration.

### CLI

- Modify: `apps/trader-cli/src/main.rs`
  - Add `logs list` command with filters.
  - Add `logs tail` command (poll for new logs).

### API

- Modify: `crates/api/src/api.rs`
  - Add `GET /api/v1/logs` with query parameters.
- Modify: `crates/api/tests/api_tests.rs`
  - Add log query tests.
- Modify: `docs/api.md`
  - Document log endpoints.

### Configuration

- Modify: `crates/config/src/config.rs`
  - Add logging config: level, categories, retention_days, buffer_size.
- Modify: configs/*.toml
  - Add logging configuration examples.

### Documentation

- Modify: `docs/分析.md`
- Modify: `docs/roadmap.md`

---

## Acceptance Gates

Every task must preserve:

- `cargo test -p storage`
- `cargo test -p events`
- `cargo test -p paper`
- `cargo test -p backtest`
- `cargo test -p algorithm`
- `cargo test -p api`
- `powershell -ExecutionPolicy Bypass -File .\scripts\v1-smoke.ps1`
- `bash ./scripts/check-db-boundary`
- `bash ./scripts/check-storage-dto-boundary`
- `bash ./scripts/check-api-read-model-boundary`

New gates:

- `cargo test -p events log_writer` — async buffered writer.
- `cargo test -p storage system_log` — insert/query/retention.
- `cargo test -p paper paper_structured_logging` — logs captured during paper run.

---

## Task 1: Extend Storage for System Logs

**Files:**

- Modify: `crates/storage/src/repositories.rs`
- Modify: `crates/storage/tests/storage_tests.rs`

- [x] **Step 1: Define system log types**

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewSystemLog {
    pub id: String,
    pub ts_ms: i64,
    pub level: LogLevel,
    pub category: LogCategory,
    pub message: String,
    pub run_id: Option<String>,
    pub order_id: Option<String>,
    pub event_id: Option<String>,
    pub config_name: Option<String>,
    pub source_file: Option<String>,
    pub source_line: Option<u32>,
    pub fields_json: Option<String>,  // additional structured fields
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum LogCategory {
    System,
    Trading,
    Risk,
    Data,
    Api,
    Broker,
    Ingestion,
    Config,
}
```

- [x] **Step 2: Add insert and query methods**

```rust
pub async fn insert_system_log(&self, log: &NewSystemLog) -> StorageResult<()>
pub async fn list_system_logs(&self, filter: &SystemLogFilter) -> StorageResult<Vec<StoredSystemLog>>
pub async fn count_system_logs(&self, filter: &SystemLogFilter) -> StorageResult<u64>
pub async fn cleanup_old_logs(&self, before_ms: i64) -> StorageResult<u64>
```

```rust
pub struct SystemLogFilter {
    pub levels: Option<Vec<LogLevel>>,
    pub categories: Option<Vec<LogCategory>>,
    pub run_id: Option<String>,
    pub order_id: Option<String>,
    pub from_ms: Option<i64>,
    pub to_ms: Option<i64>,
    pub text_search: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}
```

- [ ] **Step 3: Add batch insert for performance**

```rust
pub async fn insert_system_logs_batch(&self, logs: &[NewSystemLog]) -> StorageResult<()> {
    // Use a single transaction for batch insert
    // Much faster than individual inserts for high-volume logging
}
```

- [ ] **Step 4: Add storage tests**

```rust
#[tokio::test]
async fn system_log_insert_and_query() {
    // Insert logs with different levels and categories
    // Query with level filter
    // Assert: correct filtering
}

#[tokio::test]
async fn system_log_text_search() {
    // Insert logs with various messages
    // Search for keyword
    // Assert: matching logs returned
}

#[tokio::test]
async fn system_log_cleanup_retention() {
    // Insert logs at different timestamps
    // Cleanup logs older than threshold
    // Assert: only old logs deleted
}

#[tokio::test]
async fn system_log_batch_insert() {
    // Insert 100 logs in batch
    // Assert: all 100 retrievable
}
```

- [ ] **Step 5: Run storage tests**

```powershell
cargo test -p storage system_log
```

Expected: pass.

- [ ] **Step 6: Commit**

```powershell
git add crates/storage
git commit -m "feat: extend system_logs storage for structured logging"
```

---

## Task 2: Implement Async Buffered Log Writer

**Files:**

- Create: `crates/events/src/log_writer.rs`
- Modify: `crates/events/src/event.rs`
- Modify: `crates/events/Cargo.toml`
- Modify: `crates/events/tests/log_writer_tests.rs`

- [ ] **Step 1: Add dependencies**

```toml
# crates/events/Cargo.toml
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tokio = { version = "1", features = ["sync"] }
```

- [ ] **Step 2: Implement LogWriter**

```rust
pub struct LogWriter {
    tx: tokio::sync::mpsc::Sender<NewSystemLog>,
    _handle: tokio::task::JoinHandle<()>,
}

impl LogWriter {
    pub fn new(db: Db, buffer_size: usize, flush_interval_ms: u64) -> Self {
        let (tx, rx) = tokio::sync::mpsc::channel(buffer_size);
        let handle = tokio::spawn(async move {
            Self::write_loop(db, rx, flush_interval_ms).await;
        });
        Self { tx, _handle: handle }
    }

    pub fn sender(&self) -> &tokio::sync::mpsc::Sender<NewSystemLog> {
        &self.tx
    }

    async fn write_loop(db: Db, mut rx: tokio::sync::mpsc::Receiver<NewSystemLog>, flush_interval_ms: u64) {
        let mut buffer = Vec::new();
        let mut interval = tokio::time::interval(Duration::from_millis(flush_interval_ms));

        loop {
            tokio::select! {
                Some(log) = rx.recv() => {
                    buffer.push(log);
                    if buffer.len() >= 100 {
                        let _ = db.insert_system_logs_batch(&buffer).await;
                        buffer.clear();
                    }
                }
                _ = interval.tick() => {
                    if !buffer.is_empty() {
                        let _ = db.insert_system_logs_batch(&buffer).await;
                        buffer.clear();
                    }
                }
                else => break,
            }
        }
        // Flush remaining
        if !buffer.is_empty() {
            let _ = db.insert_system_logs_batch(&buffer).await;
        }
    }
}
```

- [ ] **Step 3: Implement tracing Layer**

```rust
pub struct SystemLogLayer {
    tx: tokio::sync::mpsc::Sender<NewSystemLog>,
    run_id: Option<String>,
}

impl tracing_subscriber::Layer for SystemLogLayer {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        // Extract level, message, fields from tracing event
        // Map tracing level to LogLevel
        // Extract correlation IDs from span extensions
        // Create NewSystemLog
        // Try send to channel (non-blocking)
    }
}
```

- [ ] **Step 4: Add tests**

```rust
#[tokio::test]
async fn log_writer_flushes_on_interval() {
    // Create writer with short flush interval
    // Send 5 logs
    // Wait for flush
    // Assert: all 5 logs in database
}

#[tokio::test]
async fn log_writer_flushes_on_buffer_full() {
    // Create writer with buffer_size=10
    // Send 15 logs
    // Assert: at least 10 logs flushed immediately
}

#[tokio::test]
async fn tracing_layer_captures_events() {
    // Initialize tracing with SystemLogLayer
    // Emit tracing::info!, tracing::error!
    // Assert: logs captured in database
}
```

- [ ] **Step 5: Run tests**

```powershell
cargo test -p events log_writer
```

Expected: pass.

- [ ] **Step 6: Commit**

```powershell
git add crates/events
git commit -m "feat: async buffered log writer with tracing integration"
```

---

## Task 3: Wire Logging into Runtime

**Files:**

- Modify: `crates/paper/src/paper.rs`
- Modify: `crates/backtest/src/backtest.rs`
- Modify: `crates/runtime/src/runtime.rs`
- Modify: `crates/algorithm/src/algorithm.rs`

- [ ] **Step 1: Initialize logging in paper runtime**

```rust
// At paper run start:
let log_writer = LogWriter::new(db.clone(), config.log_buffer_size, config.log_flush_interval_ms);
let log_layer = SystemLogLayer::new(log_writer.sender().clone(), Some(run_id.clone()));

tracing_subscriber::registry()
    .with(log_layer)
    .with(tracing_subscriber::fmt::layer())  // Also keep console output
    .init();

tracing::info!(run_id = %run_id, mode = "paper", "Paper run started");
```

- [ ] **Step 2: Add structured logging to algorithm**

```rust
// In algorithm execution:
tracing::info!(
    run_id = %self.run_id,
    symbol = %signal.symbol,
    side = ?signal.side,
    confidence = %signal.confidence,
    "Alpha signal generated"
);

tracing::info!(
    run_id = %self.run_id,
    symbol = %target.symbol,
    target_qty = %target.qty,
    "Portfolio target computed"
);

tracing::warn!(
    run_id = %self.run_id,
    symbol = %order.symbol,
    risk_type = ?rejection.risk_type,
    reason = %rejection.reason,
    "Order rejected by risk check"
);
```

- [ ] **Step 3: Add structured logging to paper/backtest**

```rust
tracing::info!(
    run_id = %self.run_id,
    order_id = %order.id,
    symbol = %order.symbol,
    side = ?order.side,
    qty = %order.qty,
    price = ?order.price,
    "Order submitted"
);

tracing::info!(
    run_id = %self.run_id,
    order_id = %fill.order_id,
    fill_price = %fill.price,
    fill_qty = %fill.qty,
    "Order filled"
);
```

- [ ] **Step 4: Add structured logging to API**

```rust
// In API middleware:
let request_id = Uuid::new_v4().to_string();
let span = tracing::info_span!("api_request", request_id = %request_id, method = %req.method(), path = %req.uri().path());
let _guard = span.enter();

// After response:
tracing::info!(
    status = %response.status().as_u16(),
    duration_ms = %duration.as_millis(),
    "API request completed"
);
```

- [ ] **Step 5: Add paper test**

```rust
#[tokio::test]
async fn paper_run_captures_structured_logs() {
    // Run paper with logging enabled
    // Query system_logs for this run_id
    // Assert: logs exist with correct categories (trading, system)
    // Assert: logs have correlation IDs
}
```

- [ ] **Step 6: Run tests**

```powershell
cargo test -p paper paper_structured_logging
cargo test -p algorithm
cargo test -p backtest
```

Expected: pass.

- [ ] **Step 7: Commit**

```powershell
git add crates/paper crates/backtest crates/runtime crates/algorithm crates/api
git commit -m "feat: wire structured logging into runtime"
```

---

## Task 4: Add Log CLI Commands

**Files:**

- Modify: `apps/trader-cli/src/main.rs`

- [ ] **Step 1: Add CLI commands**

```
trader logs list [--level <level>] [--category <cat>] [--run-id <id>] [--from <ts>] [--to <ts>] [--search <text>] [--limit <n>]
trader logs tail [--level <level>] [--category <cat>] [--run-id <id>]
trader logs count [--level <level>] [--category <cat>] [--run-id <id>]
trader logs cleanup --before <ts>
```

- [ ] **Step 2: Implement commands**

Each command calls the storage repository methods with appropriate filters.

- [ ] **Step 3: Add CLI tests**

```rust
#[test]
fn logs_list_with_level_filter() { ... }
#[test]
fn logs_count_by_category() { ... }
```

- [ ] **Step 4: Commit**

```powershell
git add apps/trader-cli
git commit -m "feat: log query CLI commands"
```

---

## Task 5: Add Log API Endpoints

**Files:**

- Modify: `crates/api/src/api.rs`
- Modify: `crates/api/tests/api_tests.rs`
- Modify: `docs/api.md`

- [ ] **Step 1: Add API endpoint**

```
GET /api/v1/logs?level={level}&category={cat}&run_id={id}&from_ms={t1}&to_ms={t2}&search={text}&limit={n}&offset={n}
```

Response:
```json
{
  "logs": [
    {
      "id": "...",
      "ts_ms": 1234567890,
      "level": "INFO",
      "category": "trading",
      "message": "Order submitted",
      "run_id": "...",
      "order_id": "...",
      "fields": { "symbol": "AAPL", "qty": "100" }
    }
  ],
  "total": 1500,
  "limit": 100,
  "offset": 0
}
```

- [ ] **Step 2: Add API response struct**

```rust
#[derive(Serialize)]
struct LogResponse {
    logs: Vec<LogEntryResponse>,
    total: u64,
    limit: u32,
    offset: u32,
}

#[derive(Serialize)]
struct LogEntryResponse {
    id: String,
    ts_ms: i64,
    level: String,
    category: String,
    message: String,
    run_id: Option<String>,
    order_id: Option<String>,
    event_id: Option<String>,
    fields: Option<serde_json::Value>,
}
```

- [ ] **Step 3: Add tests and docs**

- API test for log query endpoint.
- `docs/api.md` documentation.

- [ ] **Step 4: Run full acceptance**

```powershell
cargo test -p api logs
cargo test -p events log_writer
cargo test -p paper
powershell -ExecutionPolicy Bypass -File .\scripts\v1-smoke.ps1
bash ./scripts/check-api-read-model-boundary
```

Expected: all pass.

- [ ] **Step 5: Commit**

```powershell
git add crates/api docs/api.md
git commit -m "feat: log query API endpoint"
```

---

## Task 6: Add Configuration and Retention

**Files:**

- Modify: `crates/config/src/config.rs`
- Modify: configs/*.toml

- [ ] **Step 1: Add logging config**

```rust
pub struct LoggingConfig {
    pub enabled: bool,                    // default true
    pub level: LogLevel,                  // default Info
    pub categories: Vec<LogCategory>,     // default all
    pub buffer_size: usize,               // default 1000
    pub flush_interval_ms: u64,           // default 5000
    pub retention_days: u32,              // default 30
    pub console_output: bool,             // default true
}
```

- [ ] **Step 2: Add config examples**

```toml
[logging]
enabled = true
level = "info"
buffer_size = 1000
flush_interval_ms = 5000
retention_days = 30
console_output = true
```

- [ ] **Step 3: Add retention cleanup to scheduled tasks**

If a scheduler exists (e.g., from ingestion), add log cleanup task:
```rust
// Run daily:
let before_ms = now_ms() - (config.retention_days as i64 * 86400 * 1000);
db.cleanup_old_logs(before_ms).await?;
```

- [ ] **Step 4: Commit**

```powershell
git add crates/config configs
git commit -m "feat: logging configuration and retention"
```

---

## Task 7: Update Documentation

**Files:**

- Modify: `docs/分析.md`
- Modify: `docs/roadmap.md`

- [ ] **Step 1: Update `docs/分析.md`**

Update system_logs section from "API lifecycle logging" to "full-chain structured logging".

- [ ] **Step 2: Update `docs/roadmap.md`**

Add "Production Log Collection" milestone.

- [ ] **Step 3: Commit**

```powershell
git add docs
git commit -m "docs: update production log collection status"
```

---

## Implementation Order

1. Task 1: Storage extensions.
2. Task 2: Async buffered log writer.
3. Task 3: Wire into runtime.
4. Task 4: CLI commands.
5. Task 5: API endpoints.
6. Task 6: Configuration and retention.
7. Task 7: Documentation.

## Risks and Controls

- **Risk:** High-volume logging blocks hot path.
  - **Control:** Async buffered writer with channel. Non-blocking send. Batch inserts. Configurable buffer size and flush interval.
- **Risk:** Log storage grows unbounded.
  - **Control:** Configurable retention policy. CLI cleanup command. Default 30 days.
- **Risk:** Tracing integration adds complexity to initialization.
  - **Control:** Logging is optional (config.enabled). Fallback to console-only if writer fails.
- **Risk:** Log query performance degrades with large datasets.
  - **Control:** Index on (ts_ms, level, category, run_id). Pagination with limit/offset. Count query separate from data query.
- **Risk:** Correlation IDs not propagated across crate boundaries.
  - **Control:** Use tracing spans with run_id/order_id. Spans propagate automatically across async boundaries.

## Success Criteria

The project is materially improved when:

- All crates emit structured logs via `tracing`.
- Logs are captured in `system_logs` with correlation IDs.
- Log writer doesn't block the hot path (< 1ms overhead per log).
- CLI provides log query and tail commands.
- API provides log search endpoint.
- Retention policy automatically cleans old logs.
- Existing MVP smoke still passes.
