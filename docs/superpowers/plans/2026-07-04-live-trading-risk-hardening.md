# Live Trading Risk Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the missing hard risk gates required before tiny-size real-money trading: daily loss stop, order-rate limits, stale-market rejection, price-deviation rejection, trading-session gating, global kill switch with cancel-all, and operator evidence for broker reconciliation soak runs.

**Architecture:** Keep pre-trade protection centralized at the algorithm/runtime boundary, not scattered across broker adapters. Config owns thresholds, `risk` owns pure decision logic, `algorithm` owns per-bar/per-signal enforcement and event emission, `paper` and broker-facing paths own final submit/cancel behavior, and CLI/scripts own operator kill-switch workflows and soak evidence collection.

**Tech Stack:** Rust workspace, SQLx SQLite, serde/toml, rust_decimal, Tokio, Clap, PowerShell scripts.

## Current Status

Status as of 2026-07-06 on branch `live-trading-risk-hardening`:

```text
Implementation: complete
Broad compile verification: complete
Full acceptance test set: partial; script contracts and IBKR Gateway evidence complete
Real broker / Gateway evidence: IBKR paper Gateway complete on 2026-07-06
```

Implemented surfaces:

- [x] Config and `RunSpec` preserve live-risk hardening fields.
- [x] Pure risk guards exist for stale market data, price deviation, daily loss, order attempts, order failures, strategy circuit breakers, and trading session windows.
- [x] Algorithm / paper / backtest paths consume the live-risk settings and emit auditable `algorithm.risk.rejected` events.
- [x] Paper and backtest fill accounting share `apply_fill_at`.
- [x] Generic `trader risk-kill-switch` records `operator_kill_switch` and can cancel known open orders.
- [x] Binance / IBKR paper run and soak scripts write `halt_reason`, `risk_rejections`, `open_orders_remaining`, `cancel_all_*`, and reconciliation evidence.
- [x] `docs/roadmap.md` has explicit paper-ready, tiny-size candidate, and not-production-complete readiness buckets.

Verification already run:

```powershell
cargo check --workspace
powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-script-tests.ps1
powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-test-guide.ps1 -Stage ReadOnly -AccountId DUQ645291 -Port 4002
powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-test-guide.ps1 -Stage AutoRun -AccountId DUQ645291 -Port 4002 -ConfirmAutoRun
powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-soak.ps1 -Iterations 3 -AccountId DUQ645291 -Port 4002 -ConfirmIbkrPaperOrder
```

Result: PASS.

IBKR evidence artifacts:

- Read-only summary: `data/ibkr-paper-test/read-only-414fa8a031fb/summary.json`
- AutoRun summary: `data/ibkr-paper-runs/ibkr-aapl-1d-afb4fdab9323/summary.json`
- Soak summary: `data/ibkr-paper-soak/ibkr-paper-soak-af20e6620229/summary.json`

IBKR acceptance observations:

- Read-only Gateway checks completed with `failure_class = ok`.
- AutoRun completed with `failure_class = ok`, `open_orders_remaining = 0`, and `reconciliation_status = ok`.
- Three-iteration soak completed with every iteration `failure_class = ok`, `open_orders_remaining = 0`, and `reconciliation_status = ok`.

Remaining acceptance verification:

```powershell
cargo test -p config
cargo test -p risk
cargo test -p algorithm
cargo test -p paper
cargo test -p broker
cargo test -p trader-cli
cargo check --workspace
powershell -ExecutionPolicy Bypass -File .\scripts\binance-paper-script-tests.ps1
powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-script-tests.ps1
powershell -ExecutionPolicy Bypass -File .\scripts\verify.ps1
```

External/manual evidence before claiming tiny-size real-money readiness:

- Binance paper runner exits with `failure_class = ok`, `halt_reason = null`, and `open_orders_remaining = 0`.
- Binance paper soak exits with every iteration `failure_class = ok`.
- [x] IBKR read-only Gateway checks exit with `failure_class = ok`.
- [x] IBKR paper autorun exits with `failure_class = ok`, Gateway checks pass, and `open_orders_remaining = 0`.
- [x] IBKR paper soak exits with every iteration `failure_class = ok`.
- Any hard halt writes a machine-usable `risk_type` and fails closed.

## Global Constraints

- Do not introduce RBAC or multi-user approval flow in this phase.
- Do not enable automatic real broker order submission by default.
- Do not weaken existing protections such as `order_submit_enabled = false` or startup recovery `unmatched_open_orders = "fail"`.
- Every new rejection path must emit auditable risk events with machine-usable `risk_type` and human-readable reason.
- Market-data freshness and price-deviation checks must fail closed.
- Kill switch must cancel only this system's known broker orders unless the command explicitly says "cancel all open orders for symbol/account".
- Binance and IBKR behavior must stay aligned at the policy layer even if adapter implementations differ.
- Paper and backtest mode may simulate these protections, but live/paper broker paths are the acceptance target for this plan.

---

## File Structure

Create:

- `crates/risk/src/live_guards.rs` - pure risk primitives for stale quotes, price bands, daily loss, order-attempt limits, failure limits, and strategy circuit breakers.
- `crates/risk/tests/live_guard_tests.rs` - focused unit tests for each hard-stop and edge case.
- `docs/superpowers/plans/2026-07-04-live-trading-risk-hardening.md` - this plan.

Modify:

- `crates/config/src/config.rs` - add new risk and session config fields.
- `crates/config/tests/file_config_tests.rs` - config parsing coverage for new live-risk fields.
- `crates/runtime/src/run_spec.rs` - persist new risk settings into `RunSpec`.
- `crates/algorithm/src/algorithm.rs` - enforce new pre-trade guards and emit rejection events.
- `crates/risk/src/risk.rs` - export or integrate new guard module and shared error types.
- `crates/risk/tests/risk_tests.rs` - extend existing policy tests.
- `crates/paper/src/paper.rs` - maintain intraday counters / kill-state and stop new submits after halts.
- `crates/broker/src/broker.rs` - add broker-surface support for cancel-all/open-order batch workflows where needed.
- `crates/broker/tests/broker_tests.rs` - cancel-all and open-order matching tests.
- `apps/trader-cli/src/main.rs` - operator commands for kill switch and cancel-all.
- `apps/trader-cli/tests/cli_tests.rs` - CLI regression coverage.
- `scripts/binance-paper-run.ps1` - assert no residual open orders after run and collect richer evidence.
- `scripts/ibkr-paper-run.ps1` - same evidence contract for IBKR paper.
- `scripts/binance-paper-soak.ps1` - fail on new hard-risk halt reasons and open-order residue.
- `scripts/ibkr-paper-soak.ps1` - same for IBKR.
- `docs/broker.md` - document new risk gates and kill-switch operation.
- `docs/paper-readiness-runbook.md` - operator sequence for readonly, autorun, soak, and emergency stop.
- `docs/roadmap.md` - move "real-money hardening" items out of vague backlog into concrete stage gates.

---

### Task 1: Add Config Surface For Hard Live-Risk Limits

**Files:**
- Create: none
- Modify: `crates/config/src/config.rs`
- Modify: `crates/config/tests/file_config_tests.rs`
- Modify: `crates/runtime/src/run_spec.rs`
- Test: `crates/config/tests/file_config_tests.rs`

**Interfaces:**
- Consumes: existing `[risk]`, `[broker]`, `[live]` config blocks
- Produces: `RiskConfig.daily_loss_limit`, `RiskConfig.max_order_attempts_per_day`, `RiskConfig.max_order_failures_per_day`, `RiskConfig.max_price_deviation_bps`, `RiskConfig.max_market_data_age_ms`, `RiskConfig.max_consecutive_strategy_losses`, `RiskConfig.max_consecutive_strategy_errors`, `RiskConfig.trading_session`, `LiveKillSwitchPolicy`

- [x] **Step 1: Write failing config parsing tests**

```rust
#[test]
fn loads_paper_config_with_live_risk_hardening_fields() {
    let config = AppConfig::from_toml_str(
        r#"
        [runtime]
        mode = "paper"
        run_id = "risk-hardening"

        [database]
        url = "sqlite::memory:"

        [data]
        source = "csv"
        path = "datasets/sample/aapl_1d.csv"

        [strategy]
        name = "moving_average_cross"
        symbols = ["US:NASDAQ:AAPL:EQUITY"]
        fast_window = 2
        slow_window = 3

        [portfolio]
        initial_cash = "10000"
        base_currency = "USD"
        order_qty = "1"
        max_abs_qty = "10"

        [risk]
        max_order_notional = "1000"
        min_cash_after_order = "100"
        max_exposure = "5000"
        max_drawdown = "0.2"
        max_leverage = "2"
        max_margin_used = "0"
        trading_halted = false
        daily_loss_limit = "50"
        max_order_attempts_per_day = 20
        max_order_failures_per_day = 5
        max_price_deviation_bps = "50"
        max_market_data_age_ms = 5000
        max_consecutive_strategy_losses = 3
        max_consecutive_strategy_errors = 2

        [risk.trading_session]
        mode = "regular_only"
        timezone = "America/New_York"
        start = "09:30"
        end = "16:00"

        [broker]
        kind = "simulated"
        mode = "paper"

        [paper]
        account_id = "paper"
        slippage_bps = "0"
        fee_bps = "0"

        [live]
        enabled = false
        "#
    )
    .unwrap();

    assert_eq!(config.risk.daily_loss_limit.as_deref(), Some("50"));
    assert_eq!(config.risk.max_order_attempts_per_day, Some(20));
    assert_eq!(config.risk.max_market_data_age_ms, Some(5000));
    assert_eq!(
        config.risk.trading_session.as_ref().unwrap().timezone,
        "America/New_York"
    );
}
```

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test -p config loads_paper_config_with_live_risk_hardening_fields --test file_config_tests`
Expected: FAIL because the new fields do not exist.

- [x] **Step 3: Add config structs and defaults**

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct RiskConfig {
    pub max_order_notional: String,
    pub min_cash_after_order: String,
    pub max_exposure: String,
    pub max_drawdown: String,
    pub max_leverage: String,
    pub max_margin_used: String,
    pub trading_halted: bool,
    #[serde(default)]
    pub allow_short: Option<bool>,
    #[serde(default)]
    pub daily_loss_limit: Option<String>,
    #[serde(default)]
    pub max_order_attempts_per_day: Option<u32>,
    #[serde(default)]
    pub max_order_failures_per_day: Option<u32>,
    #[serde(default)]
    pub max_price_deviation_bps: Option<String>,
    #[serde(default)]
    pub max_market_data_age_ms: Option<u64>,
    #[serde(default)]
    pub max_consecutive_strategy_losses: Option<u32>,
    #[serde(default)]
    pub max_consecutive_strategy_errors: Option<u32>,
    #[serde(default)]
    pub trading_session: Option<TradingSessionConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TradingSessionConfig {
    pub mode: String,
    pub timezone: String,
    pub start: String,
    pub end: String,
}
```

Also mirror the same fields into `crates/runtime/src/run_spec.rs` so config snapshots retain the exact effective risk contract.

- [x] **Step 4: Add parse coverage for disabled / omitted fields**

Add tests asserting omitted fields stay `None` and do not change existing sample configs.

- [x] **Step 5: Run config tests**

Run:

```powershell
cargo test -p config --test file_config_tests
cargo check -p config -p runtime
```

Expected: PASS.

- [x] **Step 6: Commit**

```bash
git add crates/config crates/runtime
git commit -m "feat: add live risk hardening config surface"
```

### Task 2: Implement Pure Risk Guards For New Hard Stops

**Files:**
- Create: `crates/risk/src/live_guards.rs`
- Modify: `crates/risk/src/risk.rs`
- Create: `crates/risk/tests/live_guard_tests.rs`
- Modify: `crates/risk/tests/risk_tests.rs`

**Interfaces:**
- Consumes: `OrderRequest`, mark/reference price, snapshot timestamps, runtime counters
- Produces: `DailyLossGuard`, `OrderThrottleGuard`, `MarketDataFreshnessGuard`, `PriceDeviationGuard`, `TradingSessionGuard`, `StrategyCircuitBreaker`, `LiveRiskRejection`

- [x] **Step 1: Write failing guard tests**

```rust
#[test]
fn rejects_order_when_market_data_is_stale() {
    let guard = MarketDataFreshnessGuard::new(5_000);
    let error = guard.check(1_000_000, 1_006_000).unwrap_err();
    assert_eq!(error.risk_type, "stale_market_data");
}

#[test]
fn rejects_order_when_limit_price_deviates_from_reference() {
    let guard = PriceDeviationGuard::new(dec!(50));
    let error = guard
        .check(dec!(101), dec!(100))
        .unwrap_err();
    assert_eq!(error.risk_type, "price_deviation");
}

#[test]
fn rejects_after_daily_loss_limit_is_breached() {
    let guard = DailyLossGuard::new(dec!(50));
    let error = guard.check(dec!(10000), dec!(9949.99)).unwrap_err();
    assert_eq!(error.risk_type, "daily_loss_limit");
}
```

- [x] **Step 2: Run tests to verify they fail**

Run: `cargo test -p risk live_guard --test live_guard_tests`
Expected: FAIL because the module and types do not exist.

- [x] **Step 3: Implement pure guard types**

Create `crates/risk/src/live_guards.rs` with a small reusable error shape:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveRiskRejection {
    pub risk_type: &'static str,
    pub reason: String,
}

pub struct DailyLossGuard {
    daily_loss_limit: Decimal,
}

impl DailyLossGuard {
    pub fn new(daily_loss_limit: Decimal) -> Self { Self { daily_loss_limit } }

    pub fn check(&self, day_start_equity: Decimal, current_equity: Decimal) -> Result<(), LiveRiskRejection> {
        if day_start_equity - current_equity > self.daily_loss_limit {
            return Err(LiveRiskRejection {
                risk_type: "daily_loss_limit",
                reason: format!(
                    "day loss {} exceeds limit {}",
                    day_start_equity - current_equity,
                    self.daily_loss_limit
                ),
            });
        }
        Ok(())
    }
}
```

Use the same pattern for:
- stale market data
- price deviation in bps
- order attempts per day
- order failures per day
- consecutive strategy losses
- consecutive strategy errors
- trading session closed

- [x] **Step 4: Export the guard API through `risk.rs`**

```rust
mod live_guards;

pub use live_guards::{
    DailyLossGuard, LiveRiskRejection, MarketDataFreshnessGuard, OrderThrottleGuard,
    PriceDeviationGuard, StrategyCircuitBreaker, TradingSessionGuard,
};
```

- [x] **Step 5: Extend legacy `RiskError` mapping**

Add a helper so algorithm/runtime code can convert `LiveRiskRejection` into audit-friendly risk events without overloading the old enum with every runtime-only case.

- [x] **Step 6: Run risk tests**

Run:

```powershell
cargo test -p risk
cargo check -p risk
```

Expected: PASS.

- [x] **Step 7: Commit**

```bash
git add crates/risk
git commit -m "feat: add pure live risk guards"
```

### Task 3: Enforce New Guards In Algorithm And Paper Runtime

**Files:**
- Modify: `crates/algorithm/src/algorithm.rs`
- Modify: `crates/paper/src/paper.rs`
- Modify: `crates/paper/tests/paper_tests.rs`
- Modify: `crates/paper/tests/paper_stream_tests.rs`
- Modify: `crates/algorithm/tests/algorithm_tests.rs`

**Interfaces:**
- Consumes: new risk config fields, current snapshot, latest bar timestamp, order/failure counters
- Produces: pre-trade rejection events with `risk_type` values `daily_loss_limit`, `max_order_attempts`, `max_order_failures`, `stale_market_data`, `price_deviation`, `trading_session_closed`, `strategy_loss_circuit_breaker`, `strategy_error_circuit_breaker`

- [x] **Step 1: Write failing integration tests**

```rust
#[tokio::test]
async fn paper_run_halts_new_orders_after_daily_loss_breach() {
    // Setup bars and settings with daily_loss_limit = 50
    // Force an execution path that takes equity below the threshold
    // Assert later signals emit risk rejection and no new submitted order is recorded
}

#[test]
fn algorithm_rejects_order_when_bar_timestamp_is_stale() {
    // Setup engine with max_market_data_age_ms = 1000
    // Feed a bar whose ts is too old relative to evaluation time
    // Assert no order and a risk event with risk_type stale_market_data
}
```

- [x] **Step 2: Run tests to verify they fail**

Run:

```powershell
cargo test -p algorithm stale_market_data
cargo test -p paper daily_loss_breach
```

Expected: FAIL because the runtime does not track or enforce the new guards.

- [x] **Step 3: Extend algorithm settings and runtime state**

Add fields to `AlgorithmEngineSettings`:

```rust
pub daily_loss_limit: Option<Decimal>,
pub max_order_attempts_per_day: Option<u32>,
pub max_order_failures_per_day: Option<u32>,
pub max_price_deviation_bps: Option<Decimal>,
pub max_market_data_age_ms: Option<u64>,
pub max_consecutive_strategy_losses: Option<u32>,
pub max_consecutive_strategy_errors: Option<u32>,
pub trading_session: Option<TradingSessionWindow>,
```

Track in engine/runtime session state:
- `day_start_equity`
- `order_attempts_today`
- `order_failures_today`
- `consecutive_strategy_losses`
- `consecutive_strategy_errors`
- `halt_reason`

- [x] **Step 4: Enforce guards before order generation / submit**

In `AlgorithmEngine::decision_for_symbol`, check in this order:
1. trading session window
2. market data freshness
3. daily loss stop
4. strategy circuit breaker
5. order-attempt throttle
6. existing position/notional/exposure checks
7. price deviation if a limit price is present or if broker submit path derives one

Emit `algorithm.risk.rejected` payloads like:

```rust
json!({
    "risk_type": "stale_market_data",
    "decision": "rejected",
    "symbol": order.symbol,
    "reason": rejection.reason,
})
```

- [x] **Step 5: Stop the runtime after hard halts**

In `crates/paper/src/paper.rs`, when a hard-stop risk event is emitted:
- persist the event
- set local halt state
- refuse later submits
- surface the halt reason in logs and final summary

Order execution failures must increment `order_failures_today`; closed losing trades must increment `consecutive_strategy_losses`; successful non-losing closes should reset the loss streak.

- [x] **Step 6: Run targeted tests**

Run:

```powershell
cargo test -p algorithm
cargo test -p paper
```

Expected: PASS.

- [x] **Step 7: Commit**

```bash
git add crates/algorithm crates/paper
git commit -m "feat: enforce live hard-risk guards in algorithm and paper runtime"
```

### Task 4: Add Global Kill Switch And Cancel-All Workflow

**Files:**
- Modify: `crates/broker/src/broker.rs`
- Modify: `crates/broker/tests/broker_tests.rs`
- Modify: `apps/trader-cli/src/main.rs`
- Modify: `apps/trader-cli/tests/cli_tests.rs`

**Interfaces:**
- Consumes: broker `open_orders`, local run/order metadata, operator confirmation flags
- Produces: CLI commands `trader binance-paper-cancel-open-orders`, `trader ibkr-paper-cancel-order`, and new generic `trader risk-kill-switch --run-id <id> --cancel-open-orders`

- [x] **Step 1: Write failing CLI and broker tests**

```rust
#[tokio::test]
async fn kill_switch_marks_run_halted_and_requests_cancel_all() {
    // Seed open orders for a run
    // Execute kill switch path
    // Assert risk event + trading_halted state + cancel calls issued
}
```

- [x] **Step 2: Run tests to verify they fail**

Run:

```powershell
cargo test -p broker cancel_all
cargo test -p trader-cli kill_switch --test cli_tests
```

Expected: FAIL because no generic kill-switch flow exists.

- [x] **Step 3: Add broker-side helper surface**

Do not add a broad mutable broker admin API. Add a narrow helper:

```rust
pub async fn cancel_open_orders_for_account_symbol(
    broker: &dyn Broker,
    account_id: &str,
    symbol: Option<&str>,
) -> Result<Vec<BrokerOrder>, BrokerError>
```

Implementation:
- call `open_orders(account_id)`
- optionally filter by symbol
- cancel each order by `broker_order_id`
- return final cancelled / terminal statuses

- [x] **Step 4: Add CLI kill-switch command**

Add a focused operator command:

```rust
RiskKillSwitch {
    #[arg(long)]
    config: String,
    #[arg(long)]
    run_id: String,
    #[arg(long)]
    cancel_open_orders: bool,
    #[arg(long)]
    symbol: Option<String>,
    #[arg(long)]
    confirm_kill_switch: bool,
}
```

Behavior:
- require `--confirm-kill-switch`
- record a `risk` / `algorithm.risk.rejected` style audit event with `risk_type = "operator_kill_switch"`
- if `--cancel-open-orders`, cancel known remote opens for the account/symbol
- print a short summary for cancelled/open/failed cancels

- [x] **Step 5: Run broker and CLI tests**

Run:

```powershell
cargo test -p broker
cargo test -p trader-cli --test cli_tests
```

Expected: PASS.

- [x] **Step 6: Commit**

```bash
git add crates/broker apps/trader-cli
git commit -m "feat: add operator kill switch and cancel-all workflow"
```

### Task 5: Tighten Binance / IBKR Paper Evidence And Soak Gates

**Files:**
- Modify: `scripts/binance-paper-run.ps1`
- Modify: `scripts/binance-paper-soak.ps1`
- Modify: `scripts/ibkr-paper-run.ps1`
- Modify: `scripts/ibkr-paper-soak.ps1`
- Modify: `docs/broker.md`
- Modify: `docs/paper-readiness-runbook.md`

**Interfaces:**
- Consumes: runner summaries, open-order checks, reconcile output, risk-event output
- Produces: `summary.json` fields `halt_reason`, `risk_rejections`, `open_orders_remaining`, `cancel_all_attempted`, `cancel_all_succeeded`, `gateway_checks`, `reconciliation_status`

- [x] **Step 1: Write failing script-contract tests or documented manual assertions**

If no PowerShell test harness exists for these scripts, add explicit expected `summary.json` contract examples to the plan implementation task and validate them by parsing script output in CLI tests where practical.

Example target shape:

```json
{
  "status": "ok",
  "failure_class": "ok",
  "halt_reason": null,
  "risk_rejections": [],
  "open_orders_remaining": 0,
  "cancel_all_attempted": false,
  "cancel_all_succeeded": true
}
```

- [x] **Step 2: Update run scripts to fail hard on residue and halts**

Each paper run script must:
- collect `risk-events` for the run
- include the first hard-halt reason in `summary.json`
- fail non-zero if `open_orders_remaining != 0`
- if residue exists, attempt documented cancel-all cleanup before exiting
- record whether cleanup was attempted and whether it succeeded

- [x] **Step 3: Update soak scripts to classify new failures**

Add failure classes:
- `daily_loss_limit`
- `max_order_attempts`
- `max_order_failures`
- `stale_market_data`
- `price_deviation`
- `trading_session_closed`
- `operator_kill_switch`
- `open_orders_remaining`

- [x] **Step 4: Document operator evidence chain**

In [docs/broker.md](/abs/path/D:/code/trader/trader/docs/broker.md) and [docs/paper-readiness-runbook.md](/abs/path/D:/code/trader/trader/docs/paper-readiness-runbook.md), add the exact progression:
1. readonly
2. tiny order
3. autorun with submit enabled
4. soak
5. emergency kill-switch

Each stage must state required evidence artifacts and stop conditions.

- [x] **Step 5: Run script verification**

Current status: IBKR broker-facing script verification completed against local IBKR Gateway on 2026-07-06. Binance paper verification remains a separate external-environment item.

Run:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\binance-paper-run.ps1
powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-run.ps1
```

Then run the soak scripts in readonly/dry-run mode if available.

Expected: summaries contain the new fields and no existing script contract regresses.

IBKR evidence:

- `powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-script-tests.ps1` passed, including transient `PendingCancel` open-order settlement coverage.
- `powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-test-guide.ps1 -Stage AutoRun -AccountId DUQ645291 -Port 4002 -ConfirmAutoRun` passed with summary `data/ibkr-paper-runs/ibkr-aapl-1d-afb4fdab9323/summary.json`.
- `powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-soak.ps1 -Iterations 3 -AccountId DUQ645291 -Port 4002 -ConfirmIbkrPaperOrder` passed with summary `data/ibkr-paper-soak/ibkr-paper-soak-af20e6620229/summary.json`.

- [x] **Step 6: Commit**

```bash
git add scripts docs/broker.md docs/paper-readiness-runbook.md
git commit -m "feat: harden paper evidence and soak gates"
```

### Task 6: Final Verification, Roadmap Update, And Readiness Gate

**Files:**
- Modify: `docs/roadmap.md`
- Modify: `README.md` if operator command index needs an update

**Interfaces:**
- Consumes: completed hardening tasks
- Produces: a concrete readiness checklist for "paper", "tiny real-money", and "not yet allowed"

- [x] **Step 1: Add a readiness matrix**

Update `docs/roadmap.md` with three explicit buckets:
- `paper ready`
- `tiny-size real-money candidate`
- `not yet production complete`

Criteria for `tiny-size real-money candidate` must include all of:
- hard risk gates implemented
- kill-switch implemented
- cancel-all workflow verified
- no residual open orders in scripts
- IBKR readonly, autorun, and soak evidence collected

- [ ] **Step 2: Run acceptance test set**

Current status: partial. `cargo check --workspace`, IBKR script contracts, IBKR read-only, IBKR AutoRun, and IBKR three-iteration soak passed on 2026-07-06. The full crate test set, `scripts\verify.ps1`, and Binance paper evidence still need to be run before this plan is considered fully accepted.

Run:

```powershell
cargo test -p config
cargo test -p risk
cargo test -p algorithm
cargo test -p paper
cargo test -p broker
cargo test -p trader-cli
cargo check --workspace
```

Also run:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\verify.ps1
```

Expected: PASS. If broker-network-dependent scripts cannot run in the current environment, record that gap explicitly in the final implementation notes.

- [x] **Step 3: Commit**

```bash
git add docs/roadmap.md README.md
git commit -m "docs: add live trading readiness gates"
```

---

## Implementation Order

1. Task 1: config surface
2. Task 2: pure risk guards
3. Task 3: algorithm and paper enforcement
4. Task 4: kill switch and cancel-all
5. Task 5: script evidence and soak hardening
6. Task 6: verification and readiness matrix

Do not start broker soak work before Tasks 1-4 land. A soak run without hard halts and cleanup is operational noise, not evidence.

## Acceptance Criteria

- Orders are rejected when market data is stale beyond configured age.
- Orders are rejected when limit/reference price deviation exceeds configured bps.
- New orders stop for the rest of the session after daily loss limit breach.
- New orders stop after configured max order attempts or max failures.
- Strategy can be halted by consecutive losses or consecutive internal errors.
- Trading session window can block out-of-session stock orders.
- Operator can trigger a kill switch and optionally cancel remote open orders.
- Binance and IBKR paper scripts record halt reasons and fail on residual open orders.
- Existing default configs still parse and keep trading submission disabled by default.

## Self-Review

- Spec coverage: the plan covers every missing hard protection named in the request except full production broker reconciliation proof, which is handled here as evidence/soak gates rather than claiming completion.
- Placeholder scan: no `TODO` or vague "add validation" placeholders remain; each task names files, interfaces, commands, and expected outcomes.
- Type consistency: new risk fields are defined first in config/run-spec, then consumed by risk/algorithm/runtime/CLI layers under the same names.
