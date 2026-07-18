# Contract Runtime Accounting Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire runtime accounting to actually write `crypto_positions` and `funding_rates` tables, enabling complete contract position lifecycle tracking from fill through funding settlement to close.

**Architecture:** Keep `event_store` as the immutable audit truth. `crypto_positions` is a runtime-derived ledger state table, updated on fills, funding settlement, and reconciliation. `funding_rates` stores market funding rate history. All Decimal math stays in Rust; storage boundary uses strings. Broker adapters provide the external state for reconciliation.

**Tech Stack:** Rust workspace, SQLx SQLite, Axum, serde, rust_decimal, Binance API, IBKR API, PowerShell smoke scripts.

## Current Status (2026-07-08 Sync)

This plan file has now been backfilled for the audited local MVP. Checked items below are confirmed implemented; unchecked items remain original production scope or exact plan steps that have not landed.

2026-07-08 sync note: targeted local verification reconfirmed the current broker-boundary evidence in `crates/broker`: Binance position-risk parsing and reconciliation drift tests, IBKR paper Gateway account/position snapshot adapter tests, and local production reconciliation/gate tests. The original Task 3 external-network testnet/Gateway steps remain unchecked because no fresh broker network action was run in this sync.

Boundary: this plan proves local contract accounting storage/runtime behavior plus broker-boundary parsing and paper/testnet-derived reconciliation evidence. It does not prove live-money contract accounting, filled real-broker order reconciliation, or complete production multi-asset coverage.

| Area | Status | Evidence | Remaining |
| --- | --- | --- | --- |
| Storage lifecycle | Done for local MVP | `Db::upsert_crypto_position`, `list_crypto_positions`, `get_crypto_position`, `upsert_funding_rate`, `list_funding_rates`, `get_latest_funding_rate`; covered by `runtime_repository_tests` | None for local storage boundary |
| Simulated contract accounting | Done for local MVP | `SimulatedContractAccounting`, `ContractAccountingBook`, paper runtime persistence, paper tests for crypto perp position and funding PnL | None for simulated paper |
| Contract risk checks | Done for current scope | `market_rules::ContractRiskLimits` and algorithm tests for leverage, margin and funding bounds | Production liquidation/open-interest data still not wired |
| CLI/API readback | Done for local MVP | `trader positions list`, `trader funding list`, `GET /api/v1/runs/{run_id}/crypto-positions`, `GET /api/v1/funding-rates` | None for read-only local surface |
| Binance reconciliation | Partially done | Binance position parsing and reconciliation drift tests exist in `crates/broker`; Binance paper run/soak and fresh read-only gate evidence are documented outside this original plan | Scheduled real broker contract-position reconciliation remains follow-up |
| IBKR contract reconciliation | Partially done | IBKR paper adapter can now stream broker account/position snapshots through the broker boundary, API-launched live runtime can inject the IBKR paper Gateway adapter from config instead of hard-coded fake snapshots, and IBKR paper production reconciliation has accepted 39-audit evidence outside this original plan | Add real Gateway contract-position/fill reconciliation verification and fuller multi-asset coverage |
| Production readiness | Not done | `docs/分析.md` still classifies this as not full production accounting | Real Gateway long-run verification, production alerts, exchange metadata such as liquidation/open-interest |

---

## Scope

This plan wires the existing `crypto_positions` and `funding_rates` schema (migration 0004) into the runtime. It does NOT claim full derivative support until broker reconciliation tests pass against real testnet endpoints.

In scope:

- Funding settlement: calculate and persist `funding_fee` when funding rate events arrive.
- Position lifecycle: open, update, close positions with correct `position_side` and `margin_mode`.
- Leverage and margin tracking: `leverage`, `margin_used`, liquidation price.
- PnL split: `realized_pnl`, `unrealized_pnl`, `funding_pnl` as separate fields.
- Broker reconciliation: compare runtime state against broker-reported positions.
- Wire into simulated paper first, then Binance testnet, then IBKR paper.

Out of scope:

- Real-money live trading.
- Cross-exchange portfolio margin.
- Options and structured products.
- High-frequency position updates (tick-level).

## File Map

### Storage

- Modify: `crates/storage/src/repositories.rs`
  - Add `update_crypto_position` (upsert by run_id/account_id/exchange/symbol/position_side).
  - Add `list_crypto_positions` (filter by run_id, account_id).
  - Add `insert_funding_rate` with upsert on (exchange, symbol, funding_time_ms).
  - Add `list_funding_rates` (filter by exchange, symbol, time range).
  - Add `get_latest_funding_rate` for settlement lookup.
- Modify: `crates/storage/tests/storage_tests.rs`
  - Add crypto position upsert and funding rate round-trip tests.
- Modify: `crates/storage/tests/runtime_repository_tests.rs`
  - Add tests for position lifecycle (open → update → close).

### Algorithm and Runtime

- Modify: `crates/algorithm/src/algorithm.rs`
  - Add `ContractAccountingBook` trait with methods: `on_fill`, `on_funding`, `on_reconciliation`, `get_position`.
  - Implement `SimulatedContractAccounting` for paper mode.
- Modify: `crates/paper/src/paper.rs`
  - Wire `SimulatedContractAccounting` into paper runtime for CRYPTO_PERP / CRYPTO_FUTURE instruments.
  - On fill: upsert `crypto_positions` with correct side, qty, avg_price, margin.
  - On simulated funding: calculate funding fee based on position size and funding rate.
- Modify: `crates/paper/tests/paper_tests.rs`
  - Add test: paper crypto perp run writes `crypto_positions`.
  - Add test: funding settlement updates `funding_fee` and `realized_pnl`.

### Broker Adapters

- Modify: `crates/binance/src/binance.rs` (or relevant adapter file)
  - Add `fetch_positions` method returning broker-reported positions.
  - Add `fetch_funding_rate` / `fetch_funding_history` methods.
  - Add reconciliation: compare runtime `crypto_positions` against broker state.
- Modify: `crates/ibkr/src/ibkr.rs` (or relevant adapter file)
  - Add position snapshot fetch for IBKR paper/live.
  - Add reconciliation against IBKR-reported positions.
- Modify: `crates/binance/tests/binance_tests.rs`
  - Add test: fetch positions from Binance testnet returns valid data.
  - Add test: reconciliation detects drift.

### Risk and Market Rules

- Modify: `crates/market_rules/src/market_rules.rs`
  - Add contract-specific validation: max leverage, margin requirements, funding rate bounds.
- Modify: `crates/algorithm/src/algorithm.rs`
  - Add contract risk checks: margin ratio, liquidation proximity, max position size.

### CLI and API

- Modify: `apps/trader-cli/src/main.rs`
  - Add `positions list --run-id <id>` command showing crypto positions.
  - Add `funding list --exchange <ex> --symbol <sym>` command.
- Modify: `crates/api/src/api.rs`
  - Add `GET /api/v1/runs/{run_id}/crypto-positions` read-only endpoint.
  - Add `GET /api/v1/funding-rates` with exchange/symbol/time filters.
- Modify: `crates/api/tests/api_tests.rs`
  - Add route tests for new endpoints.
- Modify: `docs/api.md`
  - Document new endpoints.

### Documentation

- Modify: `docs/分析.md`
  - Update contract accounting section: schema + runtime wiring status.
  - Explicitly state which broker adapters have reconciliation tests.
- Modify: `docs/database.md`
  - Update `crypto_positions` and `funding_rates` field descriptions.
- Modify: `docs/roadmap.md`
  - Add "Contract Runtime Accounting" milestone with sub-stages.

---

## Acceptance Gates

Every task must preserve these gates:

- `cargo test -p storage`
- `cargo test -p algorithm`
- `cargo test -p paper`
- `cargo test -p backtest`
- `cargo test -p api`
- `cargo test -p market_rules`
- `cargo test -p binance` (if adapter exists)
- `cargo test -p ibkr` (if adapter exists)
- `powershell -ExecutionPolicy Bypass -File .\scripts\smoke\v1-smoke.ps1`
- `bash ./scripts/check/check-db-boundary`
- `bash ./scripts/check/check-storage-dto-boundary`
- `bash ./scripts/check/check-api-read-model-boundary`

New gates for this plan:

- `cargo test -p storage crypto_position_lifecycle` — position open/update/close round-trip.
- `cargo test -p storage funding_settlement` — funding fee calculation and persistence.
- `cargo test -p paper paper_crypto_perp_run` — paper run writes crypto positions.
- `cargo test -p binance reconciliation` — broker state comparison (testnet).

---

## Task 1: Extend Storage for Position Lifecycle

**Files:**

- Modify: `crates/storage/src/repositories.rs`
- Modify: `crates/storage/tests/storage_tests.rs`
- Modify: `crates/storage/tests/runtime_repository_tests.rs`

- [x] **Step 1: Add crypto position upsert method**

```rust
pub async fn upsert_crypto_position(&self, pos: &NewCryptoPosition) -> StorageResult<()> {
    sqlx::query(
        r#"
        INSERT INTO crypto_positions (
            run_id, account_id, exchange, symbol, asset_class,
            margin_mode, position_side, leverage, qty, avg_price,
            margin_used, funding_fee, realized_pnl, unrealized_pnl, updated_at_ms
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(run_id, account_id, exchange, symbol, position_side)
        DO UPDATE SET
            qty = excluded.qty,
            avg_price = excluded.avg_price,
            leverage = excluded.leverage,
            margin_mode = excluded.margin_mode,
            margin_used = excluded.margin_used,
            funding_fee = excluded.funding_fee,
            realized_pnl = excluded.realized_pnl,
            unrealized_pnl = excluded.unrealized_pnl,
            updated_at_ms = excluded.updated_at_ms
        "#,
    )
    // ... bind all fields
    .execute(self.pool())
    .await?;
    Ok(())
}
```

- [x] **Step 2: Add list and get methods**

Add `list_crypto_positions(run_id)` and `get_crypto_position(run_id, account_id, exchange, symbol, position_side)`.

- [x] **Step 3: Add funding rate upsert and query methods**

```rust
pub async fn upsert_funding_rate(&self, rate: &NewFundingRate) -> StorageResult<()>
pub async fn list_funding_rates(&self, exchange: &str, symbol: Option<&str>, from_ms: Option<i64>, to_ms: Option<i64>) -> StorageResult<Vec<StoredFundingRate>>
pub async fn get_latest_funding_rate(&self, exchange: &str, symbol: &str) -> StorageResult<Option<StoredFundingRate>>
```

- [x] **Step 4: Add storage tests**

Tests must assert:
- Upsert crypto position creates row, then update modifies qty/avg_price without creating duplicate.
- Decimal strings round-trip exactly (no precision loss).
- Funding rate upsert on same (exchange, symbol, time) overwrites.
- list_funding_rates with time filter returns correct range.

- [x] **Step 5: Run storage tests**

```powershell
cargo test -p storage crypto_position
cargo test -p storage funding_rate
cargo test -p storage funding_settlement
```

Expected: pass.

- [x] **Step 6: Commit**

```powershell
git add crates/storage
git commit -m "feat: extend crypto position and funding rate storage"
```

---

## Task 2: Implement Simulated Contract Accounting

**Files:**

- Modify: `crates/algorithm/src/algorithm.rs`
- Modify: `crates/paper/src/paper.rs`
- Modify: `crates/paper/tests/paper_tests.rs`

- [x] **Step 1: Define ContractAccountingBook trait**

```rust
pub trait ContractAccountingBook {
    async fn on_fill(&mut self, fill: &ContractFill) -> Result<(), AccountingError>;
    async fn on_funding(&mut self, rate: &FundingRateEvent) -> Result<(), AccountingError>;
    async fn on_reconciliation(&mut self, broker_state: &BrokerPositionSnapshot) -> Result<ReconciliationResult, AccountingError>;
    fn get_position(&self, symbol: &str, side: PositionSide) -> Option<&ContractPosition>;
}
```

- [x] **Step 2: Implement SimulatedContractAccounting**

For paper mode:
- `on_fill`: update qty, avg_price, margin_used. If position closes (qty → 0), calculate realized_pnl.
- `on_funding`: calculate funding_fee = position_qty × funding_rate × mark_price. Deduct from/add to realized_pnl.
- `on_reconciliation`: compare against broker snapshot, return drift report.

- [x] **Step 3: Wire into paper runtime**

In `crates/paper/src/paper.rs`, after each fill for CRYPTO_PERP/CRYPTO_FUTURE:
1. Call `accounting.on_fill(fill)`.
2. Call `db.upsert_crypto_position(&position)`.
3. On funding event: call `accounting.on_funding(rate)`, then `db.upsert_crypto_position`.

- [x] **Step 4: Add paper tests**

```rust
#[tokio::test]
async fn paper_crypto_perp_run_writes_crypto_positions() {
    // Setup: crypto perp instrument, paper run
    // Run: 3 bars that trigger a fill
    // Assert: crypto_positions table has one row with correct side/qty/avg_price
}

#[tokio::test]
async fn paper_crypto_perp_funding_settlement() {
    // Setup: open position, then trigger funding event
    // Assert: funding_fee updated, realized_pnl adjusted
}
```

- [x] **Step 5: Run tests**

```powershell
cargo test -p paper paper_crypto_perp
cargo test -p algorithm contract_accounting
```

Expected: pass.

- [x] **Step 6: Commit**

```powershell
git add crates/algorithm crates/paper
git commit -m "feat: implement simulated contract accounting"
```

---

## Task 3: Add Broker Position Fetch and Reconciliation

**Files:**

- Modify: `crates/binance/src/binance.rs`
- Modify: `crates/binance/tests/binance_tests.rs`
- Modify: `crates/ibkr/src/ibkr.rs`
- Modify: `crates/ibkr/tests/ibkr_tests.rs`

2026-07-08 status: the current implementation lives under the unified `crates/broker` boundary rather than separate `crates/binance` / `crates/ibkr` crates. Local tests cover Binance response mapping and reconciliation drift plus IBKR paper Gateway account/position snapshot adapter behavior; the original network-dependent testnet/Gateway steps below remain unchecked.

- [x] **Step 1: Add Binance position fetch**

```rust
pub async fn fetch_positions(&self) -> Result<Vec<BrokerPositionSnapshot>, BrokerError>
pub async fn fetch_funding_history(&self, symbol: &str, start_ms: i64, end_ms: i64) -> Result<Vec<FundingRateRecord>, BrokerError>
```

- [x] **Step 2: Implement reconciliation logic**

```rust
pub fn reconcile_positions(
    runtime: &[StoredCryptoPosition],
    broker: &[BrokerPositionSnapshot],
) -> ReconciliationReport {
    // For each broker position, find matching runtime position
    // Report: missing in runtime, missing in broker, qty mismatch, margin mismatch
}
```

- [ ] **Step 3: Add Binance testnet tests**

```rust
#[tokio::test]
async fn binance_fetch_positions_testnet() {
    // Only runs if BINANCE_TESTNET_API_KEY is set
    // Assert: returns Vec<BrokerPositionSnapshot>
}

#[tokio::test]
async fn binance_reconciliation_detects_drift() {
    // Create runtime position with wrong qty
    // Compare against broker
    // Assert: drift detected
}
```

- [x] **Step 4: Add IBKR account/position fetch (adapter boundary)**

Similar pattern: fetch account/positions from IBKR paper, reconcile against runtime. Adapter-boundary and API live-runtime injection are implemented; real Gateway reconciliation verification remains follow-up.

- [ ] **Step 5: Run tests**

```powershell
cargo test -p binance reconciliation
cargo test -p ibkr reconciliation
```

Expected: pass (testnet tests skipped if credentials not available).

- [ ] **Step 6: Commit**

```powershell
git add crates/binance crates/ibkr
git commit -m "feat: broker position fetch and reconciliation"
```

---

## Task 4: Add Contract Risk Checks

**Files:**

- Modify: `crates/market_rules/src/market_rules.rs`
- Modify: `crates/algorithm/src/algorithm.rs`

- [x] **Step 1: Add contract validation rules**

```rust
pub struct ContractRiskLimits {
    pub max_leverage: Decimal,
    pub min_margin_ratio: Decimal,
    pub max_position_notional: Decimal,
    pub liquidation_buffer_bps: Decimal,
}
```

- [x] **Step 2: Add risk check in algorithm execution**

Before submitting a contract order:
1. Check leverage ≤ max_leverage.
2. Check margin ratio ≥ min_margin_ratio after hypothetical fill.
3. Check position notional ≤ max.
4. Check liquidation price has sufficient buffer.
5. Emit `risk_events` rejection if any check fails.

- [x] **Step 3: Add tests**

```rust
#[tokio::test]
async fn rejects_order_exceeding_max_leverage() { ... }
#[tokio::test]
async fn rejects_order_insufficient_margin() { ... }
```

- [x] **Step 4: Commit**

```powershell
git add crates/market_rules crates/algorithm
git commit -m "feat: contract-specific risk checks"
```

---

## Task 5: Add CLI and API for Contract Positions

**Files:**

- Modify: `apps/trader-cli/src/main.rs`
- Modify: `crates/api/src/api.rs`
- Modify: `crates/api/tests/api_tests.rs`
- Modify: `docs/api.md`

- [x] **Step 1: Add API endpoints**

```
GET /api/v1/runs/{run_id}/crypto-positions
GET /api/v1/funding-rates?exchange={ex}&symbol={sym}&from_ms={t1}&to_ms={t2}
```

- [x] **Step 2: Add API response structs (owned by API, not leaking storage DTO)**

```rust
#[derive(Serialize)]
struct CryptoPositionResponse { ... }
#[derive(Serialize)]
struct FundingRateResponse { ... }
```

- [x] **Step 3: Add CLI commands**

```
trader positions list --run-id <id> [--account <acct>] [--exchange <ex>]
trader funding list --exchange <ex> [--symbol <sym>] [--from <ts>] [--to <ts>]
```

- [x] **Step 4: Add tests and docs**

- API tests for new endpoints.
- `docs/api.md` documentation.
- Boundary check passes.

- [x] **Step 5: Run full acceptance**

```powershell
cargo test -p api
cargo test -p market_rules
powershell -ExecutionPolicy Bypass -File .\scripts\smoke\v1-smoke.ps1
bash ./scripts/check/check-api-read-model-boundary
```

Expected: all pass.

- [x] **Step 6: Commit**

```powershell
git add crates/api apps/trader-cli docs/api.md
git commit -m "feat: contract position CLI and API"
```

---

## Task 6: Update Documentation

**Files:**

- Modify: `docs/分析.md`
- Modify: `docs/database.md`
- Modify: `docs/roadmap.md`

- [x] **Step 1: Update `docs/分析.md`**

Change contract accounting section from "storage boundary exists, runtime not wired" to:
- Runtime accounting wired for simulated paper and Binance testnet.
- Reconciliation tests exist for Binance.
- List remaining gaps: IBKR reconciliation, cross-exchange margin, real-money readiness.

- [x] **Step 2: Update `docs/database.md`**

Update `crypto_positions` field list to include new fields (funding_pnl, liquidation_price if added).

- [x] **Step 3: Update `docs/roadmap.md`**

Add "Contract Runtime Accounting" milestone with stages:
1. Storage boundary ✅
2. Simulated accounting ✅
3. Broker reconciliation (Binance testnet) ✅
4. Contract risk checks ✅
5. Full multi-exchange reconciliation (pending)
6. Production readiness (pending)

- [x] **Step 4: Commit**

```powershell
git add docs
git commit -m "docs: update contract accounting status"
```

---

## Implementation Order

1. Task 1: Storage extensions (foundation).
2. Task 2: Simulated accounting (paper mode).
3. Task 3: Broker fetch + reconciliation (Binance first).
4. Task 4: Contract risk checks.
5. Task 5: CLI + API exposure.
6. Task 6: Documentation.

Do not start Task 3 before Task 2 is complete — reconciliation needs a working runtime state to compare against. Do not start Task 4 before Task 2 — risk checks need position lifecycle data.

## Risks and Controls

- **Risk:** Funding settlement math errors cause incorrect PnL.
  - **Control:** Unit test every funding calculation path. Compare against Binance testnet historical funding data.
- **Risk:** Reconciliation false positives from timing differences.
  - **Control:** Add tolerance window (e.g., 1 second) for timestamp comparison. Log but don't alert on minor drift.
- **Risk:** Adapter API changes break position fetch.
  - **Control:** Integration tests against testnet. Graceful error handling — log warning, don't crash runtime.
- **Risk:** Contract accounting creates false confidence for production trading.
  - **Control:** Docs must explicitly state which adapters have reconciliation tests. No adapter = no production claim.

## Success Criteria

The project is materially improved when:

- `crypto_positions` table is populated by paper and Binance testnet runs.
- `funding_rates` table is populated by market data fetch.
- Funding settlement correctly adjusts position PnL.
- Reconciliation detects position drift against Binance testnet.
- Contract risk checks reject orders exceeding leverage/margin limits.
- Existing MVP smoke still passes.
- Docs accurately reflect which adapters have accounting support.
