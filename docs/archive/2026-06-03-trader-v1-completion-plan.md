# Trader V1 Completion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. User chose Inline Execution; do not use subagents.

**Goal:** Complete the V1 feature set defined by `docs/architecture.md`, not just the local MVP vertical slice.

**Architecture:** Keep the existing crate boundaries and extend them toward the documented V1: SQLite remains trading state truth, Parquet becomes historical/research data truth, REST and WebSocket share the same runtime/event state, and all order-producing paths still go through Strategy -> Portfolio -> Market Rules -> Risk -> Execution -> OMS -> Broker -> Accounting. SQL stays only in `crates/storage`.

**Tech Stack:** Rust 2024, Tokio, Axum REST/WebSocket, SQLx SQLite, Polars/Parquet, `rust_decimal::Decimal`, Clap CLI, serde JSON, PowerShell smoke scripts.

---

## V1 Acceptance Checklist

V1 is complete only when all of these are true:

- Core domain supports the documented markets/assets: CN/HK/US/CRYPTO and EQUITY/CRYPTO_SPOT/CRYPTO_PERP/CRYPTO_FUTURE.
- Event bus has stable event envelopes, dispatch, persistence, and event replay loader.
- Storage supports SQLite trading state and Parquet historical data read/write boundaries.
- Backtest, Replay, Paper, and Live runtime surfaces exist and are controllable.
- Replay supports pause, resume, seek, and speed control.
- Strategy has a registry and context object; strategies still do not access Broker, OMS, Storage, API, or Exchange API directly.
- Market rules cover at least CN equity, HK equity, US equity, crypto spot, and crypto perp minimum validation.
- Risk covers max position, max exposure, max drawdown, trading halt, and order-level checks.
- Execution supports immediate orders plus V1 documented TWAP/VWAP/PostOnly/ReduceOnly intent surfaces, even if advanced scheduling remains deterministic local execution.
- OMS supports lifecycle, partial fill, cancel, reject, duplicate/late broker report handling, and repository recovery.
- Broker abstraction has simulated broker plus V1 connector surfaces for Futu/Binance/OKX/IB with deterministic fake adapters for local validation.
- Accounting tracks position, cash, equity, realized/unrealized PnL, fees, and portfolio snapshots.
- Metrics expose return, Sharpe, Sortino, max drawdown, win rate, order count, and fill count.
- CLI exposes init, migrate, import, backtest, replay, report, check-config, CSV export, and HTML report.
- REST API exposes runtime control and query routes for runs, orders, fills, positions, account balances, portfolio snapshots, metrics, events, strategy control, replay control, and broker status.
- WebSocket API exposes subscriptions for events, orders, fills, positions, account, metrics, and replay state; it also accepts replay control messages.
- One V1 smoke script validates CLI, REST, WebSocket, SQLite, Parquet, and runtime control on local deterministic sample data.

## Explicit Non-Goals

- Production real-money deployment readiness.
- Real credentials or real external broker network calls in tests.
- Distributed cluster, Kafka/NATS, SOR, full institutional execution algorithms.
- Qlib online integration.

## File Map

- Modify: `crates/metrics/src/metrics.rs` and `crates/metrics/tests/metrics_tests.rs` for V1 metrics.
- Modify: `apps/trader-cli/src/main.rs` and `crates/api/src/api.rs` to expose expanded metrics.
- Modify/Create: `crates/data/src/parquet.rs`, `crates/data/src/data.rs`, `crates/data/tests/parquet_tests.rs` for Parquet boundaries.
- Modify: `crates/replay/src/replay.rs`, `crates/replay/tests/replay_tests.rs`, `crates/api/src/api.rs` for replay controls.
- Modify: `crates/events/src/*.rs`, `crates/events/tests/event_tests.rs`, `crates/storage/src/repositories.rs` for event replay loader.
- Create: `crates/api/src/ws.rs`, `crates/api/tests/ws_tests.rs` for WebSocket endpoint and control messages.
- Modify: `crates/strategies/src/strategies.rs`, `crates/strategies/tests/strategy_tests.rs` for registry/context.
- Modify: `crates/market_rules/src/market_rules.rs`, `crates/market_rules/tests/market_rules_tests.rs` for multi-market rules.
- Modify: `crates/risk/src/risk.rs`, `crates/risk/tests/risk_tests.rs` for exposure/drawdown risk.
- Modify: `crates/execution/src/execution.rs`, `crates/execution/tests/execution_tests.rs` for V1 execution intent surfaces.
- Modify: `crates/oms/src/oms.rs`, `crates/oms/tests/oms_tests.rs` for report idempotency/recovery.
- Modify: `crates/broker/src/broker.rs`, `crates/broker/tests/broker_tests.rs` for connector surfaces and fake adapters.
- Create: `crates/live/` or add `LiveRuntime` in `crates/runtime` after checking crate direction.
- Modify: `scripts/smoke/mvp-smoke.ps1` or create `scripts/smoke/v1-smoke.ps1` for V1 acceptance.
- Modify: `tech.md`, `docs/api.md`, `docs/database.md`, `docs/events.md`, `docs/roadmap.md` only after implementation changes.

## Task 1: V1 Metrics

- [ ] Write failing tests in `crates/metrics/tests/metrics_tests.rs` for Sharpe, Sortino, max drawdown, win rate, and expanded paper summary fields.
- [ ] Run `cargo test -p metrics` and confirm tests fail because the metrics do not exist.
- [ ] Implement the minimal Decimal-based metrics in `crates/metrics/src/metrics.rs`.
- [ ] Update CLI report and REST `/api/v1/metrics` to include expanded fields from portfolio snapshots and fills/orders.
- [ ] Run `cargo test -p metrics`, `cargo test -p trader-cli report`, and `cargo test -p api metrics`.
- [ ] Commit with `feat: add v1 performance metrics`.

## Task 2: Parquet Historical Data Boundary

- [ ] Write failing tests for reading and writing OHLCV bars through a Parquet API.
- [ ] Add `data::write_bars_to_parquet` and `data::load_bars_from_parquet`.
- [ ] Update `trader import-bars` to accept CSV input and Parquet output.
- [ ] Add config support for `data.source = "parquet"` with current `data.path`.
- [ ] Verify with `cargo test -p data parquet` and CLI import smoke.
- [ ] Commit with `feat: add parquet bar storage`.

## Task 3: Replay Runtime Controls

- [ ] Write failing tests for replay pause, resume, seek, and speed updates.
- [ ] Add `ReplayController`, replay state, and deterministic control handling.
- [ ] Add REST routes for replay pause/resume/seek/speed.
- [ ] Add CLI options for replay speed and start offset.
- [ ] Verify with `cargo test -p replay`, `cargo test -p api replay`, and smoke.
- [ ] Commit with `feat: add replay controls`.

## Task 4: Event Dispatcher And Replay Loader

- [ ] Write failing tests for dispatching persisted events back through the event bus.
- [ ] Add event categories for runtime, order, fill, position, account, metrics, risk, and replay state.
- [ ] Add storage-backed event replay loader.
- [ ] Wire runtime lifecycle events through the dispatcher.
- [ ] Verify with `cargo test -p events` and `cargo test -p storage event`.
- [ ] Commit with `feat: add event replay loader`.

## Task 5: WebSocket API

- [ ] Write failing WebSocket tests for connect, subscribe to events, receive paper order/fill updates, and replay control messages.
- [ ] Add `crates/api/src/ws.rs` and route `/ws`.
- [ ] Broadcast persisted/runtime events to subscribed clients.
- [ ] Support replay pause/resume/seek/speed control messages over WebSocket.
- [ ] Verify with `cargo test -p api ws` and server smoke.
- [ ] Commit with `feat: add websocket api`.

## Task 6: Strategy Registry And Context

- [ ] Write failing tests for registering and creating `moving_average_cross` by name.
- [ ] Add `StrategyContext` carrying symbol, runtime mode, time, and read-only market/account view.
- [ ] Ensure strategies still cannot access storage/broker/OMS/API.
- [ ] Wire backtest/paper runtime to build strategies via registry.
- [ ] Verify with `cargo test -p strategies -p backtest -p paper`.
- [ ] Commit with `feat: add strategy registry context`.

## Task 7: Market Rule Matrix

- [ ] Write failing tests for CN equity, HK equity, US equity, crypto spot, and crypto perp rule sets.
- [ ] Implement lot size, tick size, min notional, trading halt, and basic session flags per market.
- [ ] Replace hard-coded `MarketRuleSet::us_equity()` in paper path with symbol/config-derived rules.
- [ ] Verify with `cargo test -p market_rules -p paper`.
- [ ] Commit with `feat: add v1 market rule sets`.

## Task 8: Risk V1

- [ ] Write failing tests for max exposure, max drawdown, leverage/margin placeholder policy, and trading halt.
- [ ] Implement deterministic `PortfolioRiskPolicy`.
- [ ] Wire paper/backtest order path to use order risk and portfolio risk where state is available.
- [ ] Verify with `cargo test -p risk -p paper -p backtest`.
- [ ] Commit with `feat: add portfolio risk checks`.

## Task 9: Execution V1 Intent Surfaces

- [ ] Write failing tests for immediate, TWAP, VWAP, PostOnly, and ReduceOnly order intents.
- [ ] Add intent types and deterministic local expansion for TWAP/VWAP.
- [ ] Keep advanced live scheduling out of scope but make API stable.
- [ ] Verify with `cargo test -p execution`.
- [ ] Commit with `feat: add v1 execution intents`.

## Task 10: OMS Recovery And Broker Report Idempotency

- [ ] Write failing tests for duplicate fills, late cancel after fill, reject transition, and repository recovery.
- [ ] Add broker report idempotency keys and transition guards.
- [ ] Persist enough order state for local recovery.
- [ ] Verify with `cargo test -p oms -p storage`.
- [ ] Commit with `feat: harden oms recovery`.

## Task 11: Broker Connector Surfaces And Fake Adapters

- [ ] Write failing tests for fake Futu/Binance/OKX/IB adapters implementing the common broker trait.
- [ ] Add broker capability/status query types.
- [ ] Expose REST broker status.
- [ ] Do not add real network calls or credentials.
- [ ] Verify with `cargo test -p broker -p api`.
- [ ] Commit with `feat: add broker connector surfaces`.

## Task 12: Live Runtime Surface

- [ ] Write failing tests that Live runtime can start, emit lifecycle events, query broker status, and stop without placing real orders.
- [ ] Add `LiveRuntime` using broker trait and the same risk/OMS/accounting boundaries.
- [ ] Add REST start/stop/status routes for live mode.
- [ ] Verify with `cargo test -p runtime -p api`.
- [ ] Commit with `feat: add live runtime surface`.

## Task 13: Reports And Exports

- [ ] Write failing CLI tests for `report --format csv` and `report --format html`.
- [ ] Implement CSV export and minimal HTML report from persisted run data.
- [ ] Keep generated output deterministic and local.
- [ ] Verify with `cargo test -p trader-cli report`.
- [ ] Commit with `feat: add report exports`.

## Task 14: V1 Smoke And Documentation

- [ ] Create `scripts/smoke/v1-smoke.ps1`.
- [ ] Script must validate CLI, REST, WebSocket, SQLite, Parquet, replay control, report export, and fake broker/live surfaces.
- [ ] Update `tech.md` and docs to distinguish completed V1 from future production/live-real-money work.
- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `cargo check --workspace --locked`.
- [ ] Run `cargo test --workspace`.
- [ ] Run `powershell -ExecutionPolicy Bypass -File .\scripts\smoke\v1-smoke.ps1`.
- [ ] Commit with `docs: document v1 completion`.

## Current Baseline

The current branch has a working local MVP. It is missing V1-required Parquet, WebSocket, replay controls, Live runtime surface, connector surfaces, expanded metrics, report exports, and several domain hardening tasks. Do not mark V1 complete until Task 14 passes.
