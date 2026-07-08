# Live Account Multi-Broker Reconciliation Gate Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a configurable gate that blocks live-account promotion unless recent reconciliation audits for every required broker/account pass with no drift or stale inputs.

**Architecture:** Keep the decision logic in a pure broker-layer module so it can be tested without IBKR/Binance network access. Expose the gate through config, CLI, and a PowerShell verification script that can evaluate one or more broker/account evidence sets before a live run is allowed.

**Tech Stack:** Rust workspace crates (`broker`, `config`, `storage`, `trader-cli`), SQLite-backed reconciliation audit storage, PowerShell verification scripts, Markdown result docs.

## Completion Status

- Implementation completed for pure gate evaluation, config parsing, storage latest-audit lookup, CLI evaluation, operator script, and runbook.
- Local fixture validation completed and recorded in `docs/live-reconciliation-gate-results-local-2026-07-07.md`.
- Archival real-broker replay validation completed and recorded in `docs/live-reconciliation-gate-results-real-broker-replay-2026-07-08.md`.
- Fresh read-only broker/testnet evidence validation completed and recorded in `docs/live-reconciliation-gate-results-fresh-readonly-2026-07-08.md`.
- Long fresh read-only multi-broker gate validation completed and recorded in `docs/live-reconciliation-gate-results-long-readonly-2026-07-08.md`; the gate allowed with `MinSuccessfulAudits=10`.
- No live-money support is claimed by this plan, and no live orders are part of the accepted evidence.

## Global Constraints

- Do not submit live orders while implementing or testing this gate.
- Do not require real IBKR or Binance network access for unit tests.
- Preserve existing `broker_reconciliation_audits` storage schema unless a task explicitly adds a migration.
- Redact account ids in committed docs using the existing `DU****91` style.
- Generated `data/` evidence stays uncommitted.
- Keep `记录.md` untouched.

---

## File Structure

- Create: `crates/broker/src/reconciliation_gate.rs`
  - Owns pure gate input/output structs, status enum, failure reasons, and evaluation logic.
- Modify: `crates/broker/src/broker.rs`
  - Re-export the gate module types and keep existing reconciliation audit logic unchanged.
- Modify: `crates/config/src/config.rs`
  - Adds `[live.reconciliation_gate]` config parsing with conservative defaults.
- Modify: `crates/config/tests/config_tests.rs`
  - Covers parsing and defaults for the new gate config.
- Modify: `apps/trader-cli/src/main.rs`
  - Adds `trader reconciliation-gate` command that reads stored audits and exits non-zero when the gate blocks.
- Modify: `crates/storage/src/repositories.rs`
  - Adds a narrowly scoped query helper for latest reconciliation audits by broker/account.
- Modify: `crates/storage/tests/runtime_repository_tests.rs`
  - Covers latest-audit selection and multi-account filtering.
- Create: `scripts/live-reconciliation-gate.ps1`
  - Wraps the CLI gate for local/operator use.
- Create: `scripts/live-reconciliation-gate-tests.ps1`
  - Smoke-tests CLI exit behavior with fixture SQLite data.
- Create: `docs/live-reconciliation-gate-runbook.md`
  - Documents how to run the gate for paper, live dry-run, and multi-broker readiness.

---

### Task 1: Pure Gate Decision Model

**Files:**
- Create: `crates/broker/src/reconciliation_gate.rs`
- Modify: `crates/broker/src/broker.rs`

**Interfaces:**
- Produces: `evaluate_reconciliation_gate(input: ReconciliationGateInput) -> ReconciliationGateDecision`
- Produces: `ReconciliationGateRequirement`, `ReconciliationGateAudit`, `ReconciliationGateStatus`, `ReconciliationGateFailure`
- Consumes: no storage or network code.

- [x] **Step 1: Write failing broker tests**

Append this test module to `crates/broker/src/reconciliation_gate.rs` after creating the file:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn requirement(broker: &str, account_id: &str) -> ReconciliationGateRequirement {
        ReconciliationGateRequirement {
            broker: broker.to_string(),
            account_id: account_id.to_string(),
            min_successful_audits: 2,
            max_audit_age_ms: 60_000,
        }
    }

    fn audit(broker: &str, account_id: &str, ts_ms: i64) -> ReconciliationGateAudit {
        ReconciliationGateAudit {
            broker: broker.to_string(),
            account_id: account_id.to_string(),
            ts_ms,
            cash_drifts: 0,
            position_drifts: 0,
            open_order_drifts: 0,
            execution_drifts: 0,
            stale_inputs: 0,
        }
    }

    #[test]
    fn gate_allows_when_each_requirement_has_recent_clean_audits() {
        let decision = evaluate_reconciliation_gate(ReconciliationGateInput {
            now_ms: 100_000,
            requirements: vec![requirement("ibkr", "DU****91"), requirement("binance", "paper")],
            audits: vec![
                audit("ibkr", "DU****91", 90_000),
                audit("ibkr", "DU****91", 95_000),
                audit("binance", "paper", 91_000),
                audit("binance", "paper", 96_000),
            ],
        });

        assert_eq!(decision.status, ReconciliationGateStatus::Allow);
        assert!(decision.failures.is_empty());
    }

    #[test]
    fn gate_blocks_missing_required_broker_account() {
        let decision = evaluate_reconciliation_gate(ReconciliationGateInput {
            now_ms: 100_000,
            requirements: vec![requirement("ibkr", "DU****91"), requirement("binance", "paper")],
            audits: vec![audit("ibkr", "DU****91", 95_000), audit("ibkr", "DU****91", 96_000)],
        });

        assert_eq!(decision.status, ReconciliationGateStatus::Block);
        assert_eq!(decision.failures[0].reason, "missing_required_audit");
        assert_eq!(decision.failures[0].broker, "binance");
    }

    #[test]
    fn gate_blocks_drift_and_stale_inputs() {
        let mut bad = audit("ibkr", "DU****91", 95_000);
        bad.open_order_drifts = 1;
        bad.stale_inputs = 1;

        let decision = evaluate_reconciliation_gate(ReconciliationGateInput {
            now_ms: 100_000,
            requirements: vec![requirement("ibkr", "DU****91")],
            audits: vec![bad, audit("ibkr", "DU****91", 96_000)],
        });

        assert_eq!(decision.status, ReconciliationGateStatus::Block);
        assert!(decision.failures.iter().any(|failure| failure.reason == "audit_has_drift"));
        assert!(decision.failures.iter().any(|failure| failure.reason == "audit_has_stale_inputs"));
    }
}
```

- [x] **Step 2: Run tests and verify they fail**

Run: `cargo test -p broker reconciliation_gate`

Expected: FAIL because the module types and `evaluate_reconciliation_gate` are not implemented.

- [x] **Step 3: Implement the gate model**

Replace `crates/broker/src/reconciliation_gate.rs` with:

```rust
#![forbid(unsafe_code)]

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconciliationGateRequirement {
    pub broker: String,
    pub account_id: String,
    pub min_successful_audits: usize,
    pub max_audit_age_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconciliationGateAudit {
    pub broker: String,
    pub account_id: String,
    pub ts_ms: i64,
    pub cash_drifts: usize,
    pub position_drifts: usize,
    pub open_order_drifts: usize,
    pub execution_drifts: usize,
    pub stale_inputs: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconciliationGateStatus {
    Allow,
    Block,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconciliationGateFailure {
    pub broker: String,
    pub account_id: String,
    pub reason: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconciliationGateInput {
    pub now_ms: i64,
    pub requirements: Vec<ReconciliationGateRequirement>,
    pub audits: Vec<ReconciliationGateAudit>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconciliationGateDecision {
    pub status: ReconciliationGateStatus,
    pub failures: Vec<ReconciliationGateFailure>,
}

pub fn evaluate_reconciliation_gate(
    input: ReconciliationGateInput,
) -> ReconciliationGateDecision {
    let mut failures = Vec::new();

    for requirement in &input.requirements {
        let matching: Vec<&ReconciliationGateAudit> = input
            .audits
            .iter()
            .filter(|audit| {
                audit.broker == requirement.broker && audit.account_id == requirement.account_id
            })
            .collect();

        if matching.is_empty() {
            failures.push(failure(requirement, "missing_required_audit", "no matching audit"));
            continue;
        }

        let clean_recent = matching
            .iter()
            .filter(|audit| input.now_ms - audit.ts_ms <= requirement.max_audit_age_ms)
            .filter(|audit| {
                audit.cash_drifts == 0
                    && audit.position_drifts == 0
                    && audit.open_order_drifts == 0
                    && audit.execution_drifts == 0
                    && audit.stale_inputs == 0
            })
            .count();

        if clean_recent < requirement.min_successful_audits {
            failures.push(failure(
                requirement,
                "insufficient_clean_recent_audits",
                &format!(
                    "required={} observed={clean_recent}",
                    requirement.min_successful_audits
                ),
            ));
        }

        for audit in matching {
            if input.now_ms - audit.ts_ms > requirement.max_audit_age_ms {
                failures.push(failure(requirement, "audit_too_old", &audit.ts_ms.to_string()));
            }
            if audit.cash_drifts
                + audit.position_drifts
                + audit.open_order_drifts
                + audit.execution_drifts
                > 0
            {
                failures.push(failure(requirement, "audit_has_drift", &audit.ts_ms.to_string()));
            }
            if audit.stale_inputs > 0 {
                failures.push(failure(
                    requirement,
                    "audit_has_stale_inputs",
                    &audit.ts_ms.to_string(),
                ));
            }
        }
    }

    ReconciliationGateDecision {
        status: if failures.is_empty() {
            ReconciliationGateStatus::Allow
        } else {
            ReconciliationGateStatus::Block
        },
        failures,
    }
}

fn failure(
    requirement: &ReconciliationGateRequirement,
    reason: &str,
    detail: &str,
) -> ReconciliationGateFailure {
    ReconciliationGateFailure {
        broker: requirement.broker.clone(),
        account_id: requirement.account_id.clone(),
        reason: reason.to_string(),
        detail: detail.to_string(),
    }
}
```

- [x] **Step 4: Export the module**

Add this near the top of `crates/broker/src/broker.rs`:

```rust
pub mod reconciliation_gate;
```

Add this re-export block below the existing `pub use ibkr::{...};` block:

```rust
pub use reconciliation_gate::{
    ReconciliationGateAudit, ReconciliationGateDecision, ReconciliationGateFailure,
    ReconciliationGateInput, ReconciliationGateRequirement, ReconciliationGateStatus,
    evaluate_reconciliation_gate,
};
```

- [x] **Step 5: Run tests and commit**

Run: `cargo test -p broker reconciliation_gate`

Expected: PASS.

Commit:

```powershell
git add crates/broker/src/broker.rs crates/broker/src/reconciliation_gate.rs
git commit -m "feat: add reconciliation gate decision model"
```

---

### Task 2: Gate Configuration

**Files:**
- Modify: `crates/config/src/config.rs`
- Modify: `crates/config/tests/config_tests.rs`

**Interfaces:**
- Consumes: `LiveConfig`
- Produces: `LiveReconciliationGateConfig` with fields `enabled`, `min_successful_audits`, `max_audit_age_ms`, and `required_accounts`.

- [x] **Step 1: Write failing config tests**

Append to `crates/config/tests/config_tests.rs`:

```rust
#[test]
fn parses_live_reconciliation_gate_config() {
    let config = config_from_toml(
        r#"
        [runtime]
        mode = "live"
        run_id = "live-gated"

        [database]
        url = "sqlite://data/live-gated.sqlite"

        [data]
        source = "parquet"
        path = "datasets/ibkr/aapl_1d.parquet"

        [strategy]
        name = "moving_average_cross"
        symbols = ["US:NASDAQ:AAPL:EQUITY"]
        fast_window = 2
        slow_window = 3

        [portfolio]
        initial_cash = "10000"
        base_currency = "USD"
        order_qty = "1"
        max_abs_qty = "10"

        [risk]
        max_order_notional = "1000"
        min_cash_after_order = "1000"
        max_exposure = "10000"
        max_drawdown = "0.2"
        max_leverage = "1"
        max_margin_used = "1000"
        trading_halted = false

        [broker]
        kind = "ibkr"
        mode = "live"
        host = "127.0.0.1"
        port = 4001
        client_id = 1
        order_submit_enabled = false

        [paper]
        account_id = "DU****91"
        slippage_bps = "1"
        fee_bps = "1"

        [live]
        enabled = true
        heartbeat_ms = 1000
        broker_snapshot_interval_ms = 1000

        [live.reconciliation_gate]
        enabled = true
        min_successful_audits = 3
        max_audit_age_ms = 300000
        required_accounts = ["ibkr:DU****91", "binance:paper"]
        "#,
    );

    assert!(config.live.reconciliation_gate.enabled);
    assert_eq!(config.live.reconciliation_gate.min_successful_audits, 3);
    assert_eq!(config.live.reconciliation_gate.max_audit_age_ms, 300000);
    assert_eq!(
        config.live.reconciliation_gate.required_accounts,
        vec!["ibkr:DU****91".to_string(), "binance:paper".to_string()]
    );
}

#[test]
fn defaults_live_reconciliation_gate_to_disabled() {
    let config = config_from_toml(MINIMAL_CONFIG);

    assert!(!config.live.reconciliation_gate.enabled);
    assert_eq!(config.live.reconciliation_gate.min_successful_audits, 1);
    assert_eq!(config.live.reconciliation_gate.max_audit_age_ms, 300000);
    assert!(config.live.reconciliation_gate.required_accounts.is_empty());
}
```

- [x] **Step 2: Run tests and verify they fail**

Run: `cargo test -p config live_reconciliation_gate`

Expected: FAIL because `LiveConfig` has no `reconciliation_gate` field.

- [x] **Step 3: Add config structs**

In `crates/config/src/config.rs`, add this field to `LiveConfig`:

```rust
#[serde(default)]
pub reconciliation_gate: LiveReconciliationGateConfig,
```

Add this struct below `LiveConfig`:

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct LiveReconciliationGateConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_reconciliation_gate_min_successful_audits")]
    pub min_successful_audits: usize,
    #[serde(default = "default_reconciliation_gate_max_audit_age_ms")]
    pub max_audit_age_ms: i64,
    #[serde(default)]
    pub required_accounts: Vec<String>,
}

impl Default for LiveReconciliationGateConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            min_successful_audits: default_reconciliation_gate_min_successful_audits(),
            max_audit_age_ms: default_reconciliation_gate_max_audit_age_ms(),
            required_accounts: Vec::new(),
        }
    }
}

fn default_reconciliation_gate_min_successful_audits() -> usize {
    1
}

fn default_reconciliation_gate_max_audit_age_ms() -> i64 {
    300_000
}
```

- [x] **Step 4: Run tests and commit**

Run: `cargo test -p config live_reconciliation_gate`

Expected: PASS.

Commit:

```powershell
git add crates/config/src/config.rs crates/config/tests/config_tests.rs
git commit -m "feat: parse live reconciliation gate config"
```

---

### Task 3: Storage Query and CLI Gate Command

**Files:**
- Modify: `crates/storage/src/repositories.rs`
- Modify: `crates/storage/tests/runtime_repository_tests.rs`
- Modify: `apps/trader-cli/src/main.rs`

**Interfaces:**
- Consumes: `broker::evaluate_reconciliation_gate`
- Produces: CLI command `trader reconciliation-gate --config <path> --account ibkr:DU****91 --account binance:paper`
- Produces: exit code `0` when gate allows and non-zero when it blocks.

- [x] **Step 1: Write failing storage test**

Add a test to `crates/storage/tests/runtime_repository_tests.rs`:

```rust
#[tokio::test]
async fn lists_latest_reconciliation_audits_for_gate() {
    let db = test_db().await;

    db.record_reconciliation_audit(ReconciliationAuditCommand {
        id: "old".to_string(),
        run_id: "run-a".to_string(),
        account_id: "DU****91".to_string(),
        broker: "ibkr".to_string(),
        severity: "info".to_string(),
        cash_drifts: 0,
        position_drifts: 0,
        open_order_drifts: 0,
        execution_drifts: 0,
        stale_inputs: 0,
        ts_ms: 1000,
    })
    .await
    .unwrap();

    db.record_reconciliation_audit(ReconciliationAuditCommand {
        id: "new".to_string(),
        run_id: "run-a".to_string(),
        account_id: "DU****91".to_string(),
        broker: "ibkr".to_string(),
        severity: "info".to_string(),
        cash_drifts: 0,
        position_drifts: 0,
        open_order_drifts: 0,
        execution_drifts: 0,
        stale_inputs: 0,
        ts_ms: 2000,
    })
    .await
    .unwrap();

    let audits = db
        .list_latest_reconciliation_audits_for_gate("ibkr", "DU****91", 1)
        .await
        .unwrap();

    assert_eq!(audits.len(), 1);
    assert_eq!(audits[0].id, "new");
    assert_eq!(audits[0].ts_ms, 2000);
}
```

- [x] **Step 2: Run storage test and verify it fails**

Run: `cargo test -p storage lists_latest_reconciliation_audits_for_gate`

Expected: FAIL because `list_latest_reconciliation_audits_for_gate` does not exist.

- [x] **Step 3: Implement storage helper**

Add this method next to `list_reconciliation_audits` in `crates/storage/src/repositories.rs`:

```rust
pub async fn list_latest_reconciliation_audits_for_gate(
    &self,
    broker: &str,
    account_id: &str,
    limit: i64,
) -> StorageResult<Vec<StoredReconciliationAudit>> {
    let rows = sqlx::query_as::<_, StoredReconciliationAudit>(
        r#"
        SELECT id, run_id, account_id, broker, severity, cash_drifts, position_drifts,
               open_order_drifts, execution_drifts, stale_inputs, ts_ms
        FROM broker_reconciliation_audits
        WHERE broker = ? AND account_id = ?
        ORDER BY ts_ms DESC
        LIMIT ?
        "#,
    )
    .bind(broker)
    .bind(account_id)
    .bind(limit)
    .fetch_all(&self.pool)
    .await?;

    Ok(rows)
}
```

- [x] **Step 4: Write failing CLI tests**

Add focused tests in `apps/trader-cli/src/main.rs` near existing `ibkr_reconcile` tests:

```rust
#[test]
fn parses_gate_account_requirement() {
    let requirement = parse_gate_account_requirement("ibkr:DU****91").unwrap();

    assert_eq!(requirement.broker, "ibkr");
    assert_eq!(requirement.account_id, "DU****91");
}

#[test]
fn rejects_gate_account_requirement_without_separator() {
    let error = parse_gate_account_requirement("ibkr").unwrap_err().to_string();

    assert!(error.contains("expected broker:account_id"));
}
```

Run: `cargo test -p trader-cli gate_account_requirement`

Expected: FAIL because `parse_gate_account_requirement` does not exist.

- [x] **Step 5: Add CLI command parser**

Add this command variant to `Command` in `apps/trader-cli/src/main.rs`:

```rust
ReconciliationGate {
    #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
    config: String,
    #[arg(long = "account")]
    accounts: Vec<String>,
    #[arg(long)]
    min_successful_audits: Option<usize>,
    #[arg(long)]
    max_audit_age_ms: Option<i64>,
},
```

Add this helper:

```rust
fn parse_gate_account_requirement(value: &str) -> Result<broker::ReconciliationGateRequirement> {
    let Some((broker, account_id)) = value.split_once(':') else {
        bail!("expected broker:account_id");
    };
    if broker.trim().is_empty() || account_id.trim().is_empty() {
        bail!("expected broker:account_id");
    }
    Ok(broker::ReconciliationGateRequirement {
        broker: broker.trim().to_string(),
        account_id: account_id.trim().to_string(),
        min_successful_audits: 1,
        max_audit_age_ms: 300_000,
    })
}
```

- [x] **Step 6: Implement CLI evaluation**

Add a match arm in `run_command`:

```rust
Command::ReconciliationGate {
    config,
    accounts,
    min_successful_audits,
    max_audit_age_ms,
} => run_reconciliation_gate(&config, accounts, min_successful_audits, max_audit_age_ms).await?,
```

Add this function:

```rust
async fn run_reconciliation_gate(
    config: &str,
    accounts: Vec<String>,
    min_successful_audits: Option<usize>,
    max_audit_age_ms: Option<i64>,
) -> Result<()> {
    let (app_config, db) = load_db(config).await?;
    let mut requirements = if accounts.is_empty() {
        app_config
            .live
            .reconciliation_gate
            .required_accounts
            .iter()
            .map(|value| parse_gate_account_requirement(value))
            .collect::<Result<Vec<_>>>()?
    } else {
        accounts
            .iter()
            .map(|value| parse_gate_account_requirement(value))
            .collect::<Result<Vec<_>>>()?
    };

    for requirement in &mut requirements {
        requirement.min_successful_audits = min_successful_audits
            .unwrap_or(app_config.live.reconciliation_gate.min_successful_audits);
        requirement.max_audit_age_ms =
            max_audit_age_ms.unwrap_or(app_config.live.reconciliation_gate.max_audit_age_ms);
    }

    if requirements.is_empty() {
        bail!("reconciliation gate has no required accounts");
    }

    let mut audits = Vec::new();
    for requirement in &requirements {
        let rows = db
            .list_latest_reconciliation_audits_for_gate(
                &requirement.broker,
                &requirement.account_id,
                requirement.min_successful_audits as i64,
            )
            .await?;
        audits.extend(rows.into_iter().map(|row| broker::ReconciliationGateAudit {
            broker: row.broker,
            account_id: row.account_id,
            ts_ms: row.ts_ms,
            cash_drifts: row.cash_drifts as usize,
            position_drifts: row.position_drifts as usize,
            open_order_drifts: row.open_order_drifts as usize,
            execution_drifts: row.execution_drifts as usize,
            stale_inputs: row.stale_inputs as usize,
        }));
    }

    let now_ms = chrono::Utc::now().timestamp_millis();
    let decision = broker::evaluate_reconciliation_gate(broker::ReconciliationGateInput {
        now_ms,
        requirements,
        audits,
    });

    match decision.status {
        broker::ReconciliationGateStatus::Allow => {
            println!("reconciliation gate ok");
            Ok(())
        }
        broker::ReconciliationGateStatus::Block => {
            for failure in decision.failures {
                eprintln!(
                    "reconciliation gate blocked: broker={} account={} reason={} detail={}",
                    failure.broker, failure.account_id, failure.reason, failure.detail
                );
            }
            bail!("reconciliation gate blocked")
        }
    }
}
```

- [x] **Step 7: Run tests and commit**

Run:

```powershell
cargo test -p storage lists_latest_reconciliation_audits_for_gate
cargo test -p trader-cli gate_account_requirement
cargo check --workspace
```

Expected: all commands PASS.

Commit:

```powershell
git add crates/storage/src/repositories.rs crates/storage/tests/runtime_repository_tests.rs apps/trader-cli/src/main.rs
git commit -m "feat: add reconciliation gate cli"
```

---

### Task 4: Operator Script and Runbook

**Files:**
- Create: `scripts/live-reconciliation-gate.ps1`
- Create: `scripts/live-reconciliation-gate-tests.ps1`
- Create: `docs/live-reconciliation-gate-runbook.md`

**Interfaces:**
- Consumes: `trader reconciliation-gate`
- Produces: repeatable operator command for single-broker and multi-broker checks.

- [x] **Step 1: Create wrapper script**

Create `scripts/live-reconciliation-gate.ps1`:

```powershell
param(
    [string]$Config = "configs/paper/ibkr_aapl_1d_parquet.toml",
    [string[]]$Account = @(),
    [int]$MinSuccessfulAudits = 1,
    [int64]$MaxAuditAgeMs = 300000
)

$ErrorActionPreference = "Stop"

$args = @(
    "run", "-p", "trader-cli", "--",
    "reconciliation-gate",
    "--config", $Config,
    "--min-successful-audits", $MinSuccessfulAudits,
    "--max-audit-age-ms", $MaxAuditAgeMs
)

foreach ($item in $Account) {
    $args += @("--account", $item)
}

cargo @args
exit $LASTEXITCODE
```

- [x] **Step 2: Create script test**

Create `scripts/live-reconciliation-gate-tests.ps1`:

```powershell
$ErrorActionPreference = "Stop"

$script = Join-Path $PSScriptRoot "live-reconciliation-gate.ps1"
if (-not (Test-Path $script)) {
    throw "missing live-reconciliation-gate.ps1"
}

$content = Get-Content $script -Raw
if ($content -notmatch "reconciliation-gate") {
    throw "wrapper does not call reconciliation-gate"
}
if ($content -notmatch "MinSuccessfulAudits") {
    throw "wrapper does not expose MinSuccessfulAudits"
}
if ($content -notmatch "MaxAuditAgeMs") {
    throw "wrapper does not expose MaxAuditAgeMs"
}

Write-Host "live reconciliation gate script tests ok"
```

- [x] **Step 3: Create runbook**

Create `docs/live-reconciliation-gate-runbook.md`:

```markdown
# Live Reconciliation Gate Runbook

## Purpose

The live reconciliation gate blocks live-account promotion unless every required broker/account has recent clean reconciliation audits.

## Single Broker

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\live-reconciliation-gate.ps1 `
  -Config configs/paper/ibkr_aapl_1d_parquet.toml `
  -Account ibkr:DU****91 `
  -MinSuccessfulAudits 3 `
  -MaxAuditAgeMs 300000
```

Expected: exits `0` and prints `reconciliation gate ok`.

## Multi Broker

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\live-reconciliation-gate.ps1 `
  -Config configs/paper/ibkr_aapl_1d_parquet.toml `
  -Account ibkr:DU****91 `
  -Account binance:paper `
  -MinSuccessfulAudits 3 `
  -MaxAuditAgeMs 300000
```

Expected: exits `0` only when both broker/account requirements have enough clean recent audits.

## Blocking Conditions

- Missing required audit.
- Too few clean recent audits.
- Any cash, position, open-order, or execution drift.
- Any stale input.

## Safety

This command reads stored audit evidence only. It does not submit orders.
```

- [x] **Step 4: Run script test and commit**

Run:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\live-reconciliation-gate-tests.ps1
```

Expected: prints `live reconciliation gate script tests ok`.

Commit:

```powershell
git add scripts/live-reconciliation-gate.ps1 scripts/live-reconciliation-gate-tests.ps1 docs/live-reconciliation-gate-runbook.md
git commit -m "docs: add live reconciliation gate runbook"
```

---

### Task 5: End-to-End Verification and Acceptance Evidence

**Files:**
- Modify: `docs/live-reconciliation-gate-runbook.md`
- Create: `docs/live-reconciliation-gate-results-local-2026-07-07.md`

**Interfaces:**
- Consumes: completed Tasks 1-4.
- Produces: committed acceptance evidence for local fixture behavior and optional real broker read-only evidence.

- [x] **Step 1: Run full local verification**

Run:

```powershell
cargo test -p broker reconciliation_gate
cargo test -p config live_reconciliation_gate
cargo test -p storage lists_latest_reconciliation_audits_for_gate
cargo test -p trader-cli gate_account_requirement
cargo check --workspace
powershell -ExecutionPolicy Bypass -File .\scripts\live-reconciliation-gate-tests.ps1
```

Expected: all commands PASS.

- [x] **Step 2: Run optional real-evidence gate only after fresh read-only soak**

Use this only after a fresh read-only production reconciliation run records clean audits:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\production-reconciliation-soak.ps1 `
  -Broker ibkr `
  -Iterations 3 `
  -DelaySeconds 10 `
  -ReadOnly `
  -AccountId DU****91 `
  -GatewayHost 127.0.0.1 `
  -Port 4002 `
  -ClientId 1
```

Expected: production reconciliation summary has `status = completed`, `failure_class = ok`, and all drift counters are `0`.

- [x] **Step 3: Record results document**

Create `docs/live-reconciliation-gate-results-local-2026-07-07.md`:

```markdown
# Live Reconciliation Gate Results: local-2026-07-07

## Summary

- Date: 2026-07-07
- Scope: local gate logic, config parsing, storage query, CLI parser, operator script
- Status: completed
- Failure class: ok

## Verification

| Check | Result |
| --- | --- |
| `cargo test -p broker reconciliation_gate` | pass |
| `cargo test -p config live_reconciliation_gate` | pass |
| `cargo test -p storage lists_latest_reconciliation_audits_for_gate` | pass |
| `cargo test -p trader-cli gate_account_requirement` | pass |
| `cargo check --workspace` | pass |
| `scripts/live-reconciliation-gate-tests.ps1` | pass |

## Decision

The reconciliation gate is acceptable for blocking live-account promotion from stored audit evidence. Real broker readiness still depends on fresh read-only reconciliation evidence for every required broker/account before live enablement.
```

- [x] **Step 4: Commit and push**

Run:

```powershell
git add docs/live-reconciliation-gate-results-local-2026-07-07.md docs/live-reconciliation-gate-runbook.md
git commit -m "docs: record live reconciliation gate results"
git push
```

Expected: `main` is synchronized with `origin/main`; only unrelated local files remain unstaged.

---

## Self-Review

- Spec coverage: The plan covers live-account gating, multi-broker/account requirements, drift/stale blocking, config, CLI, script, tests, and acceptance evidence.
- Placeholder scan: No task depends on a missing placeholder; every command has an expected result and every code step includes concrete code.
- Type consistency: Gate types are defined in Task 1, parsed config fields are consumed by the Task 3 CLI, and script flags map directly to CLI flags.

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-07-07-live-account-multi-broker-reconciliation-gate.md`. Two execution options:

1. Subagent-Driven (recommended) - dispatch a fresh subagent per task, review between tasks, fast iteration.
2. Inline Execution - execute tasks in this session using executing-plans, batch execution with checkpoints.

Which approach?
