# Config Approval/Publish Lifecycle Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Upgrade `configs` table from "run snapshot dump" to a proper version management + approval workflow, enabling config versioning, diff, rollback, approval states, and binding to strategy runs.

**Architecture:** `configs` table gains version number, approval state, parent version reference, and diff metadata. Config changes go through a lifecycle: draft → pending_review → approved → published → archived. Each state transition is logged to `event_store`. Configs are immutable once published; new versions are created on change.

**Tech Stack:** Rust workspace, SQLx SQLite, Axum, serde, serde_json, chrono, PowerShell CLI.

## Current Status (2026-07-08 Sync)

The local lifecycle MVP is implemented. Managed configs now support immutable version creation, target environment and rollout metadata, draft/pending_review/approved/published/archived state transitions, production independent-approver enforcement, lightweight production role policy, pending approval queue readback, latest/published/specific queries, structured JSON diff, rollback-to-new-draft, API routes, CLI commands, release/audit readback, and event-store logging for state changes. Remaining work is full production governance: authenticated RBAC, multi-environment permission matrices, and multi-person approval queues.

Run config binding is implemented through `RUN` snapshots in `configs` plus `run_config_versions` bindings. This avoided adding a `strategy_runs.config_version` column while still giving API/CLI readback for the config version used by local run entrypoints.

2026-07-08 sync: targeted local verification reconfirmed storage/API/CLI evidence for version creation, version queries, state transitions, production/staging role policy checks, independent production approver checks, pending approval readback, JSON diff, rollback-to-draft, release/audit readback, run config version bindings, and ops smoke coverage for pending approvals plus release/audit readback. This remains a local lifecycle/governance MVP: authenticated RBAC, multi-environment permission matrices, multi-person approval queues, and production change reports are still follow-up work.

| Area | Status | Evidence | Remaining |
| --- | --- | --- | --- |
| Run config snapshots | Done for API-launched and CLI-launched runs | `record_run_config_snapshot` writes `configs`, `config_releases` and `run_config_versions`; Backtest, Paper, Replay and Live API starts bind run config versions; Backtest, Paper, and Replay CLI starts bind run config versions | None for current local run entrypoints |
| Release/readback surface | Done for local MVP | `GET /api/v1/configs/{config_id}/releases`, `GET /api/v1/runs/{run_id}/config-version`, `configs releases`, `runs config-version` | None for lightweight readback |
| Audit readback | Done for local MVP | `config_audits`, `record_config_audit`, API/CLI audit queries exist; state changes also write `event_store` category `config.state.changed` | Production audit reports remain follow-up work |
| Approval state machine | Done for local MVP | `ConfigState` and validated transitions exist in storage; API and CLI expose state updates; production transitions can enforce `release_manager` / `approver` roles when supplied | Full authenticated RBAC is not enforced |
| Config CRUD/diff/rollback workflow | Done for local MVP | Storage, API, and CLI support create/list/show/latest/published/diff/rollback | UI remains follow-up work |
| Production rollout governance | Partial local enforcement done | `target_env=production` publish requires an independent approver; target_env/rollout/approved_by/published_by are persisted and returned through API/CLI; pending production approvals can be listed through API/CLI | Add authenticated RBAC, multi-environment permission matrix, and multi-person approval queues |

---

## Scope

In scope:

- Config versioning: each config change creates a new version number.
- Approval states: draft → pending_review → approved → published → archived.
- Config diff between versions (structured JSON diff).
- Rollback: create new version from any previous version.
- Bind config to strategy run: record which config version was used.
- CLI commands for config management.
- API endpoints for config CRUD with approval workflow.
- Audit trail: every state change logged to `event_store`.

Out of scope:

- Multi-user authorization (single-user local system).
- Web UI for config management.
- Config encryption or secret management.
- Real-time config hot-reload during runs.
- Config templates or inheritance.

## File Map

### Storage

- Modify: `crates/storage/src/repositories.rs`
  - Add `create_config_version` — inserts new version with auto-incremented version number.
  - Add `get_config(name, version)` — fetch specific version.
  - Add `get_latest_config(name)` — fetch latest version.
  - Add `get_published_config(name)` — fetch latest published version.
  - Add `list_config_versions(name)` — list all versions with metadata.
  - Add `update_config_state(name, version, new_state, changed_by, reason)` — state transition.
  - Add `diff_configs(name, version_a, version_b)` — structured diff.
- Modify: `crates/storage/tests/storage_tests.rs`
  - Add config versioning tests.
  - Add state transition tests.
  - Add diff tests.

### API

- Modify: `crates/api/src/api.rs`
  - Add config CRUD endpoints with approval workflow.
- Modify: `crates/api/tests/api_tests.rs`
  - Add route tests.
- Modify: `docs/api.md`
  - Document config endpoints.

### CLI

- Modify: `apps/trader-cli/src/main.rs`
  - Add config management commands.
- Modify: `apps/trader-cli/tests/cli_tests.rs`
  - Add config management command tests.

### Runtime

- Modify: `crates/runtime/src/runtime.rs`
  - Bind config version to run at startup.
- Modify: `crates/paper/src/paper.rs`
  - Same: record config version when run starts.

### Documentation

- Modify: `docs/分析.md`
- Modify: `docs/roadmap.md`

---

## Acceptance Gates

Every task must preserve:

- `cargo test -p storage`
- `cargo test -p api`
- `cargo test -p paper`
- `cargo test -p backtest`
- `powershell -ExecutionPolicy Bypass -File .\scripts\v1-smoke.ps1`
- `bash ./scripts/check-db-boundary`
- `bash ./scripts/check-storage-dto-boundary`
- `bash ./scripts/check-api-read-model-boundary`

New gates:

- `cargo test -p storage config_versioning` — version creation and retrieval.
- `cargo test -p storage config_state_transition` — approval workflow.
- `cargo test -p storage config_diff` — structured diff.
- `cargo test -p api config_crud` — API config endpoints.

---

## Task 1: Extend Storage for Config Versioning

**Files:**

- Modify: `crates/storage/src/repositories.rs`
- Modify: `crates/storage/tests/storage_tests.rs`

- [x] **Step 1: Define config version types**

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConfigVersion {
    pub name: String,
    pub version: u32,
    pub content_json: String,
    pub state: ConfigState,
    pub parent_version: Option<u32>,
    pub created_by: String,
    pub created_at_ms: i64,
    pub state_changed_at_ms: i64,
    pub state_changed_by: String,
    pub state_change_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ConfigState {
    Draft,
    PendingReview,
    Approved,
    Published,
    Archived,
}
```

- [x] **Step 2: Add create_config_version**

```rust
pub async fn create_config_version(&self, config: &NewConfigVersion) -> StorageResult<u32> {
    // Get max version for this name
    // Insert new version = max + 1
    // Return version number
}
```

- [x] **Step 3: Add get/list methods**

```rust
pub async fn get_config(&self, name: &str, version: u32) -> StorageResult<Option<ConfigVersion>>
pub async fn get_latest_config(&self, name: &str) -> StorageResult<Option<ConfigVersion>>
pub async fn get_published_config(&self, name: &str) -> StorageResult<Option<ConfigVersion>>
pub async fn list_config_versions(&self, name: &str) -> StorageResult<Vec<ConfigVersion>>
```

- [x] **Step 4: Add state transition**

```rust
pub async fn update_config_state(
    &self,
    name: &str,
    version: u32,
    new_state: ConfigState,
    changed_by: &str,
    reason: Option<&str>,
) -> StorageResult<()> {
    // Validate state transition (e.g., Draft → PendingReview, not Draft → Published)
    // Update state, state_changed_at_ms, state_changed_by, state_change_reason
}
```

Valid transitions:
- Draft → PendingReview, Archived
- PendingReview → Approved, Draft (reject)
- Approved → Published, Archived
- Published → Archived
- Archived → (terminal, no transitions)

- [x] **Step 5: Add config diff**

```rust
pub async fn diff_configs(&self, name: &str, version_a: u32, version_b: u32) -> StorageResult<ConfigDiff> {
    // Fetch both versions
    // Parse JSON content
    // Return structured diff (added keys, removed keys, changed values)
}
```

- [x] **Step 6: Add storage tests**

```rust
#[tokio::test]
async fn config_version_auto_increment() {
    // Create version 1, then version 2
    // Assert: version numbers are 1 and 2
}

#[tokio::test]
async fn config_state_transition() {
    // Create draft → transition to pending_review → approved → published
    // Assert: each state is recorded correctly
}

#[tokio::test]
async fn config_invalid_state_transition_rejected() {
    // Try draft → published directly
    // Assert: error returned
}

#[tokio::test]
async fn config_diff_shows_changes() {
    // Create two versions with different content
    // Diff them
    // Assert: changed keys are identified
}
```

- [x] **Step 7: Run storage tests**

```powershell
cargo test -p storage config_versioning
cargo test -p storage config_state_transition
cargo test -p storage config_diff
```

Expected: pass.

- [x] **Step 8: Commit**

```powershell
git add crates/storage
git commit -m "feat: config versioning and approval storage"
```

---

## Task 2: Add Config API Endpoints

**Files:**

- Modify: `crates/api/src/api.rs`
- Modify: `crates/api/tests/api_tests.rs`
- Modify: `docs/api.md`

- [x] **Step 1: Add API endpoints**

```
POST   /api/v1/configs                          — create new config version
GET    /api/v1/configs/{name}                    — list versions
GET    /api/v1/configs/{name}/latest             — get latest version
GET    /api/v1/configs/{name}/published          — get published version
GET    /api/v1/configs/{name}/{version}           — get specific version
PUT    /api/v1/configs/{name}/{version}/state     — transition state
GET    /api/v1/configs/{name}/diff?v1=1&v2=2      — diff two versions
POST   /api/v1/configs/{name}/{version}/rollback  — create new version from old
```

- [x] **Step 2: Add API request/response types**

```rust
#[derive(Deserialize)]
struct CreateConfigRequest {
    name: String,
    content_json: String,
    created_by: String,
}

#[derive(Deserialize)]
struct UpdateStateRequest {
    new_state: ConfigState,
    changed_by: String,
    reason: Option<String>,
}

#[derive(Serialize)]
struct ConfigVersionResponse {
    name: String,
    version: u32,
    content: serde_json::Value,
    state: ConfigState,
    parent_version: Option<u32>,
    created_by: String,
    created_at_ms: i64,
}

#[derive(Serialize)]
struct ConfigDiffResponse {
    name: String,
    version_a: u32,
    version_b: u32,
    added: Vec<String>,
    removed: Vec<String>,
    changed: Vec<ConfigDiffEntry>,
}
```

- [x] **Step 3: Implement handlers**

Implement each endpoint handler with proper validation:
- State transition validation (reject invalid transitions).
- Rollback: copy content from old version, create as new Draft.

- [x] **Step 4: Add API tests**

```rust
#[tokio::test]
async fn create_and_list_config_versions() { ... }
#[tokio::test]
async fn config_state_transition_via_api() { ... }
#[tokio::test]
async fn config_diff_via_api() { ... }
#[tokio::test]
async fn config_rollback_creates_new_version() { ... }
```

- [x] **Step 5: Document endpoints**

Add to `docs/api.md` with full endpoint documentation.

- [x] **Step 6: Run tests**

```powershell
cargo test -p api config
bash ./scripts/check-api-read-model-boundary
```

Expected: pass.

- [x] **Step 7: Commit**

```powershell
git add crates/api docs/api.md
git commit -m "feat: config CRUD and approval API"
```

---

## Task 3: Add CLI Config Commands

**Files:**

- Modify: `apps/trader-cli/src/main.rs`

- [x] **Step 1: Add CLI commands**

```
trader config create --name <name> --file <path> [--created-by <user>]
trader config list --name <name>
trader config show --name <name> [--version <v>]
trader config show --name <name> --published
trader config diff --name <name> --v1 <v1> --v2 <v2>
trader config rollback --name <name> --version <v>
trader config approve --name <name> --version <v> [--reason <msg>]
trader config publish --name <name> --version <v> [--reason <msg>]
trader config archive --name <name> --version <v> [--reason <msg>]
```

- [x] **Step 2: Implement commands**

Each command calls the storage repository methods. CLI reads config files from disk for `create`.

- [x] **Step 3: Add CLI tests**

```rust
#[test]
fn config_create_and_list() { ... }
#[test]
fn config_diff_output() { ... }
```

- [x] **Step 4: Commit**

```powershell
git add apps/trader-cli
git commit -m "feat: config management CLI commands"
```

---

## Task 4: Bind Config Version to Runs

**Files:**

- Modify: `crates/storage/src/repositories.rs`
- Modify: `crates/paper/src/paper.rs`
- Modify: `crates/backtest/src/backtest.rs`
- Modify: `crates/runtime/src/runtime.rs`

- [x] **Step 1: Add config_version field to strategy_runs**

Implemented through `RUN` snapshots in `configs` plus `run_config_versions` bindings rather than a new `strategy_runs.config_version` column:

```rust
// In the run creation:
let config_version = db.get_published_config(&config_name).await?;
// Store config_name and config_version in strategy_runs.config_json or new column
```

- [x] **Step 2: Record config version at run start**

In paper/backtest runtime initialization:
1. If config name is provided, look up published version.
2. Record config name + version in run metadata.
3. If no published version exists, log warning but allow run (backward compatible).

- [x] **Step 3: Add run-config binding query**

```rust
pub async fn get_runs_by_config(&self, config_name: &str, version: Option<u32>) -> StorageResult<Vec<StoredStrategyRun>> {
    // List all runs that used a specific config version
}
```

- [x] **Step 4: Add tests**

```rust
#[tokio::test]
async fn run_records_config_version() {
    // Create config, publish it, start paper run
    // Assert: run metadata includes config name and version
}
```

- [x] **Step 5: Commit**

```powershell
git add crates/storage crates/paper crates/backtest crates/runtime
git commit -m "feat: bind config version to strategy runs"
```

---

## Task 5: Add Audit Trail for Config Changes

**Files:**

- Modify: `crates/storage/src/repositories.rs`

- [x] **Step 1: Log state transitions to event_store**

```rust
pub async fn update_config_state(&self, ...) -> StorageResult<()> {
    // ... update config state ...
    // Also insert into event_store:
    self.insert_event(NewEventRecord {
        event_id: Uuid::new_v4().to_string(),
        ts_ms: now_ms(),
        source: "config-lifecycle".to_string(),
        category: "config.state.changed".to_string(),
        payload_json: serde_json::json!({
            "name": name,
            "version": version,
            "old_state": old_state,
            "new_state": new_state,
            "changed_by": changed_by,
            "reason": reason,
        }).to_string(),
    }).await?;
}
```

- [x] **Step 2: Add test**

```rust
#[tokio::test]
async fn config_state_change_logged_to_event_store() {
    // Transition config state
    // Query event_store for config.state.changed events
    // Assert: event exists with correct payload
}
```

- [x] **Step 3: Commit**

```powershell
git add crates/storage
git commit -m "feat: config state change audit trail"
```

---

## Task 6: Update Documentation

**Files:**

- Modify: `docs/分析.md`
- Modify: `docs/roadmap.md`

- [x] **Step 1: Update `docs/分析.md`**

Update config section from "run snapshot dump" to "version management with approval workflow".

- [x] **Step 2: Update `docs/roadmap.md`**

Add "Config Lifecycle" milestone.

- [x] **Step 3: Commit**

```powershell
git add docs
git commit -m "docs: update config lifecycle status"
```

---

## Implementation Order

1. Task 1: Storage extensions (foundation).
2. Task 2: API endpoints.
3. Task 3: CLI commands.
4. Task 4: Bind config to runs.
5. Task 5: Audit trail.
6. Task 6: Documentation.

## Risks and Controls

- **Risk:** Config versioning adds complexity to run startup.
  - **Control:** Backward compatible — if no config name provided, use existing behavior. Config binding is optional.
- **Risk:** State transition validation bugs allow invalid transitions.
  - **Control:** Exhaustive test coverage for every valid and invalid transition.
- **Risk:** Large config JSON causes storage bloat.
  - **Control:** Config content is typically small (< 10KB). No special handling needed for MVP.
- **Risk:** Rollback creates confusion about which version is "current".
  - **Control:** Rollback creates a new Draft version. Must go through approval again. Published version is always the "current" one.

## Success Criteria

The project is materially improved when:

- Configs have version numbers and approval states.
- State transitions are validated and logged.
- Config diff shows changes between versions.
- Rollback creates a new version from any previous version.
- Strategy runs record which config version they used.
- CLI provides full config management commands.
- API provides config CRUD with approval workflow.
- Existing MVP smoke still passes.
