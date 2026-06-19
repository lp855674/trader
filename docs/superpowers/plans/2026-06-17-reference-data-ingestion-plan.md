# Reference-Data Ingestion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Auto-fetch `crypto_market_meta` and `corporate_actions_meta` from exchanges and data providers, replacing manual population with scheduled or CLI-triggered ingestion jobs.

**Architecture:** Add an ingestion layer in `crates/data` that fetches external reference data and upserts into storage. Use idempotent upserts so re-running ingestion is safe. Track ingestion metadata (last fetch time, change count) for monitoring. Keep `event_store` as the audit truth for ingestion events.

**Tech Stack:** Rust workspace, reqwest/ureq for HTTP, SQLx SQLite, serde, serde_json, chrono, PowerShell CLI.

## Current Status (2026-06-19 Audit)

This plan is backfilled and the local MVP scope is complete. The remaining work is production hardening beyond the original local verification surface.

| Area | Status | Evidence | Remaining |
| --- | --- | --- | --- |
| Binance market metadata ingestion | Done for local MVP | `ingest_binance_market_meta`, idempotent storage upsert and parser tests exist | Network-backed production tests remain gated by connectivity |
| Binance funding-rate ingestion | Done for local MVP | Incremental latest-time fetch, parser tests and `funding_rates` storage readback exist | Production backoff/rate-limit policy is not implemented |
| Yahoo corporate actions ingestion | Done for local MVP | `ingest_yahoo_corporate_actions`, parser tests and idempotent storage upsert exist | Provider hardening and broader action coverage remain future work |
| Scheduled ingestion/status | Done for local MVP | `[ingestion]` config, `run_scheduled_ingestion`, `ingest status` and `/api/v1/ingestion/status` exist | Stale-data alerting and production retry strategy remain future work |

---

## Scope

In scope:

- Binance exchangeInfo → `crypto_market_meta` (symbol, contract type, filters, margin info).
- Binance funding rate history → `funding_rates` table.
- Corporate actions from Yahoo Finance / other providers → `corporate_actions_meta`.
- CLI-triggered ingestion commands.
- Scheduled ingestion (configurable interval).
- Ingestion metadata tracking (last fetch, row count, change detection).
- Idempotent upserts (re-running is safe and cheap).

Out of scope:

- Real-time streaming of market data changes.
- Tick-level data ingestion.
- Full historical data lake (tick, order book, OHLCV beyond current bars).
- Multi-exchange unified reference data normalization.
- Web UI for ingestion monitoring.

## File Map

### Ingestion Module

- Create: `crates/data/src/ingestion/mod.rs`
  - Top-level ingestion coordinator.
- Create: `crates/data/src/ingestion/binance_meta.rs`
  - Binance exchangeInfo fetcher and parser.
- Create: `crates/data/src/ingestion/binance_funding.rs`
  - Binance funding rate history fetcher.
- Create: `crates/data/src/ingestion/corporate_actions.rs`
  - Yahoo Finance / other provider corporate actions fetcher.
- Create: `crates/data/src/ingestion/tracker.rs`
  - Ingestion metadata tracking (last fetch time, row count, status).

### Storage

- Modify: `crates/storage/src/repositories.rs`
  - Add `upsert_crypto_market_meta` (idempotent on exchange+symbol).
  - Add `upsert_corporate_action` (idempotent on symbol+ex_date+action_type).
  - Add `upsert_funding_rate` (already exists from contract accounting plan, reuse).
  - Add `list_ingestion_log` for monitoring.
  - Add ingestion metadata table or use `system_logs`.
- Modify: `crates/storage/tests/storage_tests.rs`
  - Add upsert idempotency tests.
  - Add ingestion log round-trip test.

### CLI

- Modify: `apps/trader-cli/src/main.rs`
  - Add `ingest binance-meta` command.
  - Add `ingest funding-rates --exchange <ex> [--symbol <sym>]` command.
  - Add `ingest corporate-actions [--symbol <sym>]` command.
  - Add `ingest status` command showing last fetch times.

### Configuration

- Modify: `crates/config/src/config.rs`
  - Add ingestion config section: enabled sources, fetch interval, API credentials reference.
- Modify: configs/*.toml
  - Add ingestion configuration examples.

### API (Optional)

- Modify: `crates/api/src/api.rs`
  - Add `GET /api/v1/ingestion/status` for monitoring.
- Modify: `docs/api.md`
  - Document ingestion status endpoint.

### Documentation

- Modify: `docs/分析.md`
  - Update reference-data section from "manual population" to "automated ingestion".
- Modify: `docs/roadmap.md`
  - Add "Reference-Data Ingestion" milestone.

---

## Acceptance Gates

Every task must preserve:

- `cargo test -p storage`
- `cargo test -p data`
- `cargo test -p api`
- `powershell -ExecutionPolicy Bypass -File .\scripts\v1-smoke.ps1`
- `bash ./scripts/check-db-boundary`
- `bash ./scripts/check-storage-dto-boundary`

New gates for this plan:

- `cargo test -p data ingestion` — all ingestion module tests.
- `cargo test -p storage upsert_crypto_market_meta` — idempotent upsert.
- `cargo test -p storage upsert_corporate_action` — idempotent upsert.
- Integration test: `ingest binance-meta` populates `crypto_market_meta` (requires network).

---

## Task 1: Add Binance Market Meta Ingestion

**Files:**

- Create: `crates/data/src/ingestion/mod.rs`
- Create: `crates/data/src/ingestion/binance_meta.rs`
- Modify: `crates/storage/src/repositories.rs`
- Modify: `crates/storage/tests/storage_tests.rs`
- Modify: `crates/data/Cargo.toml`

- [x] **Step 1: Add `reqwest` dependency to `crates/data/Cargo.toml`**

```toml
reqwest = { version = "0.12", features = ["json"] }
```

- [x] **Step 2: Define Binance exchangeInfo response types**

```rust
#[derive(Deserialize)]
struct BinanceExchangeInfo {
    symbols: Vec<BinanceSymbolInfo>,
}

#[derive(Deserialize)]
struct BinanceSymbolInfo {
    symbol: String,
    #[serde(rename = "contractType")]
    contract_type: Option<String>,
    status: String,
    #[serde(rename = "baseAsset")]
    base_asset: String,
    #[serde(rename = "quoteAsset")]
    quote_asset: String,
    filters: Vec<BinanceFilter>,
}
```

- [x] **Step 3: Implement fetch and parse**

```rust
pub async fn fetch_binance_market_meta(client: &reqwest::Client) -> Result<Vec<NewCryptoMarketMeta>, IngestionError> {
    let resp = client.get("https://api.binance.com/api/v3/exchangeInfo")
        .send().await?
        .json::<BinanceExchangeInfo>().await?;
    // Parse each symbol into NewCryptoMarketMeta
    // Extract lot size, tick size, min notional from filters
}
```

- [x] **Step 4: Add storage upsert**

```rust
pub async fn upsert_crypto_market_meta(&self, meta: &NewCryptoMarketMeta) -> StorageResult<()> {
    sqlx::query(
        r#"
        INSERT INTO crypto_market_meta (exchange, symbol, contract_type, status, base_asset, quote_asset, lot_size, tick_size, min_notional, updated_at_ms)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(exchange, symbol) DO UPDATE SET
            contract_type = excluded.contract_type,
            status = excluded.status,
            lot_size = excluded.lot_size,
            tick_size = excluded.tick_size,
            min_notional = excluded.min_notional,
            updated_at_ms = excluded.updated_at_ms
        "#,
    )
    // ... bind
    .execute(self.pool()).await?;
    Ok(())
}
```

- [x] **Step 5: Add tests**

```rust
#[tokio::test]
async fn upsert_crypto_market_meta_idempotent() {
    // Insert, then insert again with different lot_size
    // Assert: row updated, not duplicated
    // Assert: lot_size reflects second insert
}
```

- [x] **Step 6: Commit**

```powershell
git add crates/data crates/storage
git commit -m "feat: binance market meta ingestion"
```

---

## Task 2: Add Binance Funding Rate Ingestion

**Files:**

- Create: `crates/data/src/ingestion/binance_funding.rs`
- Modify: `crates/storage/src/repositories.rs`

- [x] **Step 1: Implement funding rate fetch**

```rust
pub async fn fetch_binance_funding_history(
    client: &reqwest::Client,
    symbol: &str,
    start_ms: Option<i64>,
    end_ms: Option<i64>,
    limit: Option<u32>,
) -> Result<Vec<NewFundingRate>, IngestionError> {
    // GET /fapi/v1/fundingRate
    // Parse into NewFundingRate with exchange="binance"
}
```

- [x] **Step 2: Add incremental fetch logic**

```rust
pub async fn ingest_funding_rates(
    db: &Db,
    client: &reqwest::Client,
    symbol: &str,
) -> Result<IngestionResult, IngestionError> {
    // Get latest funding rate time from DB
    // Fetch only rates after that time
    // Upsert each rate
    // Return count of new/updated records
}
```

- [x] **Step 3: Add tests**

```rust
#[tokio::test]
async fn fetch_binance_funding_history_returns_data() {
    // Only runs if network available
    // Assert: returns non-empty Vec<FundingRateRecord>
}

#[tokio::test]
async fn incremental_fetch_skips_existing() {
    // Insert one funding rate, then run incremental fetch
    // Assert: only new rates are inserted
}
```

- [x] **Step 4: Commit**

```powershell
git add crates/data crates/storage
git commit -m "feat: binance funding rate ingestion"
```

---

## Task 3: Add Corporate Actions Ingestion

**Files:**

- Create: `crates/data/src/ingestion/corporate_actions.rs`
- Modify: `crates/storage/src/repositories.rs`

- [x] **Step 1: Define corporate action types**

```rust
pub enum CorporateActionType {
    Dividend,
    Split,
    Merger,
    SpinOff,
    SymbolChange,
}

pub struct NewCorporateAction {
    pub symbol: String,
    pub action_type: CorporateActionType,
    pub ex_date: String,
    pub record_date: Option<String>,
    pub payment_date: Option<String>,
    pub ratio: Option<String>,        // for splits
    pub amount: Option<String>,       // for dividends
    pub description: String,
    pub source: String,
    pub fetched_at_ms: i64,
}
```

- [x] **Step 2: Implement Yahoo Finance fetcher**

```rust
pub async fn fetch_yahoo_corporate_actions(
    client: &reqwest::Client,
    symbol: &str,
) -> Result<Vec<NewCorporateAction>, IngestionError> {
    // Fetch from Yahoo Finance API or scrape
    // Parse dividends, splits into NewCorporateAction
}
```

- [x] **Step 3: Add storage upsert**

```rust
pub async fn upsert_corporate_action(&self, action: &NewCorporateAction) -> StorageResult<()> {
    sqlx::query(
        r#"
        INSERT INTO corporate_actions_meta (symbol, action_type, ex_date, record_date, payment_date, ratio, amount, description, source, fetched_at_ms)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(symbol, ex_date, action_type) DO UPDATE SET
            record_date = excluded.record_date,
            payment_date = excluded.payment_date,
            ratio = excluded.ratio,
            amount = excluded.amount,
            description = excluded.description,
            fetched_at_ms = excluded.fetched_at_ms
        "#,
    )
    // ... bind
    .execute(self.pool()).await?;
    Ok(())
}
```

- [x] **Step 4: Add tests**

```rust
#[tokio::test]
async fn upsert_corporate_action_idempotent() { ... }
#[tokio::test]
async fn fetch_yahoo_corporate_actions_returns_data() { ... }
```

- [x] **Step 5: Commit**

```powershell
git add crates/data crates/storage
git commit -m "feat: corporate actions ingestion"
```

---

## Task 4: Add Ingestion Tracker and CLI Commands

**Files:**

- Create: `crates/data/src/ingestion/tracker.rs`
- Modify: `apps/trader-cli/src/main.rs`

- [x] **Step 1: Implement ingestion tracker**

```rust
pub struct IngestionTracker;

impl IngestionTracker {
    pub async fn log_ingestion(db: &Db, source: &str, table: &str, rows_fetched: usize, rows_upserted: usize, duration_ms: i64) -> StorageResult<()> {
        // Write to system_logs with category "ingestion"
    }
    pub async fn last_ingestion(db: &Db, source: &str, table: &str) -> StorageResult<Option<IngestionStatus>> {
        // Query system_logs for latest ingestion entry
    }
}
```

- [x] **Step 2: Add CLI commands**

```
trader ingest binance-meta [--exchange binance]
trader ingest funding-rates --exchange binance [--symbol BTCUSDT]
trader ingest corporate-actions [--symbol AAPL]
trader ingest status
```

- [x] **Step 3: Add CLI tests**

```rust
#[test]
fn ingest_status_shows_last_fetch_time() { ... }
```

- [x] **Step 4: Commit**

```powershell
git add crates/data apps/trader-cli
git commit -m "feat: ingestion tracker and CLI commands"
```

---

## Task 5: Add Scheduled Ingestion and API Status

**Files:**

- Modify: `crates/data/src/ingestion/mod.rs`
- Modify: `crates/config/src/config.rs`
- Modify: `crates/api/src/api.rs`
- Modify: `crates/api/tests/api_tests.rs`
- Modify: `docs/api.md`

- [x] **Step 1: Add ingestion config**

```toml
[ingestion]
enabled = true
sources = ["binance"]
fetch_interval_minutes = 60
symbols = ["BTCUSDT", "ETHUSDT"]
```

- [x] **Step 2: Implement scheduled ingestion**

```rust
pub async fn run_scheduled_ingestion(db: &Db, config: &IngestionConfig) -> Result<(), IngestionError> {
    // For each enabled source:
    //   1. Fetch market meta (once per startup or daily)
    //   2. Fetch funding rates (incremental, per interval)
    //   3. Fetch corporate actions (per interval, only if configured)
    // Log results via IngestionTracker
}
```

- [x] **Step 3: Add API status endpoint**

```
GET /api/v1/ingestion/status
```

Response:
```json
{
  "sources": [
    {
      "name": "binance",
      "last_meta_fetch": "2026-06-17T10:00:00Z",
      "last_funding_fetch": "2026-06-17T10:30:00Z",
      "meta_row_count": 1200,
      "funding_row_count": 50000
    }
  ]
}
```

- [x] **Step 4: Add tests and docs**

- API test for ingestion status endpoint.
- `docs/api.md` documentation.

- [x] **Step 5: Run full acceptance**

```powershell
cargo test -p data ingestion
cargo test -p api
powershell -ExecutionPolicy Bypass -File .\scripts\v1-smoke.ps1
bash ./scripts/check-api-read-model-boundary
```

Expected: all pass.

- [x] **Step 6: Commit**

```powershell
git add crates/data crates/config crates/api apps/trader-cli docs/api.md
git commit -m "feat: scheduled ingestion and API status"
```

---

## Task 6: Update Documentation

**Files:**

- Modify: `docs/分析.md`
- Modify: `docs/roadmap.md`

- [x] **Step 1: Update `docs/分析.md`**

Change reference-data section to reflect automated ingestion capabilities and limitations.

- [x] **Step 2: Update `docs/roadmap.md`**

Add "Reference-Data Ingestion" milestone.

- [x] **Step 3: Commit**

```powershell
git add docs
git commit -m "docs: update reference-data ingestion status"
```

---

## Implementation Order

1. Task 1: Binance market meta ingestion (foundation).
2. Task 2: Binance funding rate ingestion (reuses storage from contract accounting plan).
3. Task 3: Corporate actions ingestion.
4. Task 4: Tracker + CLI commands.
5. Task 5: Scheduled ingestion + API status.
6. Task 6: Documentation.

## Risks and Controls

- **Risk:** External API rate limits block ingestion.
  - **Control:** Respect rate limits (Binance: 1200 req/min). Add exponential backoff. Log rate limit hits.
- **Risk:** API response format changes break parsing.
  - **Control:** Strict serde deserialization with `#[serde(deny_unknown_fields)]` for early detection. Log parse errors.
- **Risk:** Large ingestion writes block runtime.
  - **Control:** Run ingestion in background task. Use batch upserts (100 rows per query). Don't hold pool connections during HTTP calls.
- **Risk:** Stale reference data causes incorrect trading decisions.
  - **Control:** Track last fetch time. Alert if data is older than configured threshold. Add `ingest status` CLI command.

## Success Criteria

The project is materially improved when:

- `crypto_market_meta` is populated by Binance exchangeInfo fetch.
- `funding_rates` is incrementally populated by scheduled fetch.
- `corporate_actions_meta` is populated from external provider.
- `ingest status` shows last fetch time and row counts.
- Re-running ingestion is idempotent (no duplicates, no errors).
- Existing MVP smoke still passes.
