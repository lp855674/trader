# Market Rules Runtime Governance Expansion Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expand market-rule runtime assembly, local governance evidence, and operator readback so versioned market rules can be configured, inspected, and exercised deterministically without broker credentials.

**Architecture:** Keep rule persistence in `storage`, rule semantics in `market_rules`, and runtime assembly in `paper`/runtime call sites. Reuse existing event-store audit and config governance patterns for local evidence, but do not claim production RBAC, external identity, hosted approvals, or live-money coverage.

**Tech Stack:** Rust workspace crates (`market_rules`, `storage`, `paper`, `api`, `trader-cli`), SQLite-backed market-rule/reference tables, PowerShell operator smoke scripts, existing roadmap/analysis docs.

## Current Status (2026-07-11 Sync)

The current implementation already covers:

- Storage boundaries for `market_calendars`, `trading_sessions`, `fee_rules`, `lot_size_rules`, `price_limit_rules`, and `crypto_market_meta`.
- Runtime paper assembly for lot-size and price-limit rules with fallback to code defaults.
- Simulated paper fill fee calculation using maker/taker fees, tax fees, exchange fees, minimum fee floors, tiers, and account volume.
- Batch paper trading schedule preload and stream paper dynamic storage-backed calendar/session refresh.
- Event-store audit writes for `lot_size_rules`, `price_limit_rules`, and `fee_rules` insert/update/effective-to transitions.
- API tests for fee-rule creation, tier ordering, explicit volume windows, and symbol-specific versus exchange-default lookup.

The remaining local engineering gap is not initial market-rule persistence. The gap is operator confidence that configured rules are selected by time/version, assembled into runtime behavior, governed with explicit evidence, and readable through a consistent local surface:

- Fee tier volume-window behavior needs broader runtime assembly evidence, not only storage/API route coverage.
- Calendar/session boundaries need more explicit local gates for market-day selection and dynamic refresh behavior.
- Governance evidence exists at storage/event level, but market-rule change approval/readback is not yet presented as a cohesive operator workflow.
- API/CLI/smoke coverage is incomplete for proving the market-rule state that a run used.

This plan intentionally excludes live-money trading, external broker validation, production SSO/IdP RBAC, hosted approval systems, and reference-data production rate-limit/backoff/stale alerting. Those remain separate production hardening tracks.

## Global Constraints

- Do not submit live-money orders while implementing this plan.
- Keep all money, quantity, notional, fee, and volume fields in `rust_decimal::Decimal` at domain boundaries.
- Preserve existing storage DTO and API read-model boundaries.
- Do not duplicate rule engines that already exist in `market_rules`.
- Generated `data/` evidence stays uncommitted; committed docs summarize run ids, commands, and result status.
- Do not touch unrelated local notes such as `记录.md`.

---

## File Map

### Market Rules Core

- Modify: `crates/market_rules/src/market_rules.rs`
  - Add only narrow helpers needed for explicit runtime assembly evidence if existing APIs cannot express it.
  - Preserve current fee-tier and volume-window semantics.
- Modify: `crates/market_rules/tests/market_rules_tests.rs`
  - Cover any missing fee-tier volume-window or version-selection behavior at the engine boundary.

### Storage and Audit

- Modify: `crates/storage/src/repositories.rs`
  - Add narrow readback helpers only if current repository APIs cannot inspect effective market rules and audit evidence by market/exchange/symbol/time.
- Modify: `crates/storage/tests/runtime_repository_tests.rs`
  - Cover effective lot/price/fee/calendar/session readback and market-rule audit projection.

### Runtime Assembly

- Modify: `crates/paper/src/paper.rs`
  - Make the configured market-rule assembly path explicit enough to verify selected effective rules, fee tier windows, and calendar/session decisions.
- Modify: `crates/paper/tests/paper_tests.rs`
  - Add batch paper tests proving runtime uses effective lot/price/fee rules and blocks closed-market or out-of-session orders.
- Modify: `crates/paper/tests/paper_stream_tests.rs`
  - Add stream paper tests proving storage-backed calendar/session refresh observes rule changes by slice/trading day where currently uncovered.

### API, CLI, and Operator Surface

- Modify: `crates/api/src/api.rs`
  - Extend read-only market-rule queries only where current API cannot inspect effective configured state and local governance evidence.
- Modify: `crates/api/tests/api_tests.rs`
  - Cover effective readback and market-rule governance/audit readback routes.
- Modify: `apps/trader-cli/src/main.rs`
  - Add or extend read-only inspection commands only if API/storage evidence is not already reachable from CLI.
- Modify: `apps/trader-cli/tests/cli_tests.rs`
  - Cover any new market-rule readback command output.
- Modify: `scripts/ops-smoke.ps1`
  - Add credential-free local smoke checks for market-rule setup, effective readback, runtime behavior, and governance/audit evidence.

### Documentation

- Modify: `docs/roadmap.md`
  - Update market-rule status after implementation, keeping production limits explicit.
- Modify: `docs/分析.md`
  - Record new local runtime/governance evidence and remaining production gaps.
- Modify: `docs/tech/market_rules.md`
  - Document runtime assembly, effective-time selection, and local governance/readback expectations.
- Create: `docs/market-rules-runtime-governance-results-template.md`
  - Template for local market-rule governance/runtime evidence runs.
- Create after local verification: `docs/market-rules-runtime-governance-results-<run_id>.md`
  - Summarize one committed local evidence run after implementation lands.

---

## Acceptance Gates

Every task must preserve:

- `cargo fmt`
- `cargo test -p market_rules`
- `cargo test -p storage`
- `cargo test -p paper`
- `cargo test -p api`
- `powershell -ExecutionPolicy Bypass -File .\scripts\verify.ps1`
- `powershell -ExecutionPolicy Bypass -File .\scripts\check-db-boundary.ps1`
- `powershell -ExecutionPolicy Bypass -File .\scripts\check-storage-dto-boundary.ps1`
- `powershell -ExecutionPolicy Bypass -File .\scripts\check-api-read-model-boundary.ps1`

Focused gates for this plan:

- `cargo test -p market_rules fee`
- `cargo test -p storage market_rule`
- `cargo test -p paper market_rules`
- `cargo test -p paper trading_session`
- `cargo test -p api fee_rules`
- `cargo test -p trader-cli market_rule` if CLI surface changes
- `powershell -ExecutionPolicy Bypass -File .\scripts\ops-smoke.ps1`

Optional external-data gate, documented but not required for credential-free local validation:

- Run reference-data ingestion in a non-production sandbox and record whether imported metadata can be inspected through the same market-rule/readback surface. This must not be used to claim production stale-data alerting or rate-limit hardening.

---

## Task 1: Inventory Effective Rule Assembly And Readback Surface

**Files:**
- Read: `crates/market_rules/src/market_rules.rs`
- Read: `crates/storage/src/repositories.rs`
- Read: `crates/paper/src/paper.rs`
- Read: `crates/api/src/api.rs`
- Read: `apps/trader-cli/src/main.rs`
- Modify: this plan document

**Produces:**
- A concrete inventory of current effective-rule selection, runtime assembly, audit writes, and readback gaps.
- A narrowed implementation checklist for Tasks 2-5 based on actual missing surfaces.

- [x] Map effective lot-size, price-limit, fee-rule, calendar, and session selection paths.
- [x] Map runtime assembly call sites for batch paper and stream paper.
- [x] Map existing API/CLI readback coverage and identify missing effective-state queries.
- [x] Map existing event-store/governance evidence for market-rule changes.
- [x] Update this plan with any file-map or task-scope corrections discovered during inventory.

### Task 1 Inventory Results

- Effective lot/price selection already exists in `storage::Db::find_lot_size_rule` and `storage::Db::find_price_limit_rule`, and `paper::load_configured_market_rules` applies those records by symbol and `as_of_ms`.
- Effective fee selection already exists in `storage::Db::find_fee_rule_with_tiers`, `storage::Db::load_market_fee_rules`, and `storage::Db::load_market_fee_rules_with_account_volume`; `paper` seeds `FeeRuleEngine` with account-volume entries at runtime startup.
- Calendar/session selection already exists through `storage::Db::find_market_calendar`, `storage::Db::list_trading_session_rules`, batch `paper::load_configured_trading_schedule`, and stream `DynamicTradingScheduleProvider` refresh.
- Market-rule audit writes already exist through `record_market_rule_audit`, with `market_rule.lot_size.changed`, `market_rule.price_limit.changed`, and `market_rule.fee.changed` event-store categories.
- API readback currently exposes `/api/v1/fee-rules` and generic `/api/v1/events`; it does not expose a cohesive effective market-rule state that includes lot-size, price-limit, fee, calendar, sessions, and matching market-rule audit events.
- CLI readback currently exposes config governance, snapshots, reconciliation, logs, and event-derived projections, but no dedicated market-rule effective-state command.
- Operator smoke currently verifies broker-agnostic snapshot/recovery, live run, and config governance, but not credential-free market-rule effective readback or audit evidence.

Tasks 2-4 should therefore avoid changing the core rule engine unless a missing test proves a semantic gap. The first implementation slice should add cohesive effective-state/audit readback, then use that readback in CLI and smoke evidence.

## Task 2: Broaden Runtime Assembly Evidence

**Files:**
- Modify: `crates/paper/src/paper.rs`
- Modify: `crates/paper/tests/paper_tests.rs`
- Modify: `crates/paper/tests/paper_stream_tests.rs`
- Modify: `crates/market_rules/tests/market_rules_tests.rs` only if core behavior is under-specified

**Produces:**
- Runtime tests proving configured effective rules are selected and applied by time, symbol/default precedence, and market-day/session state.
- Runtime evidence for fee-tier volume windows where simulated fills depend on account volume.

- [ ] Add failing runtime tests for effective lot/price rule selection at order validation time where coverage is missing.
- [ ] Add failing runtime tests for fee-tier/account-volume window selection where coverage is missing.
- [ ] Add failing runtime tests for calendar/session boundaries and stream refresh gaps where coverage is missing.
- [ ] Implement the smallest runtime assembly/readback changes needed for those tests.
- [ ] Verify focused `paper` and `market_rules` gates.

## Task 3: Add Local Governance And Effective-State Readback

**Files:**
- Modify: `crates/storage/src/repositories.rs`
- Modify: `crates/storage/tests/runtime_repository_tests.rs`
- Modify: `crates/api/src/api.rs`
- Modify: `crates/api/tests/api_tests.rs`
- Modify: `apps/trader-cli/src/main.rs` if CLI readback is missing
- Modify: `apps/trader-cli/tests/cli_tests.rs` if CLI readback changes

**Produces:**
- Read-only effective-state queries for configured market rules by market/exchange/symbol/time.
- Local governance/audit readback that lets operators inspect market-rule changes and publication evidence without claiming production RBAC.

- [ ] Add failing storage/API tests for missing effective-state readback.
- [ ] Add failing storage/API tests for market-rule audit/governance evidence readback.
- [ ] Add or extend CLI inspection only where current CLI cannot reach the new readback.
- [ ] Preserve existing fee-rule route compatibility.
- [ ] Verify focused `storage`, `api`, and `trader-cli` gates.

## Task 4: Extend Operator Smoke And Documentation

**Files:**
- Modify: `scripts/ops-smoke.ps1`
- Modify: `docs/tech/market_rules.md`
- Modify: `docs/roadmap.md`
- Modify: `docs/分析.md`
- Create: `docs/market-rules-runtime-governance-results-template.md`
- Create: `docs/market-rules-runtime-governance-results-<run_id>.md`

**Produces:**
- Credential-free operator smoke that proves configured market-rule setup, effective readback, runtime enforcement, and local governance/audit evidence.
- Updated docs that distinguish local deterministic evidence from remaining production hardening.

- [ ] Add local market-rule smoke setup with deterministic SQLite state.
- [ ] Add smoke assertions for effective readback and runtime enforcement.
- [ ] Add smoke assertions for local governance/audit evidence.
- [ ] Update docs and evidence template.
- [ ] Verify `scripts/ops-smoke.ps1`.

## Task 5: Full Local Verification And Commit

**Files:**
- Modify: this plan document
- Commit: all scoped implementation and documentation changes

**Produces:**
- Completed checklist with exact verification commands and results.
- One focused commit for the implemented market-rule runtime/governance expansion.

- [ ] Run `cargo fmt`.
- [ ] Run focused market-rule/storage/paper/API/CLI gates.
- [ ] Run `powershell -ExecutionPolicy Bypass -File .\scripts\ops-smoke.ps1`.
- [ ] Run `powershell -ExecutionPolicy Bypass -File .\scripts\verify.ps1`.
- [ ] Run boundary scripts if storage/API read models changed.
- [ ] Update status, docs, and evidence summary.
- [ ] Commit the scoped changes.
