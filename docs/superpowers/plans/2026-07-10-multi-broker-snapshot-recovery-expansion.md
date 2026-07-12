# Multi-Broker Snapshot And Recovery Expansion Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend broker reconciliation beyond the current IBKR paper-first path by adding shared snapshot scheduling and recovery coverage for additional broker adapters, while preserving the existing reconciliation audit model and acceptance evidence shape.

**Architecture:** Keep broker snapshot collection and startup recovery logic behind the existing broker boundary and live runtime orchestration. Reuse `broker_reconciliation_audits`, `cash_snapshots`, `position_snapshots`, `system_logs`, and `runtime.alert` as the evidence surfaces. Prefer adapter-specific mapping helpers inside each broker adapter and keep runtime scheduling generic by broker kind/account pair.

**Tech Stack:** Rust workspace crates (`broker`, `runtime`, `storage`, `config`, `trader-cli`), SQLite-backed snapshot/audit persistence, PowerShell verification scripts, existing broker soak/runbook docs.

## Current Status (2026-07-10 Sync)

The current implementation already covers:

- IBKR paper Gateway account/position snapshot mapping through the broker boundary.
- Fake broker cash/position snapshot scheduling and live runtime reconciliation persistence.
- Binance parser-side account/position/open-order/execution reconciliation tests.
- Live startup recovery for broker open orders/executions, including recover-success audit projection, unknown remote open-order default block, explicit `warn_only`, and recovery failure preservation.
- Accepted IBKR paper-account production reconciliation evidence with 39 clean audits and fresh multi-broker read-only reconciliation gate evidence.

The next engineering gap is no longer the generic reconciliation audit model. The remaining product gap is adapter breadth and operator confidence:

- Non-IBKR broker-reported cash/account snapshot scheduling is still incomplete.
- More real-broker startup recovery coverage is still missing beyond the existing IBKR/fake/Binance-tested paths.
- Multi-broker evidence is accepted for read-only gate behavior, but not yet expanded into broader broker/account runtime scheduling and recovery soak coverage.

This plan intentionally excludes RBAC, multi-person approval, and live-money trading controls. It also does not claim filled-order or real-money acceptance.

## 2026-07-11 Task 1 Follow-Up

- `Broker::snapshot_bundle` now carries account, positions, open orders, and executions behind one broker boundary.
- Live startup recovery and periodic reconciliation consume broker open orders/executions from the bundle instead of issuing separate runtime-side broker calls.
- Runtime supplies local order symbols as execution hints so broker adapters can include executions for recoverable/reconcilable local orders even when no remote open order or position exists.

## 2026-07-11 Task 2 Follow-Up

- Binance startup recovery coverage now verifies that local canonical order symbols are passed through the shared snapshot bundle as execution hints.
- The non-IBKR recovery test only returns a broker execution when the runtime supplies the expected Binance symbol hint, protecting recovery from regressing to IBKR-only open-order/position-derived execution discovery.

## 2026-07-12 Task 3 Follow-Up

- CLI reconciliation readback now reports persisted broker reconciliation audit count plus the latest audit broker, account, and severity.
- This keeps multi-broker snapshot evidence inspectable from the existing `reconciliation` command without adding new storage tables or changing audit payload semantics.

## 2026-07-12 Task 4 Follow-Up

- `scripts/ops-smoke.ps1` now asserts the CLI reconciliation audit evidence fields emitted by the broadened readback path.
- The smoke remains credential-free and continues to exercise the fake broker live run while checking that broker/account/severity audit evidence is visible to operators.

## 2026-07-12 Task 5 Follow-Up

- Fresh local verification passed with `powershell -ExecutionPolicy Bypass -File .\scripts\verify.ps1`, `powershell -ExecutionPolicy Bypass -File .\scripts\clippy.ps1`, `cargo test -p broker`, `cargo test -p runtime`, `cargo test -p storage`, and the three standalone boundary scripts.
- `clippy.ps1` exited 0 with existing warnings; no new clippy failure blocked this slice.
- Broker-connected evidence was available for a Binance Testnet no-submit recovery/read-only path: `powershell -ExecutionPolicy Bypass -File .\scripts\verify-live-recovery.ps1 -Iterations 1 -IncludeBinanceReadOnly -IncludeBinanceNetwork` completed as `live-recovery-83853c8d89b6`; the adapter log reported `order_submit = not_run` and `scanned=0 recovered=0 missing=0 remaining=0 trades=0`.
- This broker-connected check is partial evidence for recovery connectivity, not full acceptance evidence for persisted snapshot/reconciliation-audit writes. The remaining gap is external broker-connected snapshot/reconciliation evidence, not local wiring or local code health.

## 2026-07-12 Binance Paper Soak Follow-Up

- A stronger Binance Testnet no-submit paper/read-only soak ran with `powershell -ExecutionPolicy Bypass -File .\scripts\binance-paper-soak.ps1 -Iterations 3 -Limit 100 -DelaySeconds 0 -SkipRefresh`.
- The soak completed as `binance-paper-soak-c38b82cd44ed` with 3 completed iterations, `failure_class=ok`, `order_submit=disabled`, `reconciliation_status=ok`, and zero remaining open orders in every iteration.
- Run-scoped CLI reconciliation readback for each retained SQLite database reported `cash_snapshots=101`, `position_snapshots=98`, `drift_events=0`, and `reconciliation_audits=0`.
- The committed result document is `docs/multi-broker-snapshot-recovery-results-binance-paper-soak-c38b82cd44ed.md`.
- This improves Binance Testnet paper/no-submit snapshot evidence beyond the earlier recovery-only check, but it still does not close the full external broker-connected snapshot/reconciliation audit gap because the Binance paper path does not persist `broker_reconciliation_audits`. Full closure still requires a broker-connected runtime path that writes reconciliation audit rows, currently the IBKR paper Gateway production-reconciliation path.

## Global Constraints

- Do not submit live-money orders while implementing this plan.
- Do not weaken the current IBKR paper-account reconciliation evidence path.
- Keep all money, quantity, fee, and position fields in `rust_decimal::Decimal` at domain boundaries.
- Broker-connected tests must skip cleanly unless explicit credentials/connectivity are provided.
- Generated `data/` evidence stays uncommitted; committed docs summarize run ids, broker kinds, accounts, and result status.
- Do not touch unrelated local notes such as `记录.md`.

---

## File Map

### Broker Boundary

- Modify: `crates/broker/src/broker.rs`
  - Extend shared snapshot/recovery capabilities exposed by broker adapters.
  - Normalize broker-account snapshot metadata needed by runtime scheduling.
- Modify: `crates/broker/src/binance.rs`
  - Add explicit account snapshot mapping coverage for runtime scheduling inputs.
  - Tighten startup recovery matching helpers for live runtime reuse where missing.
- Modify: `crates/broker/src/ibkr.rs`
  - Reuse the same shared scheduling surface as other adapters; preserve current IBKR behavior.
- Modify: `crates/broker/tests/broker_tests.rs`
  - Add adapter-level tests for snapshot scheduling inputs and recovery matching behavior.

### Runtime and Config

- Modify: `crates/config/src/config.rs`
  - Add or refine broker snapshot scheduling config so non-IBKR adapters can participate without bespoke runtime wiring.
- Modify: `crates/config/tests/config_tests.rs`
  - Cover parsing/defaults for any new broker snapshot or recovery config knobs.
- Modify: `crates/runtime/src/live.rs`
  - Generalize broker snapshot scheduling across supported adapters.
  - Keep reconciliation audit and alert payloads stable while broadening adapter coverage.
  - Reuse startup recovery enforcement for more broker kinds through the existing boundary.
- Modify: `crates/runtime/tests/live_runtime_tests.rs`
  - Add fake/injected broker scheduling and recovery tests for non-IBKR adapter paths.

### Storage and Operator Surface

- Modify: `crates/storage/src/repositories.rs`
  - Add only the narrow query helpers needed to inspect broker-specific snapshot/recovery evidence if current helpers are insufficient.
- Modify: `crates/storage/tests/runtime_repository_tests.rs`
  - Cover any new snapshot/recovery readback helpers.
- Modify: `apps/trader-cli/src/main.rs`
  - Extend existing reconciliation/recovery inspection commands only if current output cannot distinguish the broader broker evidence.
- Modify: `scripts/ops-smoke.ps1`
  - Expand local operator smoke only where a broker-agnostic path can be exercised without external credentials.
- Modify: `docs/roadmap.md`
  - Update remaining broker breadth and recovery limitations after implementation.
- Modify: `docs/分析.md`
  - Record the new broker snapshot/recovery coverage and remaining production limits.
- Create template: `docs/multi-broker-snapshot-recovery-results-template.md`
- Create after broker-connected verification: `docs/multi-broker-snapshot-recovery-results-<run_id>.md`
  - Summarize one committed operator evidence run after implementation lands.

---

## Acceptance Gates

Every task must preserve:

- `powershell -ExecutionPolicy Bypass -File .\scripts\verify.ps1`
- `powershell -ExecutionPolicy Bypass -File .\scripts\clippy.ps1`
- `cargo test -p broker`
- `cargo test -p runtime`
- `cargo test -p storage`
- `powershell -ExecutionPolicy Bypass -File .\scripts\check-db-boundary.ps1`
- `powershell -ExecutionPolicy Bypass -File .\scripts\check-storage-dto-boundary.ps1`
- `powershell -ExecutionPolicy Bypass -File .\scripts\check-api-read-model-boundary.ps1`

New focused gates:

- `cargo test -p broker broker_account_snapshot`
- `cargo test -p broker startup_recovery`
- `cargo test -p runtime live_runtime_reconciliation`
- `cargo test -p runtime live_runtime_startup_recovery`
- `powershell -ExecutionPolicy Bypass -File .\scripts\ops-smoke.ps1`

Broker-connected operator gate (documented, not required for credential-free local validation):

- Run one read-only or no-submit broker-connected verification that proves the broadened adapter path writes snapshots, reconciliation audits, and recovery diagnostics without drift or orphaned open orders for the chosen broker/account.

---

## Task 1: Generalize Broker Snapshot Scheduling Surface

**Files:**
- Modify: `crates/broker/src/broker.rs`
- Modify: `crates/broker/src/binance.rs`
- Modify: `crates/broker/src/ibkr.rs`
- Modify: `crates/broker/tests/broker_tests.rs`

**Produces:**
- A broker-boundary surface that can report account/cash and position snapshots consistently across supported adapters.
- Adapter tests that prove runtime scheduling inputs are present and normalized.

- [x] Add failing broker tests for non-IBKR snapshot mapping and shared scheduling expectations.
- [x] Normalize any missing broker account snapshot fields needed by runtime scheduling.
- [x] Keep IBKR behavior unchanged while making the shared scheduling surface adapter-agnostic.
- [x] Verify focused broker tests and preserve existing reconciliation tests.

## Task 2: Reuse Startup Recovery Across More Broker Paths

**Files:**
- Modify: `crates/broker/src/broker.rs`
- Modify: `crates/runtime/src/live.rs`
- Modify: `crates/runtime/tests/live_runtime_tests.rs`
- Modify: `crates/broker/tests/broker_tests.rs`

**Produces:**
- Broader startup recovery coverage for additional broker adapters through the existing runtime boundary.
- Tests that prove unmatched remote open orders, recovery success, and warn-only behavior still work outside the current narrow path.

- [x] Add failing runtime and broker tests for non-IBKR recovery matching and enforcement.
- [x] Move any adapter-specific recovery matching helpers behind the shared broker boundary if needed.
- [x] Preserve default block-on-unknown-open-order behavior and current warn-only override semantics.
- [x] Verify recovery tests plus regression coverage for current IBKR/fake behavior.

## Task 3: Broaden Runtime Scheduling And Evidence Readback

**Files:**
- Modify: `crates/config/src/config.rs`
- Modify: `crates/config/tests/config_tests.rs`
- Modify: `crates/runtime/src/live.rs`
- Modify: `crates/storage/src/repositories.rs`
- Modify: `crates/storage/tests/runtime_repository_tests.rs`
- Modify: `apps/trader-cli/src/main.rs` (only if needed)

**Produces:**
- Configurable snapshot scheduling that can activate supported non-IBKR adapters without custom runtime forks.
- Readback/query support sufficient to inspect broker-specific snapshot and recovery evidence.

- [x] Add failing config/runtime tests for broadened broker snapshot scheduling.
- [x] Wire runtime scheduling through the shared broker surface for supported adapters.
- [x] Add narrow storage/query helpers only where current reconciliation inspection is insufficient.
- [x] Verify runtime, storage, and config gates without changing audit semantics unnecessarily.

## Task 4: Extend Local Operator Smoke And Documentation

**Files:**
- Modify: `scripts/ops-smoke.ps1`
- Modify: `docs/roadmap.md`
- Modify: `docs/分析.md`
- Create template: `docs/multi-broker-snapshot-recovery-results-template.md`
- Create after broker-connected execution: `docs/multi-broker-snapshot-recovery-results-<run_id>.md`

**Produces:**
- Local smoke coverage for the broker-agnostic parts of the new scheduling/recovery path.
- Documentation that distinguishes completed local breadth from remaining external broker evidence.

- [x] Expand `ops-smoke.ps1` only for credential-free broker-agnostic verification.
- [x] Update roadmap and analysis docs to reflect the new coverage and remaining limits.
- [x] Prepare the results-doc template/shape for one broker-connected follow-up run.
- [x] Verify docs and smoke script behavior locally.

## Task 5: Full Verification And Optional Broker-Connected Evidence

**Files:**
- No new source files required beyond prior tasks.
- Produce operator evidence doc only if broker connectivity is available.

**Produces:**
- Clean local verification across broker/runtime/storage/config/operator surfaces.
- Optional committed evidence doc for one broadened broker path.

- [x] Run full required local verification (`verify.ps1`, `clippy.ps1`, focused package tests, boundary scripts).
- [x] If broker connectivity is available, execute one read-only or no-submit verification for the broadened adapter path and record the result doc. Broker connectivity was not available in this local pass, so no broker-connected result doc was produced.
- [x] If broker connectivity is unavailable, explicitly record that the remaining gap is external broker evidence, not local code health.

---

## Execution Order

1. Task 1: Broker snapshot surface.
2. Task 2: Startup recovery breadth.
3. Task 3: Runtime scheduling and readback.
4. Task 4: Smoke/docs updates.
5. Task 5: Full verification and optional broker evidence.

Do not start Task 3 before Tasks 1-2 pass, because runtime scheduling depends on a stable shared broker snapshot/recovery surface. Do not claim Task 5 broker evidence complete without an explicit operator run against a real broker-connected environment.

## Exit Criteria

This plan is complete when:

- Supported non-IBKR adapters can participate in the shared snapshot scheduling path without bespoke runtime branches.
- Startup recovery semantics are covered by tests across the broader broker surface.
- Local operator smoke and query surfaces can inspect the broadened evidence path.
- Docs clearly separate completed local breadth from remaining external broker validation.
- Remaining open gates are external broker evidence breadth, filled-order proof, and live-money validation rather than local missing wiring.
