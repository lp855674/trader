# Trader Multi-Run Control Plane Refactor Checklist

Status: Complete
Date: 2026-06-26  
Audience: Maintainers working on the transition from single-config runtime orchestration to a true multi-run control plane

---

## 1. Problem Statement

The current `trader-server` implementation mixes two responsibilities:

- deployment-time server configuration;
- run-time trading configuration.

This causes the server process to behave like a single bound runtime instead of a control plane that can manage many runs.

Observed code symptoms:

- `AppState` stores `config_path` and many API handlers reload `config::AppConfig` from that one file.
- query handlers such as orders, fills, positions, metrics, and some preflight paths derive `run_id` from `app_config.runtime.run_id`.
- launch endpoints for Backtest, Replay, Paper, and Live still depend on server-start config instead of an explicit run request.
- `RuntimeManager` already supports multiple spawned tasks by `run_id`, but the API surface still assumes one global active config.

This is the main architectural blocker for:

- many strategies running at the same time;
- many accounts running at the same time;
- many modes running at the same time;
- independent config versioning and audit per run.

---

## 2. Target Outcome

`trader-server` becomes a control plane, not a single-mode runtime wrapper.

Target behavior:

- one server can manage many runs at once;
- each run independently declares `mode`, `strategy`, `symbols`, `broker`, `account`, `risk`, `data`, and `config version`;
- all run-scoped reads and writes are keyed by explicit `run_id`;
- server startup config only defines deployment concerns;
- Backtest, Replay, Paper, and Live share a common run lifecycle model;
- strategies become reusable definitions/templates, not a single global config section.

---

## 3. Current-State Constraints

These existing assets should be preserved and extended rather than replaced:

- `run_id` is already a strong storage partition key across orders, fills, positions, events, snapshots, and logs.
- `RuntimeManager` already manages concurrent spawned tasks by `run_id`.
- API launch endpoints for `backtests`, `paper-runs`, `replays`, and `live-runs` already exist.
- config lifecycle tables and run-to-config version bindings already exist.
- replay controllers are already stored per `run_id`.

These constraints shape the refactor:

- do not rewrite runtime implementations from scratch;
- do not remove current storage schema unless necessary;
- do not introduce distributed workers in the first phase;
- keep single-process Tokio task execution for the first multi-run milestone.

---

## 4. Architectural Decisions

### 4.1 Server Config vs Run Config

Split configuration into two classes.

Server config:

- database URL;
- bind address;
- global logging settings;
- global retention settings;
- worker limits and concurrency guardrails;
- optional broker credential registry references.

Run config:

- `mode`;
- `strategy` and parameters;
- `symbols` or universe selection;
- broker kind, broker mode, account selection;
- portfolio settings;
- risk settings;
- data source settings;
- replay settings where applicable;
- per-run alerting and recovery overrides where applicable.

Decision:

- remove run semantics from server startup config over time;
- treat current TOML app configs as input to build a `RunSpec`, not as the server's identity.

### 4.2 Run as First-Class Domain Object

`Run` is a logical execution instance, not inherently a process or thread.

Decision:

- represent runs as explicit domain entities with persistent metadata and lifecycle state;
- execute runs as Tokio tasks in the first phase;
- keep the option to isolate Live runs into dedicated worker processes later.

### 4.3 Strategy Model

Decision:

- separate strategy type, parameter template, and concrete run binding;
- stop treating `[strategy]` in a single config file as the only active strategy in the system.

Recommended conceptual split:

- `StrategyDefinition`
- `StrategyTemplate`
- `StrategyInstance` bound into a `RunSpec`

### 4.4 Runtime Factory

Decision:

- create runtimes from `RunSpec.mode` through a central factory;
- keep Backtest, Replay, Paper, and Live behind a shared lifecycle contract;
- vary clock, data source, broker adapter, and control semantics by mode instead of duplicating orchestration logic.

---

## 5. Target Domain Model

### 5.1 Core Types

Introduce or normalize the following concepts:

- `RunId`
- `RunMode`
- `RunStatus`
- `RunSpec`
- `RunMetadata`
- `RunHandle`
- `RunSummary`
- `StrategyRef`
- `BrokerSpec`
- `DataSpec`
- `RiskSpec`
- `PortfolioSpec`

Suggested `RunStatus` values:

- `created`
- `starting`
- `running`
- `stopping`
- `completed`
- `failed`
- `canceled`

### 5.2 RunSpec Shape

Minimum target fields:

```text
run_id
mode
config_ref or config_snapshot
strategy_ref
strategy_params
symbols or universe
broker_spec
portfolio_spec
risk_spec
data_spec
replay_spec?
live_spec?
metadata
```

### 5.3 Query Semantics

Any resource that belongs to a run must be queryable by explicit `run_id`.

Examples:

- orders
- fills
- positions
- balances
- snapshots
- metrics
- events
- system logs
- reconciliation

---

## 6. Required Refactor Workstreams

### Workstream A: Convert `trader-server` into a control plane

Required changes:

- stop defaulting server identity to `configs/backtest/ma_cross.toml`;
- replace `AppState.config_path` with a server config object plus run orchestration dependencies;
- preserve support for loading current TOML configs as launch inputs, but no longer as server-global active runtime state.

Primary files:

- `apps/trader-server/src/main.rs`
- `crates/api/src/state.rs`
- `crates/api/src/api.rs`
- `crates/config/src/config.rs`
- `configs/deploy/trader-server.example.toml`

### Workstream B: Introduce explicit `RunSpec`

Required changes:

- define a request model that can launch a run without relying on server-global `AppConfig`;
- allow launch by full spec or by config version reference plus overrides;
- persist a config snapshot or binding at launch time.

Primary files:

- `crates/api/src/api.rs`
- `crates/runtime/src/*`
- `crates/storage/src/repositories.rs`
- `crates/api/tests/api_tests.rs`
- `docs/api.md`

### Workstream C: Make run-scoped queries explicit

Required changes:

- remove handlers that infer run scope from `app_config.runtime.run_id`;
- require `run_id` path parameter or query parameter for run resources;
- keep aggregate list endpoints only where the semantics are truly cross-run.

Examples of endpoints to revise:

- `/api/v1/orders`
- `/api/v1/fills`
- `/api/v1/positions`
- `/api/v1/account-balances`
- `/api/v1/portfolio/snapshots`
- `/api/v1/cash/snapshots`
- `/api/v1/metrics`

Target replacement pattern:

- `/api/v1/runs/{run_id}/orders`
- `/api/v1/runs/{run_id}/fills`
- `/api/v1/runs/{run_id}/positions`
- `/api/v1/runs/{run_id}/account-balances`
- `/api/v1/runs/{run_id}/portfolio-snapshots`
- `/api/v1/runs/{run_id}/cash-snapshots`
- `/api/v1/runs/{run_id}/metrics`

Primary files:

- `crates/api/src/api.rs`
- `crates/api/tests/api_tests.rs`
- `crates/api/tests/backtest_api_tests.rs`
- `docs/api.md`
- `docs/web-admin-api.md`

### Workstream D: Upgrade `RuntimeManager` into run supervision

Current behavior is enough for simple spawn/cancel, but not enough for a full control plane.

Required changes:

- track run metadata and current status;
- expose active run registry queries;
- capture terminal result and terminal timestamps;
- separate run control from run persistence;
- support startup recovery hooks for recoverable run types.

Primary files:

- `crates/runtime/src/manager.rs`
- `crates/runtime/tests/runtime_manager_tests.rs`
- `crates/api/src/state.rs`
- `crates/api/src/api.rs`

### Workstream E: Decouple strategy selection from global app config

Required changes:

- introduce strategy references or templates at launch time;
- preserve existing strategy implementations;
- stop assuming a single active `[strategy]` block defines the whole server.

Primary files:

- `crates/config/src/config.rs`
- `crates/strategy/*`
- `crates/api/src/api.rs`
- `apps/trader-cli/src/main.rs`
- `docs/strategy.md`

Design direction:

- Treat `StrategyDefinition` as the code-level strategy implementation identity, e.g. `moving_average_cross`.
- Treat `StrategyTemplate` as a versioned managed config entity whose content materializes a valid `StrategyConfig`.
- Treat `StrategyInstance` as the final per-run strategy value after resolving a template and applying launch overrides; this is what belongs in the persisted run config snapshot.
- Add `strategy_ref` as launch-time provenance, not as a replacement for an explicit run config source. Resolution order should be: full run config source, optional `strategy_ref`, optional inline `strategy` overrides, then `RunSpec` materialization.
- Keep `run_config_versions` as the single binding for the canonical run config version or run snapshot. Do not overload it with both full config and strategy-template provenance; store `strategy_ref` provenance in the final snapshot first, or add a separate component binding table in a dedicated storage slice.

### Workstream F: Keep mode-specific runtime behavior behind common orchestration

Required changes:

- standardize launch pipeline for Backtest, Replay, Paper, Live;
- consolidate run lifecycle persistence and event emission;
- keep mode-specific differences in adapters and timing semantics, not in API-level control flow where avoidable.

Primary files:

- `crates/runtime/*`
- `crates/paper/src/paper.rs`
- `crates/replay/*`
- `crates/api/src/api.rs`

---

## 7. Detailed Gap List Against Current Code

### 7.1 Global Config Dependency in API State

Current gap:

- `AppState` stores `config_path: String`.

Impact:

- API handlers repeatedly rebuild one global `AppConfig`;
- server behavior is anchored to a single runtime config file.

Refactor goal:

- `AppState` should store server-level dependencies, not active trading runtime identity.

### 7.2 Launch Endpoints Still Read One Config File

Current gap:

- run creation handlers load config from `state.config_path`.

Impact:

- impossible to launch many independent runs from different strategies and accounts without changing server startup config.

Refactor goal:

- create runs from request payload or config version reference.

### 7.3 Query Endpoints Infer Run from Global Config

Current gap:

- many read endpoints use `app_config.runtime.run_id`.

Impact:

- one server cannot correctly serve many run-scoped resources.

Refactor goal:

- every run-scoped endpoint takes explicit `run_id`.

### 7.4 Server Example Config Mixes Deployment and Runtime Concerns

Current gap:

- `configs/deploy/trader-server.example.toml` contains both deployment settings and trading runtime settings.

Impact:

- operators are taught the wrong mental model.

Refactor goal:

- provide a dedicated server config example and separate run templates.

### 7.5 Runtime Supervision is Too Thin

Current gap:

- `RuntimeManager` knows active tasks and cancellation only.

Impact:

- no unified source for current status, start time, end time, failure metadata, or recovered state.

Refactor goal:

- evolve to a registry plus supervisor abstraction.

### 7.6 Strategy Lifecycle is Under-Modeled

Current gap:

- strategy selection is effectively embedded in one TOML app config.

Impact:

- no clean path for many strategy variants running together.

Refactor goal:

- introduce reusable strategy references/templates and bind them per run.

---

## 8. Execution Order Recommendation

Recommended order:

1. make run-scoped reads explicit;
2. introduce `RunSpec` launch requests;
3. separate server config from run config;
4. enhance runtime supervision and registry;
5. normalize strategy/template/config version binding;
6. harden Live recovery and optional process isolation.

Reason:

- run-scoped reads are the highest-leverage change with the lowest disruption;
- they remove the most dangerous global assumption before launch-path refactors;
- they preserve current storage and runtime implementations while enabling the next steps.

---

## 9. Acceptance Criteria for the Refactor

The refactor is complete when all of the following are true:

- one `trader-server` process can concurrently manage multiple runs across different modes;
- no run-scoped API endpoint depends on server-global `app_config.runtime.run_id`;
- launching a run does not require changing or restarting server startup config;
- a run always persists a config snapshot or config-version binding at launch;
- multiple strategies can run concurrently with different parameters and accounts;
- server deployment config contains no per-run trading identity;
- integration tests cover concurrent multi-run query and control flows.

---

## 10. Out of Scope for the First Refactor Milestone

Do not block the first milestone on:

- multi-process worker isolation;
- distributed scheduling;
- authenticated RBAC expansion;
- production credential vault integration;
- multi-tenant SaaS isolation;
- complete strategy marketplace UX.

These can build on the multi-run control-plane foundation later.

---

## 11. Progress Notes

2026-06-28:

- Run-scoped top-level API reads now require explicit `run_id` query scope instead of resolving through server run defaults.
- `GET /api/v1/brokers/account/{account_id}` requires explicit `broker`.
- `POST /api/v1/preflight/paper` and run launch endpoints (`backtests`, `paper-runs`, `replays`, `live-runs`) require an explicit config source: `config_toml`, `config_ref`, or `config`.
- Launch paths no longer read `[run_defaults].config_path`; server run defaults remain only compatibility configuration, not active run identity.
- Smoke scripts and API docs were updated to send explicit launch config bodies.
- Verified with `cargo test -p api`, `bash ./scripts/check-api-read-model-boundary`, PowerShell AST parsing for modified smoke scripts, and `git diff --check`.
- Full `scripts/verify.ps1` now exits 0, and the historical `check-storage-dto-boundary.ps1` violations in `crates/api/tests/api_tests.rs`, `crates/runtime/src/live.rs`, and `crates/runtime/tests/live_runtime_tests.rs` have been cleaned up.
- `AppState::new(db)` now builds a server control-plane state with no default run config; tests that need legacy run defaults use `AppState::with_default_run_config(...)` explicitly.
- API logging retention scheduling now uses server logging config directly instead of reading retention settings from a run TOML file.
- Run launch requests now support a minimal `strategy` override patch for `name`, `symbols`, `fast_window`, and `slow_window`; the final strategy config is persisted in each run snapshot and visible from `GET /api/v1/runs/{run_id}`.
- Backtest launch requests now resolve `strategy_ref` strategy templates and persist the resolved strategy plus `strategy_ref` provenance into the per-run config snapshot.
- `config_ref` plus `strategy_ref` launch coverage preserves the canonical config binding while materializing the referenced strategy template into the final per-run snapshot.
- Final verification passed with `cargo test -p config`, `cargo test -p runtime`, `cargo test -p api`, `cargo test -p trader-cli`, `cargo check --workspace`, `bash ./scripts/check-api-read-model-boundary`, and `.\scripts\verify.ps1`.

2026-06-29:

- Closed the storage DTO boundary cleanup slice by moving Live runtime startup recovery and API/runtime test seeding onto storage-owned command APIs instead of direct storage write DTO construction.
- Verified with `.\scripts\check-storage-dto-boundary.ps1`, `cargo test -p api`, `cargo test -p runtime`, `git diff --check`, and `.\scripts\verify.ps1`.

---

## 12. Completion State

This milestone is complete. The implementation plan in `docs/superpowers/plans/2026-06-26-multi-run-control-plane-refactor.md` has been executed and checked off.

Recommended next slice:

- harden Live recovery with long-running broker integration verification;
- decide whether optional process isolation for Live runs belongs in the next milestone.
