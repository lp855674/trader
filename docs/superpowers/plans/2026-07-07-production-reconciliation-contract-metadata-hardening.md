# Production Reconciliation Contract Metadata Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Harden broker reconciliation and contract metadata so a paper-validated runtime becomes credible for pre-production audit across cash, positions, orders, executions, and multi-asset contract fields.

**Architecture:** Keep `event_store`, `system_logs`, `risk_events`, and snapshot tables as immutable audit evidence. Extend the broker boundary to report normalized multi-currency cash/account balances and richer position contract metadata, then make live runtime reconciliation compare broker state against runtime state with configured thresholds and staleness windows. Long-run scripts produce committed summary documents while raw evidence stays under `data/`.

**Tech Stack:** Rust workspace, SQLx SQLite migrations, rust_decimal, serde JSON payloads, tokio runtime, PowerShell broker soak scripts, IBKR Gateway paper adapter, existing CLI/API read models.

## Global Constraints

- Do not use floating point for money, quantity, margin, PnL, funding, or liquidation calculations; use `rust_decimal::Decimal`.
- Do not edit historical migrations; add a new migration after `migrations/0005_reference_snapshots_and_ops.sql`.
- Preserve existing paper acceptance behavior; this plan adds production hardening and audit coverage.
- Broker integration tests that require IBKR Gateway or external credentials must skip cleanly unless explicit environment variables are present.
- Raw broker evidence under `data/` remains uncommitted; committed docs must summarize run ids, windows, broker kind, failure classes, and evidence paths.
- Do not touch unrelated local changes such as `记录.md`.

---

## File Map

### Broker Boundary

- Modify: `crates/broker/src/broker.rs`
  - Add normalized multi-currency account balance structs.
  - Add contract metadata fields to `BrokerPositionSnapshot`.
  - Add order/execution reconciliation report types.
  - Keep default adapter behavior backward-compatible.
- Modify: `crates/broker/src/ibkr.rs`
  - Map IBKR contract metadata into the broker boundary.
  - Map account summary values into currency-aware balances.
  - Add fake-client unit tests for stock, crypto, future, option, and cash balance mapping.

### Storage and Migrations

- Create: `migrations/0006_production_reconciliation_contract_metadata.sql`
  - Add broker account snapshot, reconciliation audit, and position contract metadata persistence.
  - Add open interest and liquidation fields for contract runtime positions.
- Modify: `crates/storage/src/repositories.rs`
  - Add insert/list methods for broker account balances.
  - Add insert/list methods for reconciliation audit records.
  - Extend crypto position and broker position snapshot commands with new nullable metadata.
- Modify: `crates/storage/tests/runtime_repository_tests.rs`
  - Add round-trip tests for balance snapshots, metadata fields, and audit records.

### Runtime

- Modify: `crates/runtime/src/live.rs`
  - Replace single-cash drift check with multi-currency account reconciliation.
  - Reconcile positions, open orders, and executions in one audit cycle.
  - Emit structured audit records and deduplicated alerts by reason/account/symbol/order/execution.
- Modify: `crates/runtime/tests/live_runtime_tests.rs`
  - Add fake broker tests for cash drift, stale broker snapshot, missing runtime position, orphan runtime position, unmatched open order, and missing execution.

### Configuration

- Modify: `crates/config/src/config.rs`
  - Add production reconciliation thresholds and stale-window settings.
- Modify: `configs/*.toml`
  - Add commented production reconciliation examples only where existing config files already document live settings.

### Scripts and Evidence

- Create: `scripts/production-reconciliation-soak.ps1`
  - Run broker-specific soak commands over a longer window.
  - Aggregate failure classes and reconciliation stats.
  - Write raw JSON evidence under `data/production-reconciliation/<soak_id>/`.
- Modify: `scripts/ibkr-paper-soak.ps1`
  - Include new reconciliation audit counters in `summary.json`.
- Create: `docs/production-reconciliation-runbook.md`
  - Document pre-production run procedure, required environment variables, evidence retention, and failure classes.
- Create after execution: `docs/production-reconciliation-results-<soak_id>.md`
  - Summarize a real run. The implementation tasks create the template and script; the final evidence doc is produced during a broker-connected run.
- Modify: `docs/roadmap.md`
  - Move production reconciliation / contract metadata hardening into the active pre-production milestone.
- Modify: `docs/分析.md`
  - Record current capability and remaining production limits after implementation.

---

## Acceptance Gates

Every task must preserve:

- `cargo test -p broker`
- `cargo test -p storage`
- `cargo test -p runtime`
- `cargo test -p config`
- `cargo check --workspace`
- `bash ./scripts/check-db-boundary`
- `bash ./scripts/check-storage-dto-boundary`
- `bash ./scripts/check-api-read-model-boundary`

New gates:

- `cargo test -p broker ibkr_contract_metadata`
- `cargo test -p broker broker_reconciliation_report`
- `cargo test -p storage production_reconciliation`
- `cargo test -p runtime production_reconciliation`
- `powershell -ExecutionPolicy Bypass -File .\scripts\production-reconciliation-soak.ps1 -Broker ibkr -Iterations 3 -ReadOnly -AccountId <DU_ACCOUNT> -GatewayHost 127.0.0.1 -Port 7497 -ClientId 1`

Expected for credential-free CI: unit tests pass, broker-connected PowerShell soak is documented but skipped unless a real paper account and Gateway are supplied.

---

### Task 1: Extend Broker Reconciliation Model

**Files:**

- Modify: `crates/broker/src/broker.rs`

**Interfaces:**

- Consumes: existing `BrokerAccountSnapshot`, `BrokerPositionSnapshot`, `BrokerOpenOrder`, `BrokerExecution`, `Broker` trait.
- Produces:
  - `BrokerCashBalance { account_id, currency, cash, available_cash, frozen_cash, equity, buying_power, margin_used, source_ts_ms }`
  - `BrokerContractMetadata { conid, sec_type, currency, exchange, primary_exchange, multiplier, expiry, right, strike, local_symbol, trading_class }`
  - `BrokerReconciliationThresholds { cash_abs, position_qty_abs, stale_after_ms }`
  - `BrokerReconciliationAudit { account_id, broker_kind, ts_ms, cash_drifts, position_drifts, open_order_drifts, execution_drifts, stale_inputs }`

- [ ] **Step 1: Add failing broker model tests**

Add to the bottom of `crates/broker/src/broker.rs`:

```rust
#[cfg(test)]
mod production_reconciliation_tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn account_snapshot_exposes_multi_currency_balances() {
        let snapshot = BrokerAccountSnapshot {
            account_id: "DU123".to_string(),
            cash: dec!(1000),
            equity: dec!(1500),
            buying_power: dec!(2000),
            margin_used: dec!(100),
            cash_balances: vec![
                BrokerCashBalance {
                    account_id: "DU123".to_string(),
                    currency: "USD".to_string(),
                    cash: dec!(1000),
                    available_cash: dec!(900),
                    frozen_cash: dec!(100),
                    equity: Some(dec!(1500)),
                    buying_power: Some(dec!(2000)),
                    margin_used: Some(dec!(100)),
                    source_ts_ms: 1_700_000_000_000,
                },
                BrokerCashBalance {
                    account_id: "DU123".to_string(),
                    currency: "HKD".to_string(),
                    cash: dec!(7800),
                    available_cash: dec!(7800),
                    frozen_cash: dec!(0),
                    equity: None,
                    buying_power: None,
                    margin_used: None,
                    source_ts_ms: 1_700_000_000_000,
                },
            ],
        };

        assert_eq!(snapshot.cash_balances.len(), 2);
        assert_eq!(snapshot.cash_balances[0].currency, "USD");
        assert_eq!(snapshot.cash_balances[1].cash, dec!(7800));
    }

    #[test]
    fn reconciliation_report_detects_cash_position_order_and_execution_drift() {
        let audit = reconcile_broker_audit(
            BrokerReconciliationInput {
                account_id: "DU123".to_string(),
                broker_kind: BrokerKind::InteractiveBrokers,
                ts_ms: 1_700_000_000_000,
                thresholds: BrokerReconciliationThresholds {
                    cash_abs: dec!(1),
                    position_qty_abs: dec!(0),
                    stale_after_ms: 60_000,
                },
                runtime_cash: vec![RuntimeCashBalance {
                    account_id: "DU123".to_string(),
                    currency: "USD".to_string(),
                    cash: dec!(1000),
                    ts_ms: 1_700_000_000_000,
                }],
                broker_cash: vec![BrokerCashBalance {
                    account_id: "DU123".to_string(),
                    currency: "USD".to_string(),
                    cash: dec!(998),
                    available_cash: dec!(998),
                    frozen_cash: dec!(0),
                    equity: None,
                    buying_power: None,
                    margin_used: None,
                    source_ts_ms: 1_700_000_000_000,
                }],
                runtime_positions: vec![RuntimePositionSnapshot {
                    account_id: "DU123".to_string(),
                    exchange: "IBKR".to_string(),
                    symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
                    position_side: BrokerPositionSide::Long,
                    qty: dec!(2),
                    avg_price: dec!(180),
                    margin_used: dec!(0),
                }],
                broker_positions: vec![BrokerPositionSnapshot {
                    account_id: "DU123".to_string(),
                    exchange: "IBKR".to_string(),
                    symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
                    position_side: BrokerPositionSide::Long,
                    qty: dec!(1),
                    avg_price: dec!(180),
                    margin_used: dec!(0),
                    unrealized_pnl: dec!(0),
                    ts_ms: 1_700_000_000_000,
                    contract: Some(BrokerContractMetadata::default()),
                    liquidation_price: None,
                    open_interest: None,
                }],
                runtime_open_order_ids: vec!["local-order-1".to_string()],
                broker_open_orders: vec![BrokerOpenOrder {
                    broker_order_id: "remote-order-1".to_string(),
                    client_order_id: "missing-client".to_string(),
                    account_id: "DU123".to_string(),
                    symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
                    side: trader_core::OrderSide::Buy,
                    order_type: trader_core::OrderType::Limit,
                    price: Some(dec!(170)),
                    qty: dec!(1),
                    filled_qty: dec!(0),
                    status: "Submitted".to_string(),
                }],
                runtime_execution_ids: vec![],
                broker_executions: vec![BrokerExecution {
                    trade_id: "exec-1".to_string(),
                    broker_order_id: "remote-order-1".to_string(),
                    client_order_id: None,
                    account_id: "DU123".to_string(),
                    symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
                    side: trader_core::OrderSide::Buy,
                    price: dec!(170),
                    qty: dec!(1),
                    fee: dec!(1),
                    ts_ms: 1_700_000_000_000,
                }],
            },
        );

        assert_eq!(audit.cash_drifts.len(), 1);
        assert_eq!(audit.position_drifts.len(), 1);
        assert_eq!(audit.open_order_drifts.len(), 1);
        assert_eq!(audit.execution_drifts.len(), 1);
        assert_eq!(audit.severity, BrokerReconciliationSeverity::Error);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p broker production_reconciliation_tests`

Expected: compile fails with missing `BrokerCashBalance`, `BrokerContractMetadata`, `BrokerReconciliationInput`, `RuntimeCashBalance`, and `reconcile_broker_audit`.

- [ ] **Step 3: Add minimal broker structs**

Add near `BrokerAccountSnapshot` in `crates/broker/src/broker.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BrokerCashBalance {
    pub account_id: String,
    pub currency: String,
    pub cash: Decimal,
    pub available_cash: Decimal,
    pub frozen_cash: Decimal,
    pub equity: Option<Decimal>,
    pub buying_power: Option<Decimal>,
    pub margin_used: Option<Decimal>,
    pub source_ts_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeCashBalance {
    pub account_id: String,
    pub currency: String,
    pub cash: Decimal,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct BrokerContractMetadata {
    pub conid: Option<i64>,
    pub sec_type: Option<String>,
    pub currency: Option<String>,
    pub exchange: Option<String>,
    pub primary_exchange: Option<String>,
    pub multiplier: Option<Decimal>,
    pub expiry: Option<String>,
    pub right: Option<String>,
    pub strike: Option<Decimal>,
    pub local_symbol: Option<String>,
    pub trading_class: Option<String>,
}
```

Extend `BrokerAccountSnapshot`:

```rust
pub struct BrokerAccountSnapshot {
    pub account_id: String,
    pub cash: Decimal,
    pub equity: Decimal,
    pub buying_power: Decimal,
    pub margin_used: Decimal,
    pub cash_balances: Vec<BrokerCashBalance>,
}
```

Extend `BrokerPositionSnapshot`:

```rust
pub struct BrokerPositionSnapshot {
    pub account_id: String,
    pub exchange: String,
    pub symbol: String,
    pub position_side: BrokerPositionSide,
    pub qty: Decimal,
    pub avg_price: Decimal,
    pub margin_used: Decimal,
    pub unrealized_pnl: Decimal,
    pub ts_ms: i64,
    pub contract: Option<BrokerContractMetadata>,
    pub liquidation_price: Option<Decimal>,
    pub open_interest: Option<Decimal>,
}
```

Update existing fake snapshot constructors so `cash_balances` has one base-currency row and fake positions set `contract: None`, `liquidation_price: None`, `open_interest: None`.

- [ ] **Step 4: Add audit structs and reconciliation implementation**

Add near `PositionReconciliationReport`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BrokerReconciliationSeverity {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BrokerReconciliationThresholds {
    pub cash_abs: Decimal,
    pub position_qty_abs: Decimal,
    pub stale_after_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BrokerReconciliationDrift {
    pub account_id: String,
    pub reason: String,
    pub symbol: Option<String>,
    pub currency: Option<String>,
    pub local_value: Option<String>,
    pub broker_value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BrokerReconciliationAudit {
    pub account_id: String,
    pub broker_kind: BrokerKind,
    pub ts_ms: i64,
    pub severity: BrokerReconciliationSeverity,
    pub cash_drifts: Vec<BrokerReconciliationDrift>,
    pub position_drifts: Vec<BrokerReconciliationDrift>,
    pub open_order_drifts: Vec<BrokerReconciliationDrift>,
    pub execution_drifts: Vec<BrokerReconciliationDrift>,
    pub stale_inputs: Vec<BrokerReconciliationDrift>,
}

pub struct BrokerReconciliationInput {
    pub account_id: String,
    pub broker_kind: BrokerKind,
    pub ts_ms: i64,
    pub thresholds: BrokerReconciliationThresholds,
    pub runtime_cash: Vec<RuntimeCashBalance>,
    pub broker_cash: Vec<BrokerCashBalance>,
    pub runtime_positions: Vec<RuntimePositionSnapshot>,
    pub broker_positions: Vec<BrokerPositionSnapshot>,
    pub runtime_open_order_ids: Vec<String>,
    pub broker_open_orders: Vec<BrokerOpenOrder>,
    pub runtime_execution_ids: Vec<String>,
    pub broker_executions: Vec<BrokerExecution>,
}

pub fn reconcile_broker_audit(input: BrokerReconciliationInput) -> BrokerReconciliationAudit {
    let mut audit = BrokerReconciliationAudit {
        account_id: input.account_id.clone(),
        broker_kind: input.broker_kind,
        ts_ms: input.ts_ms,
        severity: BrokerReconciliationSeverity::Info,
        cash_drifts: Vec::new(),
        position_drifts: Vec::new(),
        open_order_drifts: Vec::new(),
        execution_drifts: Vec::new(),
        stale_inputs: Vec::new(),
    };

    for broker_cash in &input.broker_cash {
        if input.ts_ms - broker_cash.source_ts_ms > input.thresholds.stale_after_ms {
            audit.stale_inputs.push(BrokerReconciliationDrift {
                account_id: broker_cash.account_id.clone(),
                reason: "broker_cash_stale".to_string(),
                symbol: None,
                currency: Some(broker_cash.currency.clone()),
                local_value: None,
                broker_value: Some(broker_cash.source_ts_ms.to_string()),
            });
        }
        match input.runtime_cash.iter().find(|runtime_cash| {
            runtime_cash.account_id == broker_cash.account_id
                && runtime_cash.currency == broker_cash.currency
        }) {
            Some(runtime_cash) => {
                let drift = (runtime_cash.cash - broker_cash.cash).abs();
                if drift > input.thresholds.cash_abs {
                    audit.cash_drifts.push(BrokerReconciliationDrift {
                        account_id: broker_cash.account_id.clone(),
                        reason: "cash_total_drift".to_string(),
                        symbol: None,
                        currency: Some(broker_cash.currency.clone()),
                        local_value: Some(runtime_cash.cash.to_string()),
                        broker_value: Some(broker_cash.cash.to_string()),
                    });
                }
            }
            None => audit.cash_drifts.push(BrokerReconciliationDrift {
                account_id: broker_cash.account_id.clone(),
                reason: "cash_missing_runtime".to_string(),
                symbol: None,
                currency: Some(broker_cash.currency.clone()),
                local_value: None,
                broker_value: Some(broker_cash.cash.to_string()),
            }),
        }
    }

    for position in &input.broker_positions {
        match input.runtime_positions.iter().find(|runtime_position| {
            runtime_position.account_id == position.account_id
                && runtime_position.exchange == position.exchange
                && runtime_position.symbol == position.symbol
                && runtime_position.position_side == position.position_side
        }) {
            Some(runtime_position) => {
                let drift = (runtime_position.qty - position.qty).abs();
                if drift > input.thresholds.position_qty_abs {
                    audit.position_drifts.push(BrokerReconciliationDrift {
                        account_id: position.account_id.clone(),
                        reason: "position_qty_drift".to_string(),
                        symbol: Some(position.symbol.clone()),
                        currency: position.contract.as_ref().and_then(|contract| contract.currency.clone()),
                        local_value: Some(runtime_position.qty.to_string()),
                        broker_value: Some(position.qty.to_string()),
                    });
                }
            }
            None => audit.position_drifts.push(BrokerReconciliationDrift {
                account_id: position.account_id.clone(),
                reason: "position_missing_runtime".to_string(),
                symbol: Some(position.symbol.clone()),
                currency: position.contract.as_ref().and_then(|contract| contract.currency.clone()),
                local_value: None,
                broker_value: Some(position.qty.to_string()),
            }),
        }
    }

    for runtime_position in &input.runtime_positions {
        if !input.broker_positions.iter().any(|position| {
            position.account_id == runtime_position.account_id
                && position.exchange == runtime_position.exchange
                && position.symbol == runtime_position.symbol
                && position.position_side == runtime_position.position_side
        }) {
            audit.position_drifts.push(BrokerReconciliationDrift {
                account_id: runtime_position.account_id.clone(),
                reason: "position_missing_broker".to_string(),
                symbol: Some(runtime_position.symbol.clone()),
                currency: None,
                local_value: Some(runtime_position.qty.to_string()),
                broker_value: None,
            });
        }
    }

    for order in &input.broker_open_orders {
        if !input.runtime_open_order_ids.iter().any(|id| id == &order.client_order_id || id == &order.broker_order_id) {
            audit.open_order_drifts.push(BrokerReconciliationDrift {
                account_id: order.account_id.clone(),
                reason: "open_order_missing_runtime".to_string(),
                symbol: Some(order.symbol.clone()),
                currency: None,
                local_value: None,
                broker_value: Some(order.broker_order_id.clone()),
            });
        }
    }

    for execution in &input.broker_executions {
        if !input.runtime_execution_ids.iter().any(|id| id == &execution.trade_id) {
            audit.execution_drifts.push(BrokerReconciliationDrift {
                account_id: execution.account_id.clone(),
                reason: "execution_missing_runtime".to_string(),
                symbol: Some(execution.symbol.clone()),
                currency: None,
                local_value: None,
                broker_value: Some(execution.trade_id.clone()),
            });
        }
    }

    if !audit.cash_drifts.is_empty()
        || !audit.position_drifts.is_empty()
        || !audit.open_order_drifts.is_empty()
        || !audit.execution_drifts.is_empty()
    {
        audit.severity = BrokerReconciliationSeverity::Error;
    } else if !audit.stale_inputs.is_empty() {
        audit.severity = BrokerReconciliationSeverity::Warn;
    }
    audit
}
```

- [ ] **Step 5: Run broker tests**

Run: `cargo test -p broker production_reconciliation_tests`

Expected: pass.

- [ ] **Step 6: Commit**

```powershell
git add crates/broker/src/broker.rs
git commit -m "feat: extend broker reconciliation model"
```

---

### Task 2: Persist Production Reconciliation Evidence

**Files:**

- Create: `migrations/0006_production_reconciliation_contract_metadata.sql`
- Modify: `crates/storage/src/repositories.rs`
- Modify: `crates/storage/tests/runtime_repository_tests.rs`

**Interfaces:**

- Consumes: Task 1 broker audit field names.
- Produces:
  - `record_broker_account_balances(command: BrokerAccountBalancesCommand)`
  - `record_reconciliation_audit(command: ReconciliationAuditCommand)`
  - query methods used by runtime tests and soak summaries.

- [ ] **Step 1: Add failing storage tests**

Add to `crates/storage/tests/runtime_repository_tests.rs`:

```rust
#[tokio::test]
async fn production_reconciliation_account_balances_round_trip() {
    let db = test_db().await;
    seed_strategy_run(&db, "prod-recon-balances").await;

    db.record_broker_account_balances(storage::BrokerAccountBalancesCommand {
        run_id: "prod-recon-balances".to_string(),
        account_id: "DU123".to_string(),
        broker: "ibkr".to_string(),
        ts_ms: 1_700_000_000_000,
        balances: vec![storage::BrokerAccountBalanceCommand {
            currency: "USD".to_string(),
            cash: rust_decimal_macros::dec!(1000),
            available_cash: rust_decimal_macros::dec!(900),
            frozen_cash: rust_decimal_macros::dec!(100),
            equity: Some(rust_decimal_macros::dec!(1500)),
            buying_power: Some(rust_decimal_macros::dec!(2000)),
            margin_used: Some(rust_decimal_macros::dec!(100)),
            source_ts_ms: 1_700_000_000_000,
        }],
    }).await.unwrap();

    let balances = db
        .list_broker_account_balances("prod-recon-balances", Some("DU123"))
        .await
        .unwrap();
    assert_eq!(balances.len(), 1);
    assert_eq!(balances[0].currency, "USD");
    assert_eq!(balances[0].cash, "1000");
    assert_eq!(balances[0].available_cash, "900");
}

#[tokio::test]
async fn production_reconciliation_audit_round_trip() {
    let db = test_db().await;
    seed_strategy_run(&db, "prod-recon-audit").await;

    db.record_reconciliation_audit(storage::ReconciliationAuditCommand {
        id: "audit-1".to_string(),
        run_id: "prod-recon-audit".to_string(),
        account_id: "DU123".to_string(),
        broker: "ibkr".to_string(),
        ts_ms: 1_700_000_000_001,
        severity: "error".to_string(),
        cash_drift_count: 1,
        position_drift_count: 1,
        open_order_drift_count: 1,
        execution_drift_count: 1,
        stale_input_count: 0,
        payload_json: serde_json::json!({"reason":"cash_total_drift"}),
    }).await.unwrap();

    let audits = db
        .list_reconciliation_audits("prod-recon-audit")
        .await
        .unwrap();
    assert_eq!(audits.len(), 1);
    assert_eq!(audits[0].severity, "error");
    assert_eq!(audits[0].cash_drift_count, 1);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p storage production_reconciliation`

Expected: compile fails with missing storage commands and methods.

- [ ] **Step 3: Add migration**

Create `migrations/0006_production_reconciliation_contract_metadata.sql`:

```sql
CREATE TABLE IF NOT EXISTS broker_account_balances (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL,
    account_id TEXT NOT NULL,
    broker TEXT NOT NULL,
    ts_ms INTEGER NOT NULL,
    currency TEXT NOT NULL,
    cash TEXT NOT NULL,
    available_cash TEXT NOT NULL,
    frozen_cash TEXT NOT NULL,
    equity TEXT,
    buying_power TEXT,
    margin_used TEXT,
    source_ts_ms INTEGER NOT NULL,
    created_at_ms INTEGER NOT NULL,
    FOREIGN KEY(run_id) REFERENCES strategy_runs(id)
);

CREATE INDEX IF NOT EXISTS idx_broker_account_balances_run_account_ts
ON broker_account_balances(run_id, account_id, ts_ms);

CREATE TABLE IF NOT EXISTS reconciliation_audits (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL,
    account_id TEXT NOT NULL,
    broker TEXT NOT NULL,
    ts_ms INTEGER NOT NULL,
    severity TEXT NOT NULL,
    cash_drift_count INTEGER NOT NULL,
    position_drift_count INTEGER NOT NULL,
    open_order_drift_count INTEGER NOT NULL,
    execution_drift_count INTEGER NOT NULL,
    stale_input_count INTEGER NOT NULL,
    payload_json TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    FOREIGN KEY(run_id) REFERENCES strategy_runs(id)
);

CREATE INDEX IF NOT EXISTS idx_reconciliation_audits_run_ts
ON reconciliation_audits(run_id, ts_ms);

ALTER TABLE crypto_positions ADD COLUMN liquidation_price TEXT;
ALTER TABLE crypto_positions ADD COLUMN liquidation_buffer_bps TEXT;
ALTER TABLE crypto_positions ADD COLUMN open_interest TEXT;
ALTER TABLE crypto_positions ADD COLUMN contract_meta_id TEXT;

ALTER TABLE position_snapshots ADD COLUMN contract_meta_json TEXT;
ALTER TABLE position_snapshots ADD COLUMN liquidation_price TEXT;
ALTER TABLE position_snapshots ADD COLUMN open_interest TEXT;
```

- [ ] **Step 4: Add storage command structs and methods**

Add command/stored structs near existing snapshot command structs in `crates/storage/src/repositories.rs`:

```rust
pub struct BrokerAccountBalanceCommand {
    pub currency: String,
    pub cash: Decimal,
    pub available_cash: Decimal,
    pub frozen_cash: Decimal,
    pub equity: Option<Decimal>,
    pub buying_power: Option<Decimal>,
    pub margin_used: Option<Decimal>,
    pub source_ts_ms: i64,
}

pub struct BrokerAccountBalancesCommand {
    pub run_id: String,
    pub account_id: String,
    pub broker: String,
    pub ts_ms: i64,
    pub balances: Vec<BrokerAccountBalanceCommand>,
}

pub struct StoredBrokerAccountBalance {
    pub id: String,
    pub run_id: String,
    pub account_id: String,
    pub broker: String,
    pub ts_ms: i64,
    pub currency: String,
    pub cash: String,
    pub available_cash: String,
    pub frozen_cash: String,
    pub equity: Option<String>,
    pub buying_power: Option<String>,
    pub margin_used: Option<String>,
    pub source_ts_ms: i64,
}

pub struct ReconciliationAuditCommand {
    pub id: String,
    pub run_id: String,
    pub account_id: String,
    pub broker: String,
    pub ts_ms: i64,
    pub severity: String,
    pub cash_drift_count: i64,
    pub position_drift_count: i64,
    pub open_order_drift_count: i64,
    pub execution_drift_count: i64,
    pub stale_input_count: i64,
    pub payload_json: serde_json::Value,
}

pub struct StoredReconciliationAudit {
    pub id: String,
    pub run_id: String,
    pub account_id: String,
    pub broker: String,
    pub ts_ms: i64,
    pub severity: String,
    pub cash_drift_count: i64,
    pub position_drift_count: i64,
    pub open_order_drift_count: i64,
    pub execution_drift_count: i64,
    pub stale_input_count: i64,
    pub payload_json: String,
}
```

Implement methods on `Db` following existing repository style:

```rust
pub async fn record_broker_account_balances(
    &self,
    command: BrokerAccountBalancesCommand,
) -> StorageResult<()> {
    let now = chrono::Utc::now().timestamp_millis();
    for balance in command.balances {
        sqlx::query(
            r#"
            INSERT INTO broker_account_balances (
                id, run_id, account_id, broker, ts_ms, currency, cash,
                available_cash, frozen_cash, equity, buying_power, margin_used,
                source_ts_ms, created_at_ms
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&command.run_id)
        .bind(&command.account_id)
        .bind(&command.broker)
        .bind(command.ts_ms)
        .bind(balance.currency)
        .bind(balance.cash.to_string())
        .bind(balance.available_cash.to_string())
        .bind(balance.frozen_cash.to_string())
        .bind(balance.equity.map(|value| value.to_string()))
        .bind(balance.buying_power.map(|value| value.to_string()))
        .bind(balance.margin_used.map(|value| value.to_string()))
        .bind(balance.source_ts_ms)
        .bind(now)
        .execute(self.pool())
        .await?;
    }
    Ok(())
}
```

Use `sqlx::query_as` for `list_broker_account_balances` and `list_reconciliation_audits`, matching the repository's existing row mapping pattern.

- [ ] **Step 5: Run storage tests**

Run: `cargo test -p storage production_reconciliation`

Expected: pass.

- [ ] **Step 6: Run boundary checks**

Run:

```powershell
bash ./scripts/check-db-boundary
bash ./scripts/check-storage-dto-boundary
```

Expected: both pass.

- [ ] **Step 7: Commit**

```powershell
git add migrations/0006_production_reconciliation_contract_metadata.sql crates/storage/src/repositories.rs crates/storage/tests/runtime_repository_tests.rs
git commit -m "feat: persist production reconciliation audit evidence"
```

---

### Task 3: Harden Live Runtime Reconciliation

**Files:**

- Modify: `crates/runtime/src/live.rs`
- Modify: `crates/runtime/tests/live_runtime_tests.rs`
- Modify: `crates/config/src/config.rs`

**Interfaces:**

- Consumes: Task 1 `reconcile_broker_audit`; Task 2 storage methods.
- Produces: one persisted `reconciliation_audits` row per broker snapshot cycle, plus risk events and alert logs for non-info audits.

- [ ] **Step 1: Add failing runtime tests**

Add tests to `crates/runtime/tests/live_runtime_tests.rs`:

```rust
#[tokio::test]
async fn production_reconciliation_records_audit_for_cash_position_order_and_execution_drift() {
    let (db, run_id) = live_test_db("prod-recon-runtime").await;
    seed_runtime_cash_snapshot(&db, &run_id, "USD", rust_decimal_macros::dec!(1000)).await;
    seed_runtime_position_snapshot(
        &db,
        &run_id,
        "IBKR",
        "US:NASDAQ:AAPL:EQUITY",
        "long",
        rust_decimal_macros::dec!(2),
    ).await;

    let broker = std::sync::Arc::new(ProductionReconciliationFakeBroker::with_drift());
    let runtime = LiveRuntime::new_with_broker(
        db.clone(),
        LiveRuntimeSettings {
            run_id: run_id.clone(),
            broker_kind: broker::BrokerKind::InteractiveBrokers,
            account_id: "DU123".to_string(),
            base_currency: "USD".to_string(),
            initial_cash: rust_decimal_macros::dec!(1000),
            broker_snapshot_interval_ms: Some(1),
            alert_sink: runtime::AlertSinkSettings::Noop,
            logging: events::LogWriterSettings::default(),
            reconciliation: runtime::ProductionReconciliationSettings {
                cash_abs_threshold: rust_decimal_macros::dec!(1),
                position_qty_abs_threshold: rust_decimal_macros::dec!(0),
                stale_after_ms: 60_000,
            },
        },
        broker,
    );

    runtime.record_broker_snapshot_for_test().await.unwrap();

    let audits = db.list_reconciliation_audits(&run_id).await.unwrap();
    assert_eq!(audits.len(), 1);
    assert_eq!(audits[0].severity, "error");
    assert_eq!(audits[0].cash_drift_count, 1);
    assert_eq!(audits[0].position_drift_count, 1);
    assert_eq!(audits[0].open_order_drift_count, 1);
    assert_eq!(audits[0].execution_drift_count, 1);

    let alerts = db.list_system_logs(Some(&run_id), Some("runtime.alert"), None).await.unwrap();
    assert!(alerts.iter().any(|log| log.message == "reconciliation_drift.alert"));
}
```

The helper `ProductionReconciliationFakeBroker` should implement `broker::Broker` in the test module and return one USD cash drift, one AAPL position qty drift, one unmatched open order, and one unmatched execution.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p runtime production_reconciliation_records_audit`

Expected: compile fails because runtime settings and test-only snapshot method do not exist.

- [ ] **Step 3: Add reconciliation settings**

In `crates/runtime/src/live.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductionReconciliationSettings {
    pub cash_abs_threshold: Decimal,
    pub position_qty_abs_threshold: Decimal,
    pub stale_after_ms: i64,
}

impl Default for ProductionReconciliationSettings {
    fn default() -> Self {
        Self {
            cash_abs_threshold: Decimal::ZERO,
            position_qty_abs_threshold: Decimal::ZERO,
            stale_after_ms: 60_000,
        }
    }
}
```

Add to `LiveRuntimeSettings`:

```rust
pub reconciliation: ProductionReconciliationSettings,
```

Update all `LiveRuntimeSettings` construction sites to use `ProductionReconciliationSettings::default()` or configured values.

- [ ] **Step 4: Replace ad hoc drift checks with audit cycle**

Refactor `record_broker_snapshot`:

1. Fetch `account_snapshot`, `position_snapshots`, `open_orders`, and broker `executions`.
2. Persist `broker_account_balances`.
3. Persist broker position snapshots with `contract_meta_json`, `liquidation_price`, and `open_interest`.
4. Build runtime cash/position/order/execution inputs from current DB state.
5. Call `broker::reconcile_broker_audit`.
6. Persist `reconciliation_audits`.
7. Emit one `risk_events` row and one `runtime.alert` row when severity is `warn` or `error`.

Use this event payload shape:

```rust
serde_json::json!({
    "run_id": &self.settings.run_id,
    "account_id": &self.settings.account_id,
    "broker_kind": self.settings.broker_kind,
    "risk_type": "reconciliation_drift",
    "severity": severity,
    "cash_drift_count": audit.cash_drifts.len(),
    "position_drift_count": audit.position_drifts.len(),
    "open_order_drift_count": audit.open_order_drifts.len(),
    "execution_drift_count": audit.execution_drifts.len(),
    "stale_input_count": audit.stale_inputs.len(),
    "audit": audit,
})
```

Keep the old `record_cash_drift_if_needed` and `record_position_drift_if_needed` functions only if tests still use them; otherwise delete them in the same commit.

- [ ] **Step 5: Add test-only snapshot method**

Add under `impl LiveRuntime`:

```rust
#[cfg(test)]
pub async fn record_broker_snapshot_for_test(&self) -> anyhow::Result<()> {
    self.record_broker_snapshot().await
}
```

- [ ] **Step 6: Wire config parsing**

In `crates/config/src/config.rs`, add fields under the existing live config struct:

```rust
#[serde(default)]
pub struct ReconciliationConfig {
    pub cash_abs_threshold: Decimal,
    pub position_qty_abs_threshold: Decimal,
    pub stale_after_ms: i64,
}
```

Map config into `LiveRuntimeSettings.reconciliation` at the existing live runtime construction site.

- [ ] **Step 7: Run runtime and config tests**

Run:

```powershell
cargo test -p runtime production_reconciliation
cargo test -p config reconciliation
```

Expected: pass.

- [ ] **Step 8: Commit**

```powershell
git add crates/runtime/src/live.rs crates/runtime/tests/live_runtime_tests.rs crates/config/src/config.rs configs
git commit -m "feat: harden live production reconciliation audit"
```

---

### Task 4: Enrich IBKR Contract and Account Mapping

**Files:**

- Modify: `crates/broker/src/ibkr.rs`
- Modify: `crates/broker/src/broker.rs` if Task 1 tests require field visibility changes.

**Interfaces:**

- Consumes: `BrokerCashBalance`, `BrokerContractMetadata`.
- Produces: IBKR position snapshots with conid/sec_type/currency/exchange/primary_exchange/multiplier/expiry/right/strike/local_symbol/trading_class where `ibapi` exposes them.

- [ ] **Step 1: Add failing IBKR mapping tests**

Add to `crates/broker/src/ibkr.rs`:

```rust
#[cfg(test)]
mod ibkr_contract_metadata_tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn ibkr_contract_metadata_maps_stock_contract_fields() {
        let mut contract = Contract::stock("AAPL").build();
        contract.contract_id = 265598;
        contract.exchange = "SMART".into();
        contract.primary_exchange = "NASDAQ".into();
        contract.currency = "USD".into();
        contract.local_symbol = "AAPL".into();
        contract.trading_class = "NMS".into();

        let metadata = broker_contract_metadata_from_ibkr_contract(&contract).unwrap();

        assert_eq!(metadata.conid, Some(265598));
        assert_eq!(metadata.sec_type.as_deref(), Some("STK"));
        assert_eq!(metadata.currency.as_deref(), Some("USD"));
        assert_eq!(metadata.exchange.as_deref(), Some("SMART"));
        assert_eq!(metadata.primary_exchange.as_deref(), Some("NASDAQ"));
        assert_eq!(metadata.local_symbol.as_deref(), Some("AAPL"));
        assert_eq!(metadata.trading_class.as_deref(), Some("NMS"));
    }

    #[test]
    fn ibkr_position_snapshot_keeps_contract_metadata() {
        let mut contract = Contract::stock("AAPL").build();
        contract.contract_id = 265598;
        contract.exchange = "SMART".into();
        contract.primary_exchange = "NASDAQ".into();
        contract.currency = "USD".into();

        let position = ibapi::accounts::Position {
            account: "DU123".to_string(),
            contract,
            position: 2.0,
            average_cost: 180.0,
        };

        let snapshot = map_position_snapshot(position).unwrap().unwrap();
        assert_eq!(snapshot.symbol, "US:NASDAQ:AAPL:EQUITY");
        assert_eq!(snapshot.contract.unwrap().conid, Some(265598));
        assert_eq!(snapshot.qty, dec!(2));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p broker ibkr_contract_metadata`

Expected: compile fails with missing `broker_contract_metadata_from_ibkr_contract` and missing fields in `BrokerPositionSnapshot`.

- [ ] **Step 3: Implement contract metadata mapper**

Add in `crates/broker/src/ibkr.rs` near `ibkr_position_symbol`:

```rust
fn broker_contract_metadata_from_ibkr_contract(
    contract: &Contract,
) -> Result<BrokerContractMetadata, BrokerError> {
    Ok(BrokerContractMetadata {
        conid: if contract.contract_id == 0 { None } else { Some(i64::from(contract.contract_id)) },
        sec_type: Some(contract.security_type.to_string()),
        currency: non_empty_string(contract.currency.to_string()),
        exchange: non_empty_string(contract.exchange.to_string()),
        primary_exchange: non_empty_string(contract.primary_exchange.to_string()),
        multiplier: non_empty_decimal(contract.multiplier.to_string(), "IBKR contract multiplier")?,
        expiry: non_empty_string(contract.last_trade_date_or_contract_month.to_string()),
        right: non_empty_string(contract.right.to_string()),
        strike: if contract.strike == 0.0 {
            None
        } else {
            Some(decimal_from_f64(contract.strike, "IBKR option strike")?)
        },
        local_symbol: non_empty_string(contract.local_symbol.to_string()),
        trading_class: non_empty_string(contract.trading_class.to_string()),
    })
}

fn non_empty_string(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn non_empty_decimal(value: String, name: &str) -> Result<Option<Decimal>, BrokerError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    trimmed
        .parse::<Decimal>()
        .map(Some)
        .map_err(|error| BrokerError::Config(format!("invalid {name}: {error}")))
}
```

Update `map_position_snapshot` to set:

```rust
contract: Some(broker_contract_metadata_from_ibkr_contract(&position.contract)?),
liquidation_price: None,
open_interest: None,
```

- [ ] **Step 4: Add multi-currency account balances for IBKR summary**

Update `account_snapshot_from_summary` so the returned `BrokerAccountSnapshot` includes one `BrokerCashBalance` row for the summary currency. If IBKR account summary values expose per-currency tags in this adapter version, parse each currency into a separate row; otherwise produce a single row using `USD` only when the caller's config base currency is `USD` and record the limitation in `docs/分析.md`.

Use:

```rust
cash_balances: vec![BrokerCashBalance {
    account_id: account_id.to_string(),
    currency: "USD".to_string(),
    cash,
    available_cash: cash,
    frozen_cash: Decimal::ZERO,
    equity: Some(equity),
    buying_power: Some(buying_power),
    margin_used: Some(margin_used),
    source_ts_ms: Utc::now().timestamp_millis(),
}]
```

- [ ] **Step 5: Run broker tests**

Run:

```powershell
cargo test -p broker ibkr_contract_metadata
cargo test -p broker production_reconciliation_tests
```

Expected: pass.

- [ ] **Step 6: Commit**

```powershell
git add crates/broker/src/ibkr.rs crates/broker/src/broker.rs
git commit -m "feat: enrich IBKR contract metadata mapping"
```

---

### Task 5: Add Production Reconciliation Soak Script

**Files:**

- Create: `scripts/production-reconciliation-soak.ps1`
- Modify: `scripts/ibkr-paper-soak.ps1`

**Interfaces:**

- Consumes: existing `scripts/ibkr-paper-soak.ps1` summary format.
- Produces: `data/production-reconciliation/<soak_id>/summary.json` with audit counters and failure class.

- [ ] **Step 1: Create production soak script**

Create `scripts/production-reconciliation-soak.ps1`:

```powershell
param(
    [ValidateSet("ibkr")]
    [string]$Broker = "ibkr",
    [int]$Iterations = 6,
    [int]$DelaySeconds = 10,
    [switch]$ReadOnly,
    [string]$AccountId = "",
    [string]$GatewayHost = "127.0.0.1",
    [int]$Port = 7497,
    [int]$ClientId = 1
)

$ErrorActionPreference = "Stop"

if ($Iterations -lt 1) {
    throw "Iterations must be at least 1"
}
if ($Broker -eq "ibkr" -and $AccountId.Trim().Length -eq 0) {
    throw "IBKR production reconciliation soak requires -AccountId DU..."
}

$repoRoot = Get-Location
$id = [guid]::NewGuid().ToString("N")
$soakId = "production-reconciliation-$Broker-$($id.Substring(0, 12))"
$soakDir = Join-Path $repoRoot "data/production-reconciliation/$soakId"
$summaryPath = Join-Path $soakDir "summary.json"
New-Item -ItemType Directory -Force -Path $soakDir | Out-Null

$iterations = @()
$failed = $false
$failureClass = "ok"

for ($iteration = 1; $iteration -le $Iterations; $iteration++) {
    $iterationLog = Join-Path $soakDir "iteration-$iteration.log"
    $args = @(
        "-ExecutionPolicy", "Bypass",
        "-File", ".\scripts\ibkr-paper-soak.ps1",
        "-Iterations", "1",
        "-SkipRefresh",
        "-AccountId", $AccountId,
        "-GatewayHost", $GatewayHost,
        "-Port", "$Port",
        "-ClientId", "$ClientId"
    )
    if (-not $ReadOnly) {
        $args += "-ConfirmIbkrPaperOrder"
    }

    Write-Host "Production reconciliation soak $soakId iteration $iteration/$Iterations"
    $output = powershell @args 2>&1
    $exitCode = $LASTEXITCODE
    $text = $output -join [Environment]::NewLine
    $text | Set-Content -Path $iterationLog -Encoding UTF8
    $output | ForEach-Object { Write-Host $_ }

    $iterationStatus = if ($exitCode -eq 0) { "completed" } else { "failed" }
    $iterationFailureClass = if ($exitCode -eq 0) { "ok" } else { "iteration_failed" }
    if ($text -match "gateway_unreachable") { $iterationFailureClass = "gateway_unreachable" }
    if ($text -match "account_mismatch") { $iterationFailureClass = "account_mismatch" }
    if ($text -match "reconciliation_drift") { $iterationFailureClass = "reconciliation_drift" }

    $iterations += [pscustomobject]@{
        iteration = $iteration
        exit_code = $exitCode
        status = $iterationStatus
        failure_class = $iterationFailureClass
        log = $iterationLog
    }

    if ($iterationFailureClass -ne "ok") {
        $failed = $true
        $failureClass = $iterationFailureClass
        break
    }

    if ($iteration -lt $Iterations -and $DelaySeconds -gt 0) {
        Start-Sleep -Seconds $DelaySeconds
    }
}

$summary = [pscustomobject]@{
    soak_id = $soakId
    broker = $Broker
    read_only = [bool]$ReadOnly
    account_id = $AccountId
    iterations_requested = $Iterations
    iterations_completed = $iterations.Count
    status = if ($failed) { "failed" } else { "completed" }
    failure_class = $failureClass
    evidence_dir = $soakDir
    iterations = $iterations
}
$summary | ConvertTo-Json -Depth 6 | Set-Content -Path $summaryPath -Encoding UTF8
Write-Host "Production reconciliation soak summary: $summaryPath"

if ($failed) {
    throw "Production reconciliation soak failed; see $summaryPath"
}

$summary
```

- [ ] **Step 2: Extend IBKR soak summary counters**

Modify `scripts/ibkr-paper-soak.ps1` so each iteration summary includes:

```powershell
reconciliation_audits = if ($null -ne $runSummary -and $null -ne $runSummary.reconciliation_audits) { [int]$runSummary.reconciliation_audits } else { 0 }
reconciliation_cash_drifts = if ($null -ne $runSummary -and $null -ne $runSummary.reconciliation_cash_drifts) { [int]$runSummary.reconciliation_cash_drifts } else { 0 }
reconciliation_position_drifts = if ($null -ne $runSummary -and $null -ne $runSummary.reconciliation_position_drifts) { [int]$runSummary.reconciliation_position_drifts } else { 0 }
reconciliation_open_order_drifts = if ($null -ne $runSummary -and $null -ne $runSummary.reconciliation_open_order_drifts) { [int]$runSummary.reconciliation_open_order_drifts } else { 0 }
reconciliation_execution_drifts = if ($null -ne $runSummary -and $null -ne $runSummary.reconciliation_execution_drifts) { [int]$runSummary.reconciliation_execution_drifts } else { 0 }
```

- [ ] **Step 3: Syntax check scripts**

Run:

```powershell
powershell -NoProfile -Command "$null = [System.Management.Automation.PSParser]::Tokenize((Get-Content .\scripts\production-reconciliation-soak.ps1 -Raw), [ref]$null)"
powershell -NoProfile -Command "$null = [System.Management.Automation.PSParser]::Tokenize((Get-Content .\scripts\ibkr-paper-soak.ps1 -Raw), [ref]$null)"
```

Expected: both commands exit 0.

- [ ] **Step 4: Run a local parameter validation smoke**

Run:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\production-reconciliation-soak.ps1 -Broker ibkr -Iterations 0 -ReadOnly -AccountId DU123
```

Expected: fails with `Iterations must be at least 1`.

- [ ] **Step 5: Commit**

```powershell
git add scripts/production-reconciliation-soak.ps1 scripts/ibkr-paper-soak.ps1
git commit -m "feat: add production reconciliation soak script"
```

---

### Task 6: Document Runbook, Results Template, and Roadmap Status

**Files:**

- Create: `docs/production-reconciliation-runbook.md`
- Create: `docs/production-reconciliation-results-template.md`
- Modify: `docs/roadmap.md`
- Modify: `docs/分析.md`

**Interfaces:**

- Consumes: soak summary path from Task 5.
- Produces: clear operator procedure and committed result template.

- [ ] **Step 1: Write runbook**

Create `docs/production-reconciliation-runbook.md`:

```markdown
# Production Reconciliation Runbook

## Purpose

This runbook verifies broker-reported account balances, positions, open orders, and executions against runtime state before any live-money claim.

## Preconditions

- IBKR paper Gateway is running on `127.0.0.1:7497`.
- API mode is ReadOnly unless the run explicitly uses `-ConfirmIbkrPaperOrder`.
- Account id is a real paper account such as `DU...`.
- Runtime config enables broker snapshot and production reconciliation intervals.

## Read-Only Soak

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\production-reconciliation-soak.ps1 -Broker ibkr -Iterations 6 -DelaySeconds 10 -ReadOnly -AccountId DU... -GatewayHost 127.0.0.1 -Port 7497 -ClientId 1
```

## Order-Recovery Soak

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\production-reconciliation-soak.ps1 -Broker ibkr -Iterations 3 -DelaySeconds 10 -AccountId DU... -GatewayHost 127.0.0.1 -Port 7497 -ClientId 1
```

## Evidence

- Raw logs and summaries are written under `data/production-reconciliation/<soak_id>/`.
- Commit only a result document under `docs/production-reconciliation-results-<soak_id>.md`.
- Result documents must include run ids, account id redaction policy, broker, window, iteration count, drift counts, stale input counts, failure class, and raw evidence path.

## Failure Classes

- `gateway_unreachable`: Gateway or TWS was unavailable.
- `account_mismatch`: configured account was not returned by broker.
- `reconciliation_drift`: broker and runtime disagreed on cash, position, order, or execution state.
- `open_orders_remaining`: cleanup did not cancel all expected broker open orders.
- `iteration_failed`: command failed without a more specific class.
```

- [ ] **Step 2: Write result template**

Create `docs/production-reconciliation-results-template.md`:

```markdown
# Production Reconciliation Results: <soak_id>

## Summary

- Broker: ibkr
- Mode: read-only
- Window: <start_iso> to <end_iso>
- Iterations requested: 6
- Iterations completed: 6
- Status: completed
- Failure class: ok
- Evidence directory: `data/production-reconciliation/<soak_id>/`

## Audit Counters

| Counter | Value |
| --- | ---: |
| Reconciliation audits | 0 |
| Cash drifts | 0 |
| Position drifts | 0 |
| Open order drifts | 0 |
| Execution drifts | 0 |
| Stale inputs | 0 |

## Broker Coverage

| Surface | Covered | Notes |
| --- | --- | --- |
| Account balances | yes | Multi-currency if broker reports currency-level values |
| Positions | yes | Includes contract metadata where IBKR exposes it |
| Open orders | yes | Unmatched broker orders fail the run |
| Executions | yes | Missing runtime executions fail the run |
| Liquidation price | partial | Populated for brokers that report it |
| Open interest | partial | Populated when reference-data ingestion supplies it |

## Decision

This run is acceptable for pre-production reconciliation only when all drift counters are zero and failure class is `ok`.
```

- [ ] **Step 3: Update roadmap**

In `docs/roadmap.md`, add an active milestone entry:

```markdown
### Production Reconciliation / Contract Metadata Hardening

- Status: active implementation plan saved in `docs/superpowers/plans/2026-07-07-production-reconciliation-contract-metadata-hardening.md`.
- Scope: broker account balances, positions, open orders, executions, IBKR contract metadata, reconciliation audits, and long-run evidence.
- Exit gate: broker-connected soak produces `failure_class=ok` with zero cash, position, open-order, and execution drift counters.
```

- [ ] **Step 4: Update analysis document**

In `docs/分析.md`, record:

```markdown
## Production Reconciliation / Contract Metadata

Paper acceptance is not the remaining gating item. The next production-readiness gate is broker reconciliation across account balances, positions, open orders, executions, and contract metadata. The hardening plan is `docs/superpowers/plans/2026-07-07-production-reconciliation-contract-metadata-hardening.md`.

Remaining limits after this plan starts:
- IBKR contract metadata coverage depends on fields returned by Gateway for each security type.
- Liquidation price and open interest are partial until each broker/reference-data source reports them.
- Non-IBKR broker account snapshot scheduling remains a follow-up once the shared broker boundary is stable.
```

- [ ] **Step 5: Commit docs**

```powershell
git add docs/production-reconciliation-runbook.md docs/production-reconciliation-results-template.md docs/roadmap.md docs/分析.md
git commit -m "docs: add production reconciliation runbook"
```

---

### Task 7: Full Verification and Broker-Connected Evidence

**Files:**

- Create after successful broker-connected run: `docs/production-reconciliation-results-<soak_id>.md`

**Interfaces:**

- Consumes: all previous tasks and a real IBKR paper Gateway session.
- Produces: committed result summary for the first production reconciliation soak.

- [ ] **Step 1: Run unit and boundary gates**

Run:

```powershell
cargo test -p broker
cargo test -p storage
cargo test -p runtime
cargo test -p config
cargo check --workspace
bash ./scripts/check-db-boundary
bash ./scripts/check-storage-dto-boundary
bash ./scripts/check-api-read-model-boundary
```

Expected: all pass.

- [ ] **Step 2: Run read-only IBKR production reconciliation soak**

Run with the actual paper account:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\production-reconciliation-soak.ps1 -Broker ibkr -Iterations 6 -DelaySeconds 10 -ReadOnly -AccountId DU... -GatewayHost 127.0.0.1 -Port 7497 -ClientId 1
```

Expected:

- Summary path is printed as `data/production-reconciliation/<soak_id>/summary.json`.
- `status` is `completed`.
- `failure_class` is `ok`.
- Drift counters are zero, or the result document records the exact drift and the run is not accepted.

- [ ] **Step 3: Create committed results document**

Copy the fields from `docs/production-reconciliation-results-template.md` into `docs/production-reconciliation-results-<soak_id>.md`, replacing template markers with the actual values from the summary JSON. Redact account id to the first two and last two visible characters, for example `DU****89`.

- [ ] **Step 4: Commit final evidence document**

```powershell
git add docs/production-reconciliation-results-<soak_id>.md
git commit -m "docs: record production reconciliation soak results"
```

---

## Implementation Order

1. Task 1: Broker model and pure reconciliation logic.
2. Task 2: Storage migration and audit persistence.
3. Task 3: Live runtime reconciliation hardening.
4. Task 4: IBKR contract/account mapping.
5. Task 5: Production soak script.
6. Task 6: Runbook and roadmap docs.
7. Task 7: Full verification and broker-connected evidence.

Do not start Task 3 before Tasks 1 and 2 pass because runtime reconciliation needs stable model and storage interfaces. Do not run Task 7 without a real IBKR paper Gateway session.

## Risks and Controls

- **Risk:** Multi-currency cash is falsely collapsed into base currency.
  - **Control:** Persist currency-level balances when available; if IBKR only returns aggregate tags in the current adapter path, record that limitation in the audit payload and docs.
- **Risk:** Reconciliation false positives from stale broker data.
  - **Control:** Use `stale_after_ms` and report stale input separately from cash/position drift.
- **Risk:** Open order and execution matching misses broker ids after restart.
  - **Control:** Match by client order id first, broker order id second, and execution trade id for fills; persist unmatched ids in the audit payload.
- **Risk:** Migration breaks existing local databases.
  - **Control:** New nullable columns only; new tables are additive.
- **Risk:** Broker-connected soak depends on external Gateway state.
  - **Control:** Unit tests cover fake broker behavior; broker soak has clear failure classes and raw evidence paths.

## Self-Review

- Spec coverage: The plan covers broader broker reconciliation, IBKR contract/multi-asset metadata, production audit evidence, open orders/executions/cash/position reconciliation, liquidation/open-interest storage fields, runbooks, and long-run soak evidence.
- Placeholder scan: The plan avoids deferred implementation markers and gives concrete files, commands, payload shapes, and test expectations.
- Type consistency: Broker structs created in Task 1 are consumed by storage/runtime/IBKR tasks using the same field names.

## Success Criteria

The project is materially closer to pre-production when:

- Broker account balances are persisted per currency.
- Broker position snapshots carry contract metadata where available.
- Live runtime writes reconciliation audit records every broker snapshot cycle.
- Cash, position, open-order, execution, and stale-input drift are counted separately.
- Reconciliation alerts contain structured payloads that identify reason, account, currency, symbol, order, or execution.
- IBKR Gateway fake-client tests cover metadata mapping.
- A read-only IBKR paper production reconciliation soak can produce a committed results document with `failure_class=ok` and zero drift counters.
