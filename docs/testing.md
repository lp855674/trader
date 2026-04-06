# Testing Guide

**689 tests, all green.** This document explains what each module tests, why those tests
matter for business correctness, and how the full end-to-end flow is verified.

---

## Quick Reference

```bash
# Run everything
cargo test --workspace

# Run one crate
cargo test -p exec
cargo test -p risk
cargo test -p strategy
cargo test -p marketdata
cargo test -p infra

# Run a specific test
cargo test -p exec full_lifecycle

# Run benchmarks (requires nightly or criterion)
cargo bench -p exec
cargo bench -p strategy
cargo bench -p marketdata
cargo bench -p infra

# End-to-end pipeline smoke test
cargo test -p quantd four_venues_minimal_closed_loop
```

---

## Module-by-Module

### `crates/domain` — Shared Types (2 tests)

The foundation everything else builds on. Tests here verify that the `InstrumentId`
type serializes to and from JSON without losing information, and that its `Display`
output is stable (used as database keys and map keys across the whole system).

```
instrument_id_display          → "CRYPTO:BTC-USD" format is stable
instrument_id_roundtrip_json   → serde round-trip preserves venue + symbol
```

**Business rule:** Every order, position, fill, and bar references an `InstrumentId`.
If this type is broken, nothing else can be trusted.

---

### `crates/db` — Database Layer (1 test)

```
mock_ingest_inserts_one_bar_row   → SQLite schema migrates, a bar row is written and read back
```

The test spins up an in-memory SQLite database, runs all migrations from
`crates/db/migrations/` in order, inserts a bar, and reads it back. This is the
baseline proof that the schema is consistent and the migration sequence is correct.

**Business rule:** Data written must be readable. If migrations are broken, the system
cannot start in production.

---

### `crates/strategy` — Signal Generation (241 tests)

The strategy crate is the most test-heavy because it is the most mathematically
intensive. Tests are split across unit, integration, and backtest layers.

#### Core Framework

```
strategy::tests::long_one_when_bar_present   → AlwaysLongOne emits Signal::Long(1.0) when a bar arrives
strategy::tests::no_signal_without_bar       → no bar → no signal (pure function guarantee)
```

**Business rule:** Strategies must be pure functions. Same input, same output, always.
`test_determinism_same_result_two_runs` and `test_concurrent_runs_identical_results`
verify this explicitly — both run the backtest engine twice on the same data and assert
byte-for-byte identical equity curves.

#### Backtest Engine

```
test_single_bar_equity_snapshot         → one bar produces one equity snapshot
test_empty_bars_preserves_capital       → no bars → capital unchanged
test_insufficient_capital_position_not_opened  → can't open a position you can't afford
test_determinism_same_result_two_runs   → two runs on identical data → identical results
test_concurrent_runs_identical_results  → parallel runs → identical results
```

**Business rule:** The backtest engine enforces capital constraints. A position is only
opened when available capital covers the full notional. The
`test_insufficient_capital_position_not_opened` test is the guard against overfitting
a strategy to a backtest that assumed infinite capital.

#### Statistical Analysis

The `analysis::` module has 60+ tests covering:

| Sub-module | What it tests |
|---|---|
| `correlation` | Pearson/Spearman correlation, diversification score |
| `risk` | Historical VaR, parametric VaR, CVaR, Cornish-Fisher, max drawdown, Ulcer index |
| `stress` | GFC scenario, black-swan drawdown, liquidity stress |
| `monte_carlo` | Bootstrap path generation, determinism with same seed, percentile ordering |
| `sensitivity` | Parameter delta, robustness flags, scenario analysis sorted by VaR |
| `walk_forward` | Efficiency, drift detection, consistency fraction |
| `outliers` | IQR and z-score outlier detection |
| `normalize` | MinMax, z-score, log-transform round-trips |
| `cv` | K-fold, purged k-fold (no look-ahead), walk-forward k-fold |

The purged k-fold test (`analysis::cv::tests::purged_kfold_has_gap`) is particularly
important: it verifies that a gap exists between training and test folds so that
time-series correlation does not leak future data into the training set.

**Business rule:** VaR calculations must be conservative. `cvar_is_worse_than_var`
checks that CVaR (Expected Shortfall) is always greater than VaR at the same confidence
level — if this fails, the risk reporting math is wrong.

#### Monte Carlo

```
analysis::monte_carlo::tests::simulation_deterministic_with_same_seed
analysis::monte_carlo::tests::percentiles_are_sorted
analysis::monte_carlo::tests::prob_profit_in_range
```

Uses an LCG (no external `rand` crate). The determinism test seeds the RNG with the
same value twice and asserts identical path output. The percentile test asserts that
P5 <= P50 <= P95 — a sanity check on the simulation math.

#### Optimizer (Bayesian + Random)

```
api::optimizer::tests::bayesian_job_completes
api::optimizer::tests::random_job_completes
api::optimizer::tests::cancel_job_marks_failed
```

**Business rule:** A cancelled optimization job must not produce results. The
`cancel_job_marks_failed` test verifies the state machine transition.

---

### `crates/risk` — Risk Management (111 tests)

Risk is the gatekeeper between strategy signals and order submission. The tests here
verify that bad orders are rejected before they reach the market.

#### Unit Tests (risk_unit_tests.rs)

```
risk::position::tests::hard_stop_triggers              → position beyond loss limit → reject
risk::position::tests::hard_stop_not_triggered_within_limit
risk::position::tests::trailing_stop_follows_peak      → trailing stop ratchets up with price
risk::position::tests::trailing_stop_not_triggered_within_limit
risk::position::tests::daily_pnl_reset                 → daily loss counter resets at midnight
risk::portfolio::tests::var_budget_breach_rejects      → exceeding VaR budget → reject
risk::rules::tests::load_from_json                     → rules hot-reload from JSON config
risk::rules::tests::hot_reload_swaps_rules             → new config takes effect mid-run
```

**Business rule:** The trailing stop must ratchet upward but never downward. If
`trailing_stop_follows_peak` breaks, a winning position could have its stop moved
backward, exposing the account to larger losses than intended.

#### Integration Tests (risk_integration_tests.rs)

```
all_checkers_approve_valid_order            → valid BTC order within all limits → Approved
position_manager_stop_prevents_new_orders  → stop loss hit → subsequent orders rejected
circuit_breaker_short_circuits_portfolio_check  → open circuit → fast reject, no computation
var_budget_exhausted_rejects               → VaR limit consumed → new order blocked
concurrent_access_no_panics               → multiple threads checking simultaneously
```

The `concurrent_access_no_panics` test runs 100 concurrent risk checks in parallel.
This is not just "does it crash" — it verifies there are no data races on the shared
`PortfolioRiskChecker` state.

**Business rule:** Circuit breaker must short-circuit. When the circuit is open, the
system must reject immediately without computing VaR. This protects against cascading
failures during market dislocations.

#### System Test (risk_system_test.rs)

```
full_pipeline_integration   → signal → risk check → order decision → alert → report
stress_test_gfc_scenario    → GFC historical stress test produces significant loss
load_config_from_json       → risk config deserializes and activates correctly
```

`full_pipeline_integration` is the closest to end-to-end within the risk crate: a
signal arrives, goes through all checkers, the decision is recorded, and a report is
generated. It verifies the whole chain without spinning up exec or marketdata.

#### Advanced Tests (risk_advanced_tests.rs)

```
alert_pipeline_var_breach           → VaR breach triggers alert
liquidity_stress_widens_prices      → illiquid market → wider bid-ask → higher cost model
data_quality_flags_anomalous_price  → stale or zero price flagged before risk calc
gfc_stress_produces_significant_loss → GFC weights produce >50% drawdown on leveraged portfolio
```

**Business rule:** Data quality must be checked before risk calculations. The
`data_quality_flags_anomalous_price` test ensures that a stale price (e.g., an exchange
feed that stopped publishing) is flagged rather than silently used in VaR, which could
understate risk.

---

### `crates/exec` — Order Execution (137 tests)

Execution is the most state-machine-heavy crate. Tests verify order lifecycle
transitions, persistence integrity, and algorithmic order behaviour.

#### Order State Machine (execution_core_tests.rs)

```
full_lifecycle   → submit → partial fill → full fill → position updated
```

This test walks an order through every valid state transition:
`Pending → Submitted → PartiallyFilled → Filled`. It then verifies that
`ExecPositionManager` reflects the correct net quantity and average cost after both
partial and full fills. This is the single most important test in the execution crate.

**Business rule:** Average fill price must be computed correctly using VWAP across
partial fills. A wrong average cost means wrong P&L and wrong risk exposure reported
upstream.

#### Algorithmic Orders (execution_advanced_tests.rs)

```
trailing_stop_follows_price_up_triggers_on_reversal  → price 1000→1200→1300 → stop at 1250 → triggered at 1250
iceberg_replenishes_and_completes                    → 9 total / 3 display → 3 refills → complete
twap_schedules_slices_at_correct_intervals           → 4 slices over 4000ms → fires at 1000, 2000, 3000, 4000ms
batch_queue_respects_rate_limit                      → 3 req/sec limit → batches of 3
priority_queue_ordering                              → Urgent → Normal → Delayed, FIFO within each tier
```

**Business rule for iceberg:** `remaining()` must reach zero exactly when `is_complete()`
is true. If these two diverge, the order lifecycle never terminates cleanly and the
position will show an open order against a fully filled quantity.

**Business rule for TWAP:** The interval fires at `start_ts + interval_ms`, not at
`start_ts`. Getting this off by one means every slice executes at the wrong time,
which defeats the TWAP's purpose of reducing market impact.

#### Persistence (persistence_tests.rs)

```
wal_replay_reconstructs_order_manager    → WAL entries → identical OrderManager on replay
snapshot_json_restore_identical          → snapshot → JSON → restore → same order IDs
fill_repository_dedup                    → duplicate fill insert returns false, count stays 1
position_repository_history_since        → time-filtered snapshot history
query_index_lookup_by_instrument         → instrument key → correct order ID set
wal_durability_status                    → 2 entries + checkpoint → unchecked=0
data_corruption_handling_invalid_json    → corrupted snapshot JSON → Err, not panic
incremental_backup_returns_only_new_snapshots → incremental since t=1500 returns only t=2000, t=3000
```

**Business rule — dedup:** A fill must only be applied once. If the same fill is
applied twice, net quantity doubles, P&L doubles, and the account shows twice the
exposure it actually has. `fill_repository_dedup` is the guard.

**Business rule — WAL:** After a checkpoint, `unchecked` must be 0. If durability
status incorrectly reports unchecked entries after a checkpoint, the system will
unnecessarily replay history on startup, which is slow and can cause incorrect
position reconstruction.

#### API Layer (api_integration_tests.rs)

```
submit_order_http    → POST /orders → 200, order in manager
cancel_order_grpc    → cancel RPC → order state Cancelled
position_query       → positions endpoint → correct instrument/qty map
webhook_register     → register webhook URL → stored
```

#### Monitoring (monitoring_tests.rs)

```
tracer_records_full_order_lifecycle  → submit/fill/cancel all traced
tracer_avg_duration_correct          → avg latency computed correctly
alert_escalation_triggers            → alert not acknowledged → escalation fires
alert_dedup                          → same alert within window → sent once
```

#### Production Config (prod_tests.rs)

```
config_default_values_are_valid   → default config passes validation
config_json_roundtrip             → serialize → deserialize → identical
shutdown_flag_starts_false        → system not shutting down on start
shutdown_request_sets_flag        → signal sets atomic bool
invalid_config_fails_validation   → negative order size → validation error
```

**Business rule:** Invalid configuration must be rejected at startup, not discovered
at runtime when an order is submitted. `invalid_config_fails_validation` is the
gate.

---

### `crates/marketdata` — Data Pipeline (118 tests)

#### Core Data Types (data_core_tests.rs)

```
tick_aggregator_assembles_bars       → ticks → OHLCV bars at correct granularity
align::tests::downsample_5_to_1     → 5-minute bars → 1-minute via downsampling
align::tests::gap_detection          → missing periods flagged
align::tests::linear_interpolation   → gap filled by interpolation
align::tests::forward_fill_creates_missing → NaN filled forward
```

**Business rule:** A bar's high must be >= open, close, and low. The aggregator tests
verify OHLCV invariants hold after every tick is processed. A bar with high < low would
propagate corrupt data into every downstream calculation.

#### Storage (data_core_tests.rs, storage tests)

```
storage::batch::tests::batch_flush_on_size      → buffer reaches 100 → flush to SQLite
storage::batch::tests::batch_flush_on_interval  → 500ms elapsed → flush regardless of size
storage::sqlite::tests::partitioned_storage_query → query by instrument + time range
storage::index::tests::index_and_lookup         → fast index lookup vs. full scan
```

**Business rule:** Batch flush on interval prevents data loss on low-volume instruments.
If the only flush trigger is batch size, a quiet instrument accumulates data in memory
and loses it on crash. The interval-based flush is the safety net.

#### Replay and Quality (replay_quality_tests.rs)

```
alert_manager_triggers_on_low_cache_hit  → cache miss rate > threshold → alert
```

**Business rule:** Cache hit rate below threshold means every bar lookup goes to disk,
which blows the <10ms query latency SLA. The alert is the early warning.

#### Infrastructure (infrastructure_tests.rs)

```
cache::mmap::tests::out_of_bounds_returns_error   → bad offset → Err, not segfault
data_api::http::tests::invalid_json_returns_error → malformed request body → 400
parser::file::tests::csv_missing_column_returns_error → incomplete CSV → Err
```

---

### `crates/infra` — Infrastructure (68 tests)

#### Integration Tests (integration_tests.rs)

```
metrics_counter_accumulates       → counter increments correctly across calls
shutdown_lifecycle_completes      → Running → Initiated → DrainConnections → SaveState → Complete
watchdog_detects_timeout          → service misses heartbeat deadline → flagged unhealthy
circuit_breaker_opens_on_failures → 5 consecutive failures → circuit opens
service_registry_finds_healthy    → unhealthy service excluded from results
```

The `shutdown_lifecycle_completes` test verifies the five-phase shutdown sequence runs
in order and does not skip phases. Skipping `SaveState` before `Complete` means position
state is lost on restart.

#### Chaos Tests (chaos_tests.rs)

Uses a deterministic LCG RNG to inject random failures:

```
random_failures_circuit_breaker_recovers     → random failures → circuit opens → half-open → closed
resource_exhaustion_cleanup_handles_many_resources → 1000 resources → all cleaned up in order
concurrent_shutdown_phases_complete_safely   → concurrent phase transitions → no deadlock
watchdog_multiple_services_partial_failure   → some services healthy, some not → correct split
service_registry_flapping_health             → services toggle health → registry stays consistent
```

**Business rule for circuit breaker:** After recovery (HalfOpen → Closed), the system
must resume accepting requests. The chaos test runs 50 rounds of random failures and
verifies the circuit always returns to Closed eventually. If it gets stuck Open, all
downstream orders are blocked permanently.

#### Services

```
services::circuit::tests::opens_after_threshold_failures
services::circuit::tests::half_open_to_closed_on_successes
services::circuit::tests::half_open_to_open_on_failure
services::balance::tests::round_robin_cycles
services::balance::tests::least_connections_picks_lowest
services::balance::tests::unhealthy_node_excluded
services::risk::tests::rejected_on_notional_breach
services::execution::tests::submit_fill_reject_lifecycle
```

---

## End-to-End Flow Test

**File:** `crates/quantd/tests/four_venues_mvp.rs`
**Test:** `four_venues_minimal_closed_loop`

This is the single test that exercises the entire system in one shot. It is the
business-correctness test for the full pipeline.

### What it does

```
DB (SQLite in-memory)
  → seed schema + account
  → for each of [UsEquity, HkEquity, Crypto, Polymarket]:
      IngestRegistry → MockBarsAdapter → one bar
      AlwaysLongOne strategy → Signal::Long(1.0)
      RiskLimits::default() → approved
      PaperAdapter → order placed → fill applied
  → assert: 4 orders in exec_orders for account "acc_mvp_paper"
```

### Why each assertion matters

| Assertion | Business rule |
|---|---|
| `count == 4` | One order per venue. No venue was skipped. No order was duplicated. |
| No `expect()` panics | The full pipeline does not crash on any venue |
| `db::count_orders_for_account` reads from DB | The paper adapter actually persisted the order — it is not just in memory |

### How to extend this test

When adding a new venue or changing the pipeline (e.g., adding a pre-trade risk
check), this test is where you add the corresponding assertion. If a new risk check
blocks all orders, `count` will drop below 4 and the test will fail immediately.

---

## Business Rules Summary

| Rule | Test |
|---|---|
| Same input → same output from strategy | `test_determinism_same_result_two_runs` |
| Can't open position without capital | `test_insufficient_capital_position_not_opened` |
| CVaR >= VaR at same confidence | `analysis::risk::tests::cvar_is_worse_than_var` |
| Trailing stop only moves in one direction | `risk::position::tests::trailing_stop_follows_peak` |
| Daily P&L limit resets at midnight | `risk::position::tests::daily_pnl_reset` |
| Circuit open → fast reject, no computation | `circuit_breaker_short_circuits_portfolio_check` |
| Duplicate fill rejected | `fill_repository_dedup` |
| WAL unchecked = 0 after checkpoint | `wal_durability_status` |
| Iceberg remaining == 0 when complete | `iceberg_replenishes_and_completes` |
| TWAP fires at start + interval, not at start | `twap_schedules_slices_at_correct_intervals` |
| Bar high >= open, close, low | `tick_aggregator_assembles_bars` |
| Invalid config rejected at startup | `invalid_config_fails_validation` |
| Cancelled job produces no results | `api::optimizer::tests::cancel_job_marks_failed` |
| Purged k-fold has gap (no look-ahead) | `analysis::cv::tests::purged_kfold_has_gap` |
| 4 venues × 1 order = 4 rows in DB | `four_venues_minimal_closed_loop` |

---

## Benchmarks

| Benchmark file | What it measures |
|---|---|
| `exec/benches/quality_bench.rs` | Fixed/volume/depth slippage computation throughput |
| `strategy/benches/strategy_bench.rs` | Signal evaluation latency, backtest throughput |
| `marketdata/benches/data_bench.rs` | Correlation matrix, bar ingestion, diversification score |
| `infra/benches/infra_bench.rs` | Metrics throughput, circuit breaker, watchdog tick, load balancer, shutdown |

Run with:
```bash
cargo bench -p exec 2>&1 | grep "time:"
cargo bench -p strategy 2>&1 | grep "time:"
```

Benchmarks are not run in CI by default (they take too long). Run them before and after
any change that touches a hot path (slippage, risk check, bar aggregation).

---

## Adding Tests

**Unit test:** Add `#[cfg(test)] mod tests { ... }` inside the source file, next to
the code it tests. Use this for pure functions and data structure invariants.

**Integration test:** Add a file under `crates/<crate>/tests/`. Use this for
multi-component workflows (e.g., order manager + position manager + WAL together).

**Business rule test:** If you add a new rule (risk limit, order constraint, data
quality check), add a test named after the rule. Put it in the integration test file
for the crate that owns the rule. Reference the test name in this document.

**End-to-end test:** Modify `four_venues_mvp.rs` or add a sibling file in
`crates/quantd/tests/`. This is the right place for tests that span DB + ingest +
strategy + risk + exec.
