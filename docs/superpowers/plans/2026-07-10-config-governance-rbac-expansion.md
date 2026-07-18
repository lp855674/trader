# Config Governance RBAC Expansion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend the local config lifecycle MVP into a locally verifiable governance surface with explicit role policy, multi-environment permission rules, and multi-approver queues without claiming production identity or live-money readiness.

**Architecture:** Keep config governance enforcement inside `storage` so API and CLI paths share the same rules. Model the next step as deterministic local policy data attached to managed config transitions: actor role, target environment, approval requirements, and approval records. API and CLI remain thin request/readback surfaces over the shared storage policy and queue.

**Tech Stack:** Rust workspace crates (`storage`, `api`, `trader-cli`), SQLx SQLite migrations, Axum JSON routes, clap CLI, PowerShell operator smoke, existing `configs`, `config_releases`, `config_audits`, and `event_store` tables.

## Current Status (2026-07-10 Sync)

The repository already has the local config lifecycle MVP:

- Managed configs support `draft -> pending_review -> approved -> published -> archived`.
- Config versions carry `target_env`, rollout metadata, `approved_by`, `approved_at`, `published_by`, and `published_at`.
- Production publish requires independent approval.
- Staging and production transitions enforce a lightweight role policy when `actor_role` is provided.
- Pending approval queues are readable through API and CLI.
- Config releases, audits, run-version bindings, JSON diff, and rollback readback are covered by storage/API/CLI tests and `ops-smoke.ps1`.

The remaining local engineering gap is not another config state machine. It is governance breadth:

- Role rules are hard-coded and not exposed as an explicit policy/readback surface.
- Approval queue entries do not express required roles or required approval counts.
- Production still has a single-approver local model; there is no deterministic multi-person approval queue.
- There is no committed local report shape for production change governance.

This plan intentionally excludes real authentication, secret management, external identity providers, web UI, and real-money deployment approval. Actors remain explicit request fields in local tests and smoke scripts.

## Global Constraints

- Do not weaken existing production independent-approver enforcement.
- Do not require network connectivity or external identity services.
- Do not submit live-money orders.
- Preserve existing API and CLI config commands unless a new option is required for the new governance behavior.
- Keep generated `data/` evidence uncommitted; committed docs may summarize local run ids and results.
- Do not touch unrelated local notes such as `记录.md`.

---

## File Map

### Storage Policy And Persistence

- Modify: `crates/storage/src/repositories.rs`
  - Add explicit config governance policy structs and readback helpers.
  - Persist or derive required roles and required approval counts per target environment.
  - Add approval-record helpers if multi-approver quorum cannot be represented by current `approved_by` fields.
- Modify: `crates/storage/src/db.rs`
  - Add migration columns/tables only if the approval quorum requires durable per-approval rows.
- Modify: `crates/storage/tests/runtime_repository_tests.rs`
  - Cover role policy readback, environment-specific transition rules, multi-approver approval queue behavior, and release/audit side effects.

### API Surface

- Modify: `crates/api/src/api.rs`
  - Add policy/readback endpoint for config governance requirements.
  - Extend pending approval responses with required role/count fields.
  - Preserve existing state-transition route and route errors.
- Modify: `crates/api/tests/api_tests.rs`
  - Cover governance policy readback and multi-approver queue behavior through HTTP.

### CLI Surface

- Modify: `apps/trader-cli/src/main.rs`
  - Add `configs governance-policy` or equivalent readback command.
  - Extend pending approval output with required role/count fields.
  - Add optional approver identity inputs only where storage requires quorum tracking.
- Modify: `apps/trader-cli/tests/cli_tests.rs`
  - Cover CLI policy readback, queue formatting, and multi-approver publish blocking/release.

### Operator Smoke And Documentation

- Modify: `scripts/smoke/ops-smoke.ps1`
  - Add a credential-free governance smoke that proves staging and production policy/queue readback.
- Modify: `docs/roadmap.md`
  - Update config governance status and remaining production identity limits.
- Modify: `docs/分析.md`
  - Record the new local governance coverage and remaining production gaps.
- Create: `docs/config-governance-rbac-results-template.md`
  - Provide a local evidence template for policy/readback/quorum runs.

---

## Acceptance Gates

Every task must preserve:

- `cargo fmt`
- `cargo test -p storage`
- `cargo test -p api`
- `cargo test -p trader-cli`
- `powershell -ExecutionPolicy Bypass -File .\scripts\smoke\ops-smoke.ps1`
- `powershell -ExecutionPolicy Bypass -File .\scripts\check\verify.ps1`
- `powershell -ExecutionPolicy Bypass -File .\scripts\check\clippy.ps1`
- `powershell -ExecutionPolicy Bypass -File .\scripts\check\check-db-boundary.ps1`
- `powershell -ExecutionPolicy Bypass -File .\scripts\check\check-storage-dto-boundary.ps1`
- `powershell -ExecutionPolicy Bypass -File .\scripts\check\check-api-read-model-boundary.ps1`

New focused gates:

- `cargo test -p storage config_governance`
- `cargo test -p api config_governance`
- `cargo test -p trader-cli config_management_commands`
- `powershell -ExecutionPolicy Bypass -File .\scripts\smoke\ops-smoke.ps1`

---

## Task 1: Make Config Governance Policy Explicit

**Files:**
- Modify: `crates/storage/src/repositories.rs`
- Modify: `crates/storage/tests/runtime_repository_tests.rs`

**Produces:**
- A storage-level `ConfigGovernancePolicy` readback surface that API/CLI can share.
- Tests proving current staging/production role rules are represented as explicit policy, not only embedded errors.

- [x] Add failing storage tests for `config_governance_policy_returns_environment_rules`.
- [x] Define policy structs with environment, transition, required role, required approval count, and independent-actor requirement.
- [x] Implement a storage readback helper that returns rules for local, staging, and production.
- [x] Verify focused storage governance tests pass.

## Task 2: Add Multi-Approver Queue Semantics

**Files:**
- Modify: `crates/storage/src/db.rs` if durable approval rows are needed.
- Modify: `crates/storage/src/repositories.rs`
- Modify: `crates/storage/tests/runtime_repository_tests.rs`

**Produces:**
- Production config publish can require a deterministic local approval quorum.
- Pending approvals can show how many approvals are present and how many are still required.

- [x] Add failing storage tests for production publish blocked with one approval when policy requires two approvers.
- [x] Add failing storage tests for publish success after two independent approvers.
- [x] Add migration support for per-approval records if current `approved_by` cannot represent the quorum without ambiguity.
- [x] Implement quorum-aware approval recording and publish enforcement.
- [x] Preserve existing single independent-approver behavior for environments whose policy requires only one approval.
- [x] Verify storage tests and migration boundary checks.

## Task 3: Expose Governance Policy And Queue Through API

**Files:**
- Modify: `crates/api/src/api.rs`
- Modify: `crates/api/tests/api_tests.rs`

**Produces:**
- API clients can read config governance policy.
- Pending approval responses include required role/count evidence.
- State transitions return clear blocking errors when quorum is incomplete.

- [x] Add failing API tests for `GET /api/v1/config-governance/policy`.
- [x] Add failing API tests for pending approval queue fields `required_role`, `required_approvals`, and `approval_count`.
- [x] Add failing API tests for production publish blocked until quorum is met.
- [x] Implement API response structs and route wiring using storage policy helpers.
- [x] Verify focused API governance tests pass.

## Task 4: Expose Governance Policy And Queue Through CLI

**Files:**
- Modify: `apps/trader-cli/src/main.rs`
- Modify: `apps/trader-cli/tests/cli_tests.rs`

**Produces:**
- Operators can inspect local governance policy from the CLI.
- Pending approval CLI output includes enough information to act without checking source code.

- [x] Add failing CLI tests for `configs governance-policy`.
- [x] Add failing CLI tests for pending approval queue required role/count output.
- [x] Add failing CLI tests for quorum-blocked production publish and successful publish after independent approvals.
- [x] Implement CLI command and formatting using storage readback helpers.
- [x] Verify focused CLI config management tests pass.

## Task 5: Update Operator Smoke And Docs

**Files:**
- Modify: `scripts/smoke/ops-smoke.ps1`
- Modify: `docs/roadmap.md`
- Modify: `docs/分析.md`
- Create: `docs/config-governance-rbac-results-template.md`
- Modify: `docs/superpowers/plans/2026-07-10-config-governance-rbac-expansion.md`

**Produces:**
- Credential-free local smoke covers governance policy and queue readback.
- Docs distinguish local RBAC/quorum governance from real authenticated production authorization.

- [x] Extend `ops-smoke.ps1` to call policy readback and assert queue fields.
- [x] Update roadmap and analysis docs with completed local governance scope and remaining identity limits.
- [x] Add results template for local governance verification.
- [x] Mark completed plan tasks with verification evidence.
- [x] Verify `ops-smoke.ps1` and docs-related boundary scripts.

## Task 6: Full Verification And Commit

**Files:**
- No additional source files beyond prior tasks.

**Produces:**
- Clean local verification and a focused commit that lands the config governance expansion.

- [x] Run `cargo fmt`.
- [x] Run focused storage/API/CLI governance tests.
- [x] Run `powershell -ExecutionPolicy Bypass -File .\scripts\smoke\ops-smoke.ps1`.
- [x] Run `powershell -ExecutionPolicy Bypass -File .\scripts\check\verify.ps1`.
- [x] Run `powershell -ExecutionPolicy Bypass -File .\scripts\check\clippy.ps1`.
- [x] Run the three boundary scripts.
- [x] Commit only files related to this plan.

---

## Execution Order

1. Task 1: Make current governance rules explicit before changing behavior.
2. Task 2: Add quorum semantics at the shared storage boundary.
3. Task 3: Expose policy and quorum state through API.
4. Task 4: Expose policy and quorum state through CLI.
5. Task 5: Add smoke/docs/evidence template.
6. Task 6: Full verification and commit.

Do not start API or CLI wiring before storage policy tests pass. Do not claim production RBAC: this plan proves local deterministic governance only, with explicit actor fields and no external identity provider.

## Exit Criteria

This plan is complete when:

- Config governance policy is readable from storage, API, and CLI.
- Pending approvals expose required role and approval-count evidence.
- Production publish can be blocked until the configured independent approval quorum is met.
- Local operator smoke proves the policy and queue readback path without external services.
- Docs clearly state that authenticated RBAC, SSO/IdP integration, and live-money deployment authorization remain production follow-up work.
