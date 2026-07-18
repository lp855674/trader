# Live Recovery Long Run Verification Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a repeatable long-run verification loop for Live startup recovery, broker snapshots, reconciliation drift detection, and alert delivery before starting Live process isolation design.

**Status (2026-07-08 Sync):** Completed for the default local verification path. The committed result document `docs/results/live-recovery/live-recovery-long-run-results-live-recovery-df3cec2a63f1.md` records 20 local fake/injected broker iterations and 320 runtime test invocations with zero non-zero exits. The optional adapter read-only passes remain skipped in that runner because `verify-live-recovery.ps1` was not rerun with Binance testnet credentials or an IBKR paper Gateway account.

Follow-up broker evidence now exists outside this runner: IBKR paper Gateway ReadOnly/AutoRun/Soak evidence, Binance paper/Testnet soak evidence, and 2026-07-08 multi-broker live-reconciliation gate evidence are documented in their dedicated result files. Those artifacts strengthen the broader paper/testnet readiness trail, but they do not change this plan's direct acceptance record: the live recovery long-run gate is local-first, and adapter recovery remains opt-in for this script.

**Architecture:** Keep verification local-first and credential-free by default. Use existing `crates/runtime/tests/live_runtime_tests.rs` fake/injected broker coverage as the deterministic core, wrap it in a PowerShell runner that records per-iteration logs and JSON summaries, and make real adapter read-only recovery checks opt-in.

**Tech Stack:** Rust workspace, Tokio runtime tests, fake/injected broker tests, PowerShell verification script, existing Binance/IBKR paper recovery scripts, JSON summary artifacts.

## Global Constraints

- Do not touch real broker credentials or submit orders in the default verification path.
- Default verification must run with local fake/injected broker tests only.
- Binance/IBKR adapter recovery checks are read-only and opt-in; run only when the operator passes the explicit switch.
- Verification outputs must be written under `data/verification/live-recovery/` and must not include secrets.
- Process isolation design is out of scope until this verification produces credible results.

---

## Verification Matrix

| Area | Default Gate | Extended Gate | Success Signal |
| --- | --- | --- | --- |
| Fake broker startup recovery | `live_runtime_recovers_open_orders_and_executions_on_startup` | Repeat across long-run iterations | Startup recovery log records scanned/recovered/executions and run stops cleanly |
| Unmatched open order fail / warn-only | `live_runtime_fails_startup_when_remote_open_order_is_unmatched`, `live_runtime_can_warn_only_for_unmatched_remote_open_orders_when_configured` | Repeat across long-run iterations | Fail policy marks startup failure; warn-only policy continues and logs warning |
| Recovered executions de-dup | `live_runtime_adds_new_recovered_executions_to_existing_fills`, `live_runtime_does_not_decrease_local_filled_qty_when_recovery_lacks_executions` | Add duplicate-trade-id stress case if a gap appears | Re-running recovery does not duplicate fills or reduce local filled qty |
| Broker snapshot drift | cash/position snapshot tests plus reconciliation drift tests | Repeat across long-run iterations | Cash and position snapshots are recorded; drift events appear only when expected |
| Alert delivery retry/cooldown | file/webhook/multi/retry/cooldown tests | Repeat across long-run iterations | Delivery logs record sent/failed status; cooldown suppresses duplicate file alerts |
| Binance adapter read-only recovery | skipped by default | `scripts/binance/binance-paper-recover-smoke.ps1 -SkipNetwork` or with explicit network switch | Config/preflight/migration pass; network recovery runs only when opted in |
| IBKR adapter read-only recovery | skipped by default | `scripts/ibkr/ibkr-paper-test-guide.ps1 -Stage ReadOnly` with real paper account | Read-only open orders/executions/reconcile/recover commands complete |

## File Structure

- Create: `scripts/check/verify-live-recovery.ps1`
  - Runs the local fake/injected broker recovery matrix for N iterations.
  - Captures one log per test group per iteration.
  - Writes `summary.json` with iteration status, command, exit code, and log path.
  - Supports opt-in adapter recovery checks without enabling order submission.
- Create: `docs/superpowers/plans/2026-06-30-live-recovery-long-run-verification.md`
  - Defines the verification matrix, execution steps, acceptance gates, and decision criteria for process isolation.
- Future result document: `docs/results/live-recovery/live-recovery-long-run-results-<run-id>.md`
  - Summarizes observed failures, flake rate, recovery behavior, adapter coverage, and the go/no-go decision for process isolation.

---

### Task 1: Add Repeatable Live Recovery Verification Script

**Files:**
- Create: `scripts/check/verify-live-recovery.ps1`

**Interfaces:**
- Consumes: existing runtime test names in `crates/runtime/tests/live_runtime_tests.rs`.
- Produces: `data/verification/live-recovery/<verification_id>/summary.json`.

- [x] **Step 1: Create the script with safe defaults**

```powershell
param(
    [int]$Iterations = 3,
    [int]$DelaySeconds = 0,
    [switch]$IncludeBinanceReadOnly,
    [switch]$IncludeBinanceNetwork,
    [switch]$IncludeIbkrReadOnly,
    [string]$IbkrAccountId = "",
    [string]$IbkrGatewayHost = "127.0.0.1",
    [int]$IbkrPort = 7497,
    [int]$IbkrClientId = 1
)
```

- [x] **Step 2: Define local runtime test groups**

```powershell
$localGroups = @(
    @{ name = "startup_recovery"; tests = @("live_runtime_recovers_open_orders_and_executions_on_startup") },
    @{ name = "unmatched_open_order_fail"; tests = @("live_runtime_fails_startup_when_remote_open_order_is_unmatched") },
    @{ name = "unmatched_open_order_warn_only"; tests = @("live_runtime_can_warn_only_for_unmatched_remote_open_orders_when_configured") },
    @{ name = "recovered_execution_dedup"; tests = @("live_runtime_adds_new_recovered_executions_to_existing_fills", "live_runtime_does_not_decrease_local_filled_qty_when_recovery_lacks_executions") },
    @{ name = "broker_snapshot_drift"; tests = @("live_runtime_periodically_records_broker_reported_cash_snapshot", "live_runtime_periodically_records_broker_reported_position_snapshot", "live_runtime_emits_reconciliation_drift_when_broker_cash_differs_from_runtime_cash", "live_runtime_emits_reconciliation_drift_when_broker_position_is_missing_from_runtime", "live_runtime_emits_reconciliation_drift_when_runtime_position_qty_differs_from_broker") },
    @{ name = "alert_retry_cooldown"; tests = @("live_runtime_writes_reconciliation_alert_to_file_sink_when_configured", "live_runtime_posts_reconciliation_alert_to_webhook_sink_when_configured", "live_runtime_sends_reconciliation_alert_to_all_configured_sinks", "live_runtime_retries_webhook_alert_with_auth_header", "live_runtime_does_not_retry_webhook_alert_on_client_error_and_logs_failure", "live_runtime_suppresses_duplicate_file_sink_alerts_within_cooldown") }
)
```

- [x] **Step 3: Run each group with `cargo test -p runtime` and capture logs**

```powershell
cargo test -p runtime $TestName
```

Expected: each group exits `0`; failures stop the run after the current group summary is written.

- [x] **Step 4: Add optional Binance read-only recovery check**

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\binance\binance-paper-recover-smoke.ps1 -SkipNetwork
```

Expected: runs only when `-IncludeBinanceReadOnly` is passed. If `-IncludeBinanceNetwork` is also passed, omit `-SkipNetwork`.

- [x] **Step 5: Add optional IBKR read-only recovery check**

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\ibkr\ibkr-paper-test-guide.ps1 -Stage ReadOnly -AccountId DU... -GatewayHost 127.0.0.1 -Port 7497 -ClientId 1
```

Expected: runs only when `-IncludeIbkrReadOnly` is passed and `-IbkrAccountId` is a non-placeholder paper account id.

- [x] **Step 6: Run local smoke verification**

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\check\verify-live-recovery.ps1 -Iterations 1
```

Expected: `status = "completed"` in the generated summary.

---

### Task 2: Execute Local Fake/Injected Broker Long Run

**Files:**
- Read: `data/verification/live-recovery/<verification_id>/summary.json`
- Create: `docs/results/live-recovery/live-recovery-long-run-results-<verification_id>.md`

**Interfaces:**
- Consumes: JSON summary from Task 1.
- Produces: result document used for the process isolation go/no-go decision.

- [x] **Step 1: Run the long-run matrix**

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\check\verify-live-recovery.ps1 -Iterations 20 -DelaySeconds 1
```

Expected: all local groups pass for all 20 iterations.

- [x] **Step 2: Inspect the summary**

```powershell
Get-Content .\data\verification\live-recovery\<verification_id>\summary.json
```

Expected: `iterations_completed = 20`, `status = "completed"`, no group with non-zero `exit_code`.

- [x] **Step 3: Write the result document**

```markdown
# Live Recovery Long Run Results: <verification_id>

## Scope

- Local fake/injected broker iterations: 20
- Binance read-only recovery: skipped
- IBKR read-only recovery: skipped

## Result

- Overall status: completed
- Startup recovery: pass
- Unmatched open order fail/warn-only: pass
- Recovered execution de-dup: pass
- Broker snapshot drift: pass
- Alert retry/cooldown: pass

## Failures

None observed.

## Decision

Live recovery is stable enough to start a focused Live process isolation design plan.
```

- [x] **Step 4: Commit**

```powershell
git add scripts/check/verify-live-recovery.ps1 docs/superpowers/plans/2026-06-30-live-recovery-long-run-verification.md docs/results/live-recovery/live-recovery-long-run-results-<verification_id>.md
git commit -m "test: add live recovery long-run verification"
```

---

### Task 3: Optional Adapter Read-Only Recovery Pass

Status: deferred by design for this pass. The result document records Binance and IBKR adapter coverage as skipped because no operator-provided testnet credentials, paper account, or running Gateway were supplied.

**Files:**
- Read: `data/verification/live-recovery/<verification_id>/summary.json`
- Modify: `docs/results/live-recovery/live-recovery-long-run-results-<verification_id>.md`

**Interfaces:**
- Consumes: generated configs and databases from existing Binance/IBKR scripts.
- Produces: adapter coverage notes, explicitly marked as skipped/pass/fail.

- [ ] **Step 1: Run Binance safe read-only path without network recovery**

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\check\verify-live-recovery.ps1 -Iterations 1 -IncludeBinanceReadOnly
```

Expected: Binance config/preflight/migration pass; `recover_network = "skipped"`.

- [ ] **Step 2: Run Binance network recovery only when testnet credentials are intentionally available**

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\check\verify-live-recovery.ps1 -Iterations 1 -IncludeBinanceReadOnly -IncludeBinanceNetwork
```

Expected: `binance-paper-recover` completes against testnet read-only recovery path.

- [ ] **Step 3: Run IBKR read-only path only with a paper account and running Gateway**

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\check\verify-live-recovery.ps1 -Iterations 1 -IncludeIbkrReadOnly -IbkrAccountId DU... -IbkrGatewayHost 127.0.0.1 -IbkrPort 7497 -IbkrClientId 1
```

Expected: IBKR read-only, open orders, executions, reconcile, recover, and next-order-id commands complete without order submission.

- [x] **Step 4: Update result document**

Record each adapter as `pass`, `fail`, or `skipped`, with the summary path and any failing log path.

---

## Acceptance Gates

- `powershell -ExecutionPolicy Bypass -File .\scripts\check\verify-live-recovery.ps1 -Iterations 1`
- `powershell -ExecutionPolicy Bypass -File .\scripts\check\verify-live-recovery.ps1 -Iterations 20 -DelaySeconds 1`
- Optional: `powershell -ExecutionPolicy Bypass -File .\scripts\check\verify-live-recovery.ps1 -Iterations 1 -IncludeBinanceReadOnly`
- Optional: `powershell -ExecutionPolicy Bypass -File .\scripts\check\verify-live-recovery.ps1 -Iterations 1 -IncludeIbkrReadOnly -IbkrAccountId DU...`

## Risks and Controls

- **Risk:** Runtime tests are deterministic but too short to expose timing issues.
  - **Control:** Repeat targeted groups for 20+ iterations and keep logs per group.
- **Risk:** Regex test filtering silently misses renamed tests.
  - **Control:** Each group summary records command output; missing or zero-test runs must be treated as invalid in the result review.
- **Risk:** Adapter checks accidentally touch real broker state.
  - **Control:** Adapter checks are opt-in, default skipped, and use read-only/recover commands without order-submit confirmation switches.
- **Risk:** Alert webhook tests become flaky under port contention.
  - **Control:** Run them in their own group and capture logs; repeated failures block process isolation.

## Process Isolation Decision Criteria

Start Live process isolation design only after:

- Local fake/injected broker long-run completes without failures.
- Startup recovery fail/warn-only semantics are stable.
- Recovered execution handling does not duplicate fills.
- Broker snapshot and reconciliation drift behavior is stable.
- Alert delivery retry and cooldown behavior is stable.
- Adapter read-only recovery coverage is either pass or explicitly deferred with a documented reason.
