# Multi-Run Control Plane Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor `trader-server` from a single-config runtime wrapper into a multi-run control plane that can launch, supervise, and query many concurrent Backtest, Replay, Paper, and Live runs.

**Architecture:** Keep runtime implementations mostly intact, but move orchestration around them. The server becomes a deployment-scoped control plane, `RunSpec` becomes the launch contract, and all run-owned resources become explicitly run-scoped in API and storage reads. Execute runs as Tokio tasks in the current process during this phase, while preserving the option to isolate selected run types later.

**Tech Stack:** Rust 2024, Tokio, Axum, SQLx SQLite, serde, clap, chrono.

## Global Constraints

- Preserve existing `run_id`-keyed storage tables and projections unless a schema addition is strictly necessary.
- Do not introduce distributed workers or process-per-run execution in this plan.
- Keep Backtest, Replay, Paper, and Live behind one orchestration model while preserving mode-specific adapters and controls.
- Keep production tests in `tests/`; do not add inline `#[cfg(test)] mod tests`.
- Keep API migrations incremental so existing behavior can be adapted without a large flag day.
- Use config snapshots or config-version bindings for every launched run.

---

## File Structure

Modify:

- `trader/apps/trader-server/src/main.rs`
- `trader/crates/api/src/state.rs`
- `trader/crates/api/src/api.rs`
- `trader/crates/api/src/ws.rs`
- `trader/crates/api/tests/api_tests.rs`
- `trader/crates/api/tests/backtest_api_tests.rs`
- `trader/crates/api/tests/ws_tests.rs`
- `trader/crates/runtime/src/manager.rs`
- `trader/crates/runtime/tests/runtime_manager_tests.rs`
- `trader/crates/config/src/config.rs`
- `trader/crates/storage/src/repositories.rs`
- `trader/crates/storage/tests/runtime_repository_tests.rs`
- `trader/apps/trader-cli/src/main.rs`
- `trader/apps/trader-cli/tests/cli_tests.rs`
- `trader/configs/deploy/trader-server.example.toml`
- `trader/trader-server.local.example.ps1`
- `trader/docs/api.md`
- `trader/docs/architecture.md`
- `trader/docs/web-admin-api.md`
- `trader/docs/strategy.md`
- `trader/docs/linux-deployment-runbook.md`

Create:

- `trader/crates/runtime/src/run_spec.rs`
- `trader/crates/runtime/src/run_registry.rs`
- `trader/crates/runtime/tests/run_spec_tests.rs`
- `trader/crates/api/tests/multi_run_api_tests.rs`
- `trader/docs/2026-06-26-multi-run-control-plane-refactor-checklist.md`

---

### Task 1: Introduce Server Config Separate from Run Config

**Files:**
- Modify: `trader/apps/trader-server/src/main.rs`
- Modify: `trader/crates/api/src/state.rs`
- Modify: `trader/crates/config/src/config.rs`
- Modify: `trader/configs/deploy/trader-server.example.toml`
- Modify: `trader/trader-server.local.example.ps1`

**Interfaces:**
- Consumes: `config::AppConfig::from_toml_file`, `api::AppState::new`
- Produces: `config::ServerConfig`, `api::AppState::new(db: Db, server_config: ServerConfig)`

- [x] **Step 1: Add failing config parsing test for server config**

Add a test in `trader/crates/config/tests/config_tests.rs` that parses a deployment-only config containing database, bind, and logging settings but no `[runtime]` or `[strategy]` section.

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test -p config server_config_parses_deployment_only_file`

Expected: FAIL because `ServerConfig` or equivalent parser does not exist.

- [x] **Step 3: Implement `ServerConfig` and wire `trader-server` to use it**

Create a deployment-scoped config type in `trader/crates/config/src/config.rs` and update `trader/apps/trader-server/src/main.rs` so the server no longer defaults to `configs/backtest/ma_cross.toml` as its runtime identity.

- [x] **Step 4: Update `AppState` to hold server config instead of `config_path`**

Replace `config_path: String` in `trader/crates/api/src/state.rs` with a server-level config object plus the existing runtime dependencies.

- [x] **Step 5: Update deployment examples**

Revise `trader/configs/deploy/trader-server.example.toml` and `trader/trader-server.local.example.ps1` to show deployment config only and to stop teaching a single-runtime mental model.

- [x] **Step 6: Run verification**

Run:

```powershell
cargo test -p config
cargo check -p api -p config -p app
```

- [x] **Step 7: Commit**

```powershell
git add trader/apps/trader-server/src/main.rs trader/crates/api/src/state.rs trader/crates/config/src/config.rs trader/configs/deploy/trader-server.example.toml trader/trader-server.local.example.ps1
git commit -m "refactor: separate server config from run config"
```

### Task 2: Add `RunSpec` as the Canonical Launch Contract

**Files:**
- Create: `trader/crates/runtime/src/run_spec.rs`
- Modify: `trader/crates/runtime/src/lib.rs`
- Modify: `trader/crates/config/src/config.rs`
- Create: `trader/crates/runtime/tests/run_spec_tests.rs`

**Interfaces:**
- Consumes: existing runtime mode enum, broker config shape, strategy config shape
- Produces: `RunSpec`, `RunMode`, `StrategyRef`, `BrokerSpec`, `PortfolioSpec`, `RiskSpec`, `DataSpec`

- [x] **Step 1: Add failing `RunSpec` construction tests**

Add tests that build a `RunSpec` from an existing app-style config and assert mode, strategy, broker, and symbol fields are preserved.

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test -p runtime run_spec`

Expected: FAIL because `run_spec` module and types do not exist.

- [x] **Step 3: Implement `RunSpec` and conversion helpers**

Create `trader/crates/runtime/src/run_spec.rs` and expose a conversion path from current app-config format into a run launch contract.

- [x] **Step 4: Keep current app config backward-compatible**

Preserve existing app config parsing so CLI and tests can continue using current TOML files as launch templates during migration.

- [x] **Step 5: Run verification**

Run:

```powershell
cargo test -p runtime run_spec
cargo check -p runtime -p config
```

- [x] **Step 6: Commit**

```powershell
git add trader/crates/runtime/src/run_spec.rs trader/crates/runtime/src/lib.rs trader/crates/config/src/config.rs trader/crates/runtime/tests/run_spec_tests.rs
git commit -m "feat: add canonical run spec model"
```

### Task 3: Upgrade `RuntimeManager` into a Run Registry and Supervisor

**Files:**
- Create: `trader/crates/runtime/src/run_registry.rs`
- Modify: `trader/crates/runtime/src/manager.rs`
- Modify: `trader/crates/runtime/src/lib.rs`
- Modify: `trader/crates/runtime/tests/runtime_manager_tests.rs`

**Interfaces:**
- Consumes: current `RuntimeManager::spawn/cancel/wait_for_idle`
- Produces: `RunRegistry`, `RunHandle`, `RunStatus`, `RuntimeManager::status`, `RuntimeManager::list_active`

- [x] **Step 1: Add failing runtime manager tests for status and metadata**

Extend runtime tests to assert:

- a spawned run reports `running`;
- a completed run reports terminal state before cleanup;
- duplicate spawn still fails cleanly;
- cancel transitions to a terminal status.

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test -p runtime runtime_manager`

Expected: FAIL because runtime status metadata is not persisted in memory.

- [x] **Step 3: Implement registry-backed supervision**

Split task-tracking logic out of `manager.rs` into `run_registry.rs` and teach `RuntimeManager` to track status, timestamps, and terminal result metadata.

- [x] **Step 4: Preserve current spawn contract while extending it**

Keep current Tokio-task execution and cancellation semantics so existing runtime implementations do not need a rewrite.

- [x] **Step 5: Run verification**

Run:

```powershell
cargo test -p runtime
cargo check -p runtime -p api
```

- [x] **Step 6: Commit**

```powershell
git add trader/crates/runtime/src/run_registry.rs trader/crates/runtime/src/manager.rs trader/crates/runtime/src/lib.rs trader/crates/runtime/tests/runtime_manager_tests.rs
git commit -m "feat: add run registry and supervision metadata"
```

### Task 4: Convert Run-Scoped API Reads to Explicit `run_id`

**Files:**
- Modify: `trader/crates/api/src/api.rs`
- Modify: `trader/crates/api/tests/api_tests.rs`
- Modify: `trader/crates/api/tests/backtest_api_tests.rs`
- Create: `trader/crates/api/tests/multi_run_api_tests.rs`
- Modify: `trader/docs/api.md`
- Modify: `trader/docs/web-admin-api.md`

**Interfaces:**
- Consumes: repository methods that already accept `run_id`
- Produces: explicit run-scoped endpoints such as `/api/v1/runs/{run_id}/orders`

- [x] **Step 1: Add failing API tests for explicit run-scoped reads**

Add tests that seed two runs in the database and verify querying one run does not leak the other's orders, fills, positions, balances, snapshots, or metrics.

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test -p api multi_run_api_tests`

Expected: FAIL because handlers still derive run scope from global config.

- [x] **Step 3: Replace global run inference in handlers**

Update `trader/crates/api/src/api.rs` so run-owned read endpoints use path or query parameters rather than `app_config.runtime.run_id`.

- [x] **Step 4: Keep aggregate endpoints only where semantics are cross-run**

Leave endpoints like `GET /api/v1/runs` and broad event summaries as cross-run reads, but remove hidden single-run assumptions from run-owned resources.

- [x] **Step 5: Update API docs**

Document the new route shapes and deprecate legacy single-run assumptions in `trader/docs/api.md` and `trader/docs/web-admin-api.md`.

- [x] **Step 6: Run verification**

Run:

```powershell
cargo test -p api multi_run_api_tests
cargo test -p api api_tests
cargo check -p api
```

- [x] **Step 7: Commit**

```powershell
git add trader/crates/api/src/api.rs trader/crates/api/tests/api_tests.rs trader/crates/api/tests/backtest_api_tests.rs trader/crates/api/tests/multi_run_api_tests.rs trader/docs/api.md trader/docs/web-admin-api.md
git commit -m "refactor: make run-scoped api reads explicit"
```

### Task 5: Launch Runs from `RunSpec` Instead of Server-Global Config

**Files:**
- Modify: `trader/crates/api/src/api.rs`
- Modify: `trader/crates/api/src/state.rs`
- Modify: `trader/crates/api/tests/api_tests.rs`
- Modify: `trader/crates/api/tests/backtest_api_tests.rs`
- Modify: `trader/crates/storage/src/repositories.rs`
- Modify: `trader/crates/storage/tests/runtime_repository_tests.rs`

**Interfaces:**
- Consumes: `RunSpec`, `RuntimeManager`, existing config snapshot and config-version persistence paths
- Produces: launch requests for Backtest, Replay, Paper, and Live that accept run input explicitly

- [x] **Step 1: Add failing launch tests for request-driven run creation**

Add tests that submit launch payloads for two different runs with different modes or strategies and verify both can be started without changing server startup config.

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test -p api start_run_from_request_payload`

Expected: FAIL because launch handlers still reload one config file from state.

- [x] **Step 3: Implement request models and launch conversion**

Teach launch handlers to accept request payloads or config-version references and build a `RunSpec` per request.

- [x] **Step 4: Persist per-run config binding or snapshot**

Ensure every launched run stores its config snapshot or config-version binding using the existing storage model.

- [x] **Step 5: Keep temporary compatibility path for current TOML-driven tests**

Allow existing tests and CLI flows to derive a `RunSpec` from current app-style TOML templates until the CLI migration lands.

- [x] **Step 6: Run verification**

Run:

```powershell
cargo test -p api start_run_from_request_payload
cargo test -p storage runtime_repository_tests
cargo check -p api -p storage -p runtime
```

- [x] **Step 7: Commit**

```powershell
git add trader/crates/api/src/api.rs trader/crates/api/src/state.rs trader/crates/api/tests/api_tests.rs trader/crates/api/tests/backtest_api_tests.rs trader/crates/storage/src/repositories.rs trader/crates/storage/tests/runtime_repository_tests.rs
git commit -m "feat: launch runs from explicit run specs"
```

### Task 6: Migrate WebSocket and Replay Control to Run Registry Semantics

**Files:**
- Modify: `trader/crates/api/src/ws.rs`
- Modify: `trader/crates/api/src/state.rs`
- Modify: `trader/crates/api/tests/ws_tests.rs`
- Modify: `trader/crates/api/src/api.rs`

**Interfaces:**
- Consumes: replay controllers keyed by `run_id`, runtime registry state
- Produces: websocket and replay control flows aligned with explicit run ownership

- [x] **Step 1: Add failing tests for multi-run websocket subscriptions or replay control**

Add tests that subscribe or control replay state for one run while another run is active, and assert no cross-run interference.

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test -p api ws_tests`

Expected: FAIL where websocket or replay control still assumes global runtime context.

- [x] **Step 3: Align websocket and replay control with explicit run state**

Update state access patterns so replay controllers and websocket event filtering use `run_id` and runtime registry state rather than any server-global active runtime assumption.

- [x] **Step 4: Run verification**

Run:

```powershell
cargo test -p api ws_tests
cargo check -p api
```

- [x] **Step 5: Commit**

```powershell
git add trader/crates/api/src/ws.rs trader/crates/api/src/state.rs trader/crates/api/tests/ws_tests.rs trader/crates/api/src/api.rs
git commit -m "refactor: align websocket and replay control with run registry"
```

### Task 7: Move CLI Flows onto `RunSpec` and Explicit Run Queries

**Files:**
- Modify: `trader/apps/trader-cli/src/main.rs`
- Modify: `trader/apps/trader-cli/tests/cli_tests.rs`
- Modify: `trader/crates/config/src/config.rs`

**Interfaces:**
- Consumes: current TOML config loading, run-scoped repository methods, new `RunSpec`
- Produces: CLI launch and query flows that no longer depend on a server-global active runtime concept

- [x] **Step 1: Add failing CLI tests for explicit run-scoped reads**

Add CLI tests that request orders or snapshots for one run out of many and assert the command requires or accepts explicit `run_id`.

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test -p trader-cli cli_tests`

Expected: FAIL where CLI still assumes run scope from one loaded config file.

- [x] **Step 3: Update CLI launch and query commands**

Refactor CLI code to build `RunSpec` from current TOML templates for launch paths and to use explicit `run_id` for read paths.

- [x] **Step 4: Run verification**

Run:

```powershell
cargo test -p trader-cli cli_tests
cargo check -p trader-cli
```

- [x] **Step 5: Commit**

```powershell
git add trader/apps/trader-cli/src/main.rs trader/apps/trader-cli/tests/cli_tests.rs trader/crates/config/src/config.rs
git commit -m "refactor: move cli flows to run spec and explicit run ids"
```

### Task 8: Normalize Strategy Binding and Document the New Mental Model

**Files:**
- Modify: `trader/crates/config/src/config.rs`
- Modify: `trader/docs/architecture.md`
- Modify: `trader/docs/strategy.md`
- Modify: `trader/docs/linux-deployment-runbook.md`

**Interfaces:**
- Consumes: `RunSpec`, existing strategy config fields
- Produces: clear separation among strategy definition, template, and run binding in docs and config conventions

- [x] **Step 1: Add failing documentation checklist review**

Review docs for any statement that still implies one server equals one mode or one strategy. Record mismatches in the working branch notes before editing.

- [x] **Step 2: Update strategy and architecture docs**

Document that:

- server is deployment-scoped;
- run is the unit of execution;
- strategy is selected per run;
- many modes and strategies can coexist.

- [x] **Step 3: Update operational runbook**

Explain how operators deploy one server and then launch many runs without restarting it for each strategy or mode.

- [x] **Step 4: Run verification**

Run:

```powershell
cargo check --workspace
```

- [x] **Step 5: Commit**

```powershell
git add trader/crates/config/src/config.rs trader/docs/architecture.md trader/docs/strategy.md trader/docs/linux-deployment-runbook.md
git commit -m "docs: document multi-run control plane model"
```

### Task 9: Final Integration Verification for Multi-Run Control Plane

**Files:**
- Modify as needed from previous tasks only

**Interfaces:**
- Consumes: all previous tasks
- Produces: end-to-end confidence that one server can manage many runs concurrently

- [x] **Step 1: Run focused API and runtime suites**

Run:

```powershell
cargo test -p runtime
cargo test -p api
cargo test -p trader-cli
```

- [x] **Step 2: Run workspace verification**

Run:

```powershell
cargo check --workspace
```

- [x] **Step 3: Run smoke scripts if they still match the new API shape**

Run the relevant local smoke scripts and update them if the new route shapes changed expected usage.

- [x] **Step 4: Update any residual docs or examples**

Remove lingering examples that still imply a server-global active runtime.

- [x] **Step 5: Commit**

```powershell
git add .
git commit -m "refactor: complete multi-run control plane migration"
```

---

## Self-Review

### Spec Coverage

This plan covers:

- server-vs-run config split;
- `RunSpec` introduction;
- runtime manager evolution;
- explicit run-scoped APIs;
- launch-path migration;
- websocket and replay alignment;
- CLI migration;
- strategy mental-model cleanup;
- integration verification.

### Placeholder Scan

No `TODO`, `TBD`, or deferred implementation placeholders were intentionally left in task definitions. Where exact code is not shown, the work item is a refactor or integration task bounded by explicit files, tests, and commands.

### Type Consistency

The plan uses consistent names:

- `ServerConfig`
- `RunSpec`
- `RunRegistry`
- `RunHandle`
- `RunStatus`

If implementation uncovers conflicting existing names, normalize them early in Task 2 or Task 3 rather than allowing aliases to spread.

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-06-26-multi-run-control-plane-refactor.md`. Two execution options:

1. Subagent-Driven (recommended) - I dispatch a fresh subagent per task, review between tasks, fast iteration

2. Inline Execution - Execute tasks in this session using executing-plans, batch execution with checkpoints

Which approach?
