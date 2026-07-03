# IBKR Paper Gateway Long Run Verification Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Produce repeatable evidence that the IBKR paper Gateway path works across ReadOnly, AutoRun, and Soak stages without committing generated `data/` output.

**Architecture:** Reuse the existing IBKR scripts and paper-readiness runbook instead of adding a new runner. The operator supplies a real paper account id and a running local TWS / IB Gateway; each stage writes a JSON summary under `data/`, and the result document records paths, status, `failure_class`, and stop/go decisions.

**Tech Stack:** PowerShell verification scripts, `trader` CLI, IBKR paper Gateway on `127.0.0.1:7497`, SQLite evidence under `data/`, Markdown result document.

## Global Constraints

- Do not use a real-money IBKR account.
- Do not commit generated `data/` evidence.
- Do not embed the paper account id in committed configs; pass it with `-AccountId`.
- ReadOnly must complete before AutoRun.
- AutoRun and Soak may submit paper orders only with explicit confirmation switches.
- Stop immediately if any stage reports `failure_class` other than `ok`.
- If a stage fails, preserve the summary path and first failing log path in the result document.

---

## Verification Matrix

| Stage | Command | Expected Evidence | Success Signal |
| --- | --- | --- | --- |
| Local readiness | `scripts/paper-readiness.ps1` | `data/paper-readiness/{readiness_id}/summary.json` | `status = completed`, all five gates `ok` |
| IBKR ReadOnly | `scripts/ibkr-paper-test-guide.ps1 -Stage ReadOnly` | `data/ibkr-paper-test/read-only-{id}/summary.json` | `status = completed`, `failure_class = ok`, `failed_check = ""` |
| IBKR AutoRun | `scripts/ibkr-paper-test-guide.ps1 -Stage AutoRun -ConfirmAutoRun` | `data/ibkr-paper-runs/{run_id}/summary.json` | `status = completed`, `failure_class = ok`, `order_submit = enabled`, Gateway checks `ok` |
| IBKR Soak | `scripts/ibkr-paper-soak.ps1 -ConfirmIbkrPaperOrder` | `data/ibkr-paper-soak/{soak_id}/summary.json` | `status = completed`, `failure_class = ok`, all requested iterations complete |

## File Structure

- Read: `docs/paper-readiness-runbook.md`
  - Source of operator-facing commands, expected summary paths, and `failure_class` handling.
- Read: `scripts/paper-readiness.ps1`
  - Local no-Gateway readiness gate.
- Read: `scripts/ibkr-paper-test-guide.ps1`
  - ReadOnly and AutoRun Gateway stages.
- Read: `scripts/ibkr-paper-soak.ps1`
  - Multi-iteration Gateway soak stage.
- Create: `docs/ibkr-paper-gateway-long-run-results-<run-id>.md`
  - Human-readable evidence summary. Commit this document only after replacing account-sensitive values with non-secret labels such as `DU...`.

---

### Task 1: Confirm Local Readiness Gate

**Files:**
- Read: `scripts/paper-readiness.ps1`
- Read: `data/paper-readiness/<readiness_id>/summary.json`
- Create: `docs/ibkr-paper-gateway-long-run-results-<run-id>.md`

**Interfaces:**
- Consumes: no external Gateway.
- Produces: a result document with the local readiness summary path and gate statuses.

- [x] **Step 1: Run the no-Gateway readiness gate**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\paper-readiness.ps1
```

Expected: command exits `0` and prints `Paper readiness summary: data/paper-readiness/<readiness_id>/summary.json`.

- [x] **Step 2: Inspect the readiness summary**

```powershell
Get-Content .\data\paper-readiness\<readiness_id>\summary.json
```

Expected: `status` is `completed`; `reference_data_observable`, `reference_data_retry_tests`, `ibkr_paper_local_dry_run`, `ibkr_read_only_summary_behavior`, and `ibkr_soak_summary_behavior` are all `ok`.

- [x] **Step 3: Create the result document**

Create `docs/ibkr-paper-gateway-long-run-results-<run-id>.md`:

```markdown
# IBKR Paper Gateway Long Run Results: <run-id>

## Scope

- Account: `DU...`
- Gateway host: `127.0.0.1`
- Gateway port: `7497`
- Client id: `1`
- Soak iterations: `3`

## Evidence

| Stage | Summary | Status | failure_class | Notes |
| --- | --- | --- | --- | --- |
| Local readiness | `data/paper-readiness/<readiness_id>/summary.json` | completed | ok | All five local gates passed. |
| ReadOnly | pending | pending | pending | Not run yet. |
| AutoRun | pending | pending | pending | Not run yet. |
| Soak | pending | pending | pending | Not run yet. |

## Decision

Gateway verification is not complete until ReadOnly, AutoRun, and Soak all report `failure_class = ok`.
```

- [x] **Step 4: Commit the readiness result skeleton**

```powershell
git add docs/ibkr-paper-gateway-long-run-results-<run-id>.md
git commit -m "docs: start ibkr paper gateway verification results"
```

---

### Task 2: Run IBKR ReadOnly Gateway Verification

**Files:**
- Read: `scripts/ibkr-paper-test-guide.ps1`
- Read: `data/ibkr-paper-test/read-only-<id>/summary.json`
- Modify: `docs/ibkr-paper-gateway-long-run-results-<run-id>.md`

**Interfaces:**
- Consumes: running IBKR TWS / Gateway in Paper Trading mode and a real `DU...` paper account id.
- Produces: read-only Gateway evidence without order submission.

- [ ] **Step 1: Verify Gateway prerequisites**

Confirm these operator-side settings before running the command:

```text
TWS / IB Gateway is in Paper Trading mode
API socket clients are enabled
Socket port is 7497
Account id starts with DU
No real-money account is selected
```

Expected: all five statements are true.

- [ ] **Step 2: Run ReadOnly verification**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-test-guide.ps1 `
  -Stage ReadOnly `
  -AccountId DU12345 `
  -GatewayHost 127.0.0.1 `
  -Port 7497 `
  -ClientId 1
```

Expected: command exits `0` and prints `IBKR paper read-only summary: data/ibkr-paper-test/read-only-<id>/summary.json`.

- [ ] **Step 3: Inspect ReadOnly summary**

```powershell
Get-Content .\data\ibkr-paper-test\read-only-<id>\summary.json
```

Expected: `status = completed`, `failure_class = ok`, `failed_check = ""`, and all read-only checks have exit code `0`.

- [ ] **Step 4: Update the result document**

Replace the ReadOnly row:

```markdown
| ReadOnly | `data/ibkr-paper-test/read-only-<id>/summary.json` | completed | ok | Gateway read-only account, open orders, executions, reconcile, recover, and next-order-id checks passed. |
```

If the stage failed, record the actual `failure_class`, `failed_check`, and the failing `.log` path instead of continuing.

- [ ] **Step 5: Commit ReadOnly evidence summary**

```powershell
git add docs/ibkr-paper-gateway-long-run-results-<run-id>.md
git commit -m "docs: record ibkr paper readonly verification"
```

---

### Task 3: Run IBKR AutoRun Paper Verification

**Files:**
- Read: `scripts/ibkr-paper-test-guide.ps1`
- Read: `scripts/ibkr-paper-run.ps1`
- Read: `data/ibkr-paper-runs/<run_id>/summary.json`
- Modify: `docs/ibkr-paper-gateway-long-run-results-<run-id>.md`

**Interfaces:**
- Consumes: successful Task 2 ReadOnly evidence.
- Produces: one confirmed paper order-submitting AutoRun with post-run Gateway checks.

- [ ] **Step 1: Confirm ReadOnly passed**

Open the result document and verify the ReadOnly row is:

```markdown
| ReadOnly | `data/ibkr-paper-test/read-only-<id>/summary.json` | completed | ok | Gateway read-only account, open orders, executions, reconcile, recover, and next-order-id checks passed. |
```

Expected: ReadOnly is complete with `failure_class = ok`.

- [ ] **Step 2: Run AutoRun with explicit confirmation**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-test-guide.ps1 `
  -Stage AutoRun `
  -AccountId DU12345 `
  -GatewayHost 127.0.0.1 `
  -Port 7497 `
  -ClientId 1 `
  -ConfirmAutoRun
```

Expected: command exits `0` and the runner prints `summary : data/ibkr-paper-runs/<run_id>/summary.json`.

- [ ] **Step 3: Inspect AutoRun summary**

```powershell
Get-Content .\data\ibkr-paper-runs\<run_id>\summary.json
```

Expected: `status = completed`, `failure_class = ok`, `order_submit = enabled`, `gateway_checks.status = completed`, and `gateway_checks.failure_class = ok`.

- [ ] **Step 4: Update the result document**

Replace the AutoRun row:

```markdown
| AutoRun | `data/ibkr-paper-runs/<run_id>/summary.json` | completed | ok | Confirmed paper order run completed and post-run Gateway checks passed. |
```

If the stage failed, record the actual `failure_class`, `gateway_checks.failed_check`, and the summary path instead of continuing.

- [ ] **Step 5: Commit AutoRun evidence summary**

```powershell
git add docs/ibkr-paper-gateway-long-run-results-<run-id>.md
git commit -m "docs: record ibkr paper autorun verification"
```

---

### Task 4: Run IBKR Soak Verification

**Files:**
- Read: `scripts/ibkr-paper-soak.ps1`
- Read: `data/ibkr-paper-soak/<soak_id>/summary.json`
- Modify: `docs/ibkr-paper-gateway-long-run-results-<run-id>.md`

**Interfaces:**
- Consumes: successful Task 3 AutoRun evidence.
- Produces: multi-iteration paper Gateway soak evidence.

- [ ] **Step 1: Confirm AutoRun passed**

Open the result document and verify the AutoRun row is:

```markdown
| AutoRun | `data/ibkr-paper-runs/<run_id>/summary.json` | completed | ok | Confirmed paper order run completed and post-run Gateway checks passed. |
```

Expected: AutoRun is complete with `failure_class = ok`.

- [ ] **Step 2: Run a three-iteration soak**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-soak.ps1 `
  -Iterations 3 `
  -AccountId DU12345 `
  -GatewayHost 127.0.0.1 `
  -Port 7497 `
  -ClientId 1 `
  -ConfirmIbkrPaperOrder
```

Expected: command exits `0` and prints `IBKR paper soak summary: data/ibkr-paper-soak/<soak_id>/summary.json`.

- [ ] **Step 3: Inspect soak summary**

```powershell
Get-Content .\data\ibkr-paper-soak\<soak_id>\summary.json
```

Expected: `status = completed`, `failure_class = ok`, `iterations_requested = 3`, `iterations_completed = 3`, and no iteration has non-`ok` `failure_class`.

- [ ] **Step 4: Update the result document**

Replace the Soak row and final decision:

```markdown
| Soak | `data/ibkr-paper-soak/<soak_id>/summary.json` | completed | ok | Three confirmed paper order iterations completed without residual open orders. |

## Decision

IBKR paper Gateway verification passed for ReadOnly, AutoRun, and Soak. The remaining production gap is broader real-money readiness, not the paper Gateway validation path.
```

If the stage failed, record `failed_iteration`, `first_failed_log`, and the actual `failure_class`.

- [ ] **Step 5: Commit soak evidence summary**

```powershell
git add docs/ibkr-paper-gateway-long-run-results-<run-id>.md
git commit -m "docs: record ibkr paper soak verification"
```

---

## Acceptance Gates

- Local readiness summary has `status = completed` and all five gates `ok`.
- ReadOnly summary has `failure_class = ok`.
- AutoRun summary has `failure_class = ok` and `order_submit = enabled`.
- Soak summary has `failure_class = ok` and all requested iterations completed.
- `git status --short` does not show generated `data/` files.
- The committed result document redacts the paper account id as `DU...`.

## Failure Handling

| failure_class | Action |
| --- | --- |
| `gateway_unreachable` | Stop. Confirm Gateway is running in Paper Trading mode, API socket clients are enabled, host/port match, and no conflicting session owns the client id. |
| `account_mismatch` | Stop. Re-run with the `DU...` account id returned by Gateway managed accounts. |
| `command_failed` | Stop. Inspect the failing ReadOnly command log next to the summary. |
| `iteration_failed` | Stop. Inspect `first_failed_log` in the soak summary, then open the referenced iteration runner summary. |
| `open_orders_remaining` | Stop. Inspect remote open orders and cancel only with an explicit paper cancel confirmation command. |

## Self-Review

- Spec coverage: The plan covers local readiness, ReadOnly, AutoRun, Soak, evidence paths, failure classes, generated data hygiene, and commit boundaries.
- Placeholder scan: No placeholder markers are used as implementation steps; `<run-id>`, `<id>`, and `<soak_id>` are operator-filled evidence identifiers created by the scripts.
- Type consistency: Script parameters and summary fields match `docs/paper-readiness-runbook.md`, `scripts/ibkr-paper-test-guide.ps1`, `scripts/ibkr-paper-run.ps1`, and `scripts/ibkr-paper-soak.ps1`.
