# Trader Local MVP Completion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. User explicitly chose Inline Execution; do not use subagents.

**Goal:** Build a locally verifiable MVP vertical slice that can be validated by CLI, REST, persisted SQLite state, and one smoke script.

**Architecture:** Keep the current core trading path and fill the missing verification surfaces around it. SQLite remains the source of truth for run state, orders, fills, balances, snapshots, and events; CLI and REST only call crate APIs and never embed SQL. This is a local MVP, not completion of the full long-term architecture in `docs/architecture.md`.

**Tech Stack:** Rust 2024 workspace, Tokio, Axum, Clap, SQLx SQLite, `rust_decimal::Decimal`, PowerShell smoke scripts.

---

## MVP Completion Criteria

The MVP is complete when a developer can run one script and verify:

- Config loads and database migration runs.
- Backtest runs against sample CSV and persists run/order/fill/position data.
- Paper run executes the full current order path: `Strategy -> Portfolio -> Execution delta -> MarketRules -> Risk -> OMS -> Broker -> Accounting -> Storage`.
- Replay loads sample CSV and reports replayed bar count.
- Report reads persisted SQLite state and prints run/order/fill/account/portfolio metrics.
- REST exposes health, paper start/status/cancel, query routes, replay, events, and metrics.
- Event store contains persisted audit events for backtest, paper, and replay lifecycle.

## Explicit Non-Goals For This MVP

- Real broker/live trading adapters.
- Full WebSocket streaming implementation.
- Multi-market rule matrix beyond the current local rule set.
- Parquet research pipeline.
- Distributed runtimes or multi-user access control.

## File Map

- Modify: `crates/storage/src/repositories.rs` for event records and query repository methods.
- Modify: `crates/storage/tests/storage_tests.rs` for event persistence tests.
- Modify: `crates/replay/src/replay.rs` for replay summary and zero-delay local verification.
- Modify: `crates/replay/tests/replay_tests.rs` for replay summary behavior.
- Modify: `apps/trader-cli/src/main.rs` for real `replay` and `report` commands.
- Modify: `apps/trader-cli/Cargo.toml` to add workspace `replay` and `metrics` dependencies.
- Modify: `apps/trader-cli/tests/cli_tests.rs` for replay/report CLI behavior.
- Modify: `crates/api/src/api.rs` for replay and event query endpoints.
- Modify: `crates/api/Cargo.toml` to add workspace `replay` and `serde_json` dependencies.
- Modify: `crates/api/tests/backtest_api_tests.rs` for replay/events API behavior.
- Create: `scripts/smoke/mvp-smoke.ps1` for local end-to-end validation.
- Modify: `scripts/smoke/rest-smoke.ps1` to include replay and events checks.
- Modify: `tech.md` to document the actual local MVP and verification commands.

## Task 1: Storage Event Repository

- [ ] Write failing tests in `crates/storage/tests/storage_tests.rs` for inserting two `event_store` rows and listing all events, plus filtering by `source`.
- [ ] Run `cargo test -p storage event` and confirm tests fail because the event repository types/methods do not exist.
- [ ] Add `NewEventRecord`, `EventRecord`, `Db::insert_event`, `Db::list_events`, and `Db::list_events_by_source` in `crates/storage/src/repositories.rs`.
- [ ] Run `cargo test -p storage event` and confirm tests pass.
- [ ] Commit with `feat: add event repository`.

## Task 2: Replay Runtime Summary

- [ ] Write failing tests in `crates/replay/tests/replay_tests.rs` for `ReplayRuntime::replay_bars` returning a summary with `bars` and `speed`.
- [ ] Run `cargo test -p replay` and confirm tests fail because `replay_bars` returns `usize`.
- [ ] Add `ReplaySummary { bars, speed }` and change `ReplayRuntime::replay_bars` to return it.
- [ ] Preserve current sleep behavior but allow fast tests by using high speed.
- [ ] Run `cargo test -p replay` and confirm tests pass.
- [ ] Commit with `feat: return replay summaries`.

## Task 3: CLI Replay And Report

- [ ] Write failing CLI tests for `trader replay --config configs/backtest/ma_cross.toml` printing `replay completed: bars=...`, and `trader report --config configs/backtest/ma_cross.toml` printing `report: run_id=...`.
- [ ] Run `cargo test -p trader-cli replay report` and confirm tests fail because commands are placeholders or lack `--config`.
- [ ] Implement `Replay { config }` and `Report { config }`.
- [ ] `Replay` must load config, migrate DB, load CSV bars, persist a replay strategy run and lifecycle events, then print replay summary.
- [ ] `Report` must read persisted orders/fills/account balances/portfolio snapshots/runs and print a compact summary.
- [ ] Run `cargo test -p trader-cli replay report` and confirm tests pass.
- [ ] Commit with `feat: complete cli replay and report`.

## Task 4: API Replay And Events

- [ ] Write failing API tests for `POST /api/v1/replays`, `GET /api/v1/events`, and `GET /api/v1/runs/{run_id}/events`.
- [ ] Run `cargo test -p api replay events` and confirm tests fail because routes do not exist.
- [ ] Add replay route that loads bars, runs replay, persists run lifecycle and events, and returns `201 CREATED`.
- [ ] Add event query routes backed by storage repository methods.
- [ ] Persist paper start/completed events from the existing paper route task.
- [ ] Run `cargo test -p api replay events` and confirm tests pass.
- [ ] Commit with `feat: expose replay and events api`.

## Task 5: MVP Smoke Script

- [ ] Create `scripts/smoke/mvp-smoke.ps1`.
- [ ] Script must create a temporary config and SQLite path, run CLI `check-config`, `migrate`, `backtest`, `paper-run`, `replay`, `report`, and then run `scripts/smoke/server-smoke.ps1`.
- [ ] Extend `scripts/smoke/rest-smoke.ps1` to validate replay and events routes.
- [ ] Run `powershell -ExecutionPolicy Bypass -File .\scripts\smoke\mvp-smoke.ps1`.
- [ ] Commit with `test: add mvp smoke validation`.

## Task 6: Documentation And Final Verification

- [ ] Update `tech.md` with current local MVP scope, completed surfaces, non-goals, and exact validation command.
- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `cargo check --workspace --locked`.
- [ ] Run `cargo test --workspace`.
- [ ] Run `powershell -ExecutionPolicy Bypass -File .\scripts\smoke\mvp-smoke.ps1`.
- [ ] Commit with `docs: document local mvp completion`.

## Final Answer Requirement

Report honestly:

- This branch completes a locally verifiable MVP vertical slice.
- It does not complete the full architecture roadmap.
- Include the exact smoke command and the latest verification results.
