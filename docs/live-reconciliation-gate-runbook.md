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

## Enforcement Policy

The gate is fail-closed by default. Every policy field below defaults to `block`:

```toml
[live.reconciliation_gate]
missing_required_accounts = "block"
missing_required_audit = "block"
insufficient_clean_recent_audits = "block"
audit_too_old = "block"
audit_has_drift = "block"
audit_has_stale_inputs = "block"
log_write_failure = "block"
```

For paper-mode operational drills, a specific condition can be downgraded to `warn_only`:

```toml
[live.reconciliation_gate]
insufficient_clean_recent_audits = "warn_only"
audit_too_old = "warn_only"
```

When a condition is `warn_only`, the gate still records `reconciliation_gate.block` and emits the block alert, but the launch is allowed to continue if every failure reason is warn-only.

`broker.mode = "live"` always forces block enforcement, even if a policy field is set to `warn_only`.

## Audit Readback

Gate decisions are written to `system_logs` with:

- `target=runtime.reconciliation_gate`
- `message=reconciliation_gate.allow` or `message=reconciliation_gate.block`
- `fields.status=allow|block`
- `fields.enforcement_action=allow|block|warn_only`
- `fields.requirements[]` and `fields.failures[]`
- `fields.policy`

Query one run from the CLI:

```powershell
trader logs list `
  --config configs/paper/ibkr_aapl_1d_parquet.toml `
  --run-id <RUN_ID> `
  --target runtime.reconciliation_gate `
  --limit 20
```

Count gate decisions for one run:

```powershell
trader logs count `
  --config configs/paper/ibkr_aapl_1d_parquet.toml `
  --run-id <RUN_ID> `
  --target runtime.reconciliation_gate
```

Export gate decisions for incident review:

```powershell
trader logs export `
  --config configs/paper/ibkr_aapl_1d_parquet.toml `
  --run-id <RUN_ID> `
  --target runtime.reconciliation_gate `
  --output target/reconciliation-gate-<RUN_ID>.jsonl
```

Gate blocks also emit a runtime alert when `[live.alerts]` is enabled:

- `target=runtime.alert`
- `message=reconciliation_gate.block.alert`
- `fields.reason=reconciliation_gate_block`

Query the alert and downstream delivery status:

```powershell
trader reconciliation-gate-alerts-summary `
  --config configs/paper/ibkr_aapl_1d_parquet.toml `
  --run-id <RUN_ID>

trader logs list `
  --config configs/paper/ibkr_aapl_1d_parquet.toml `
  --run-id <RUN_ID> `
  --target runtime.alert `
  --search reconciliation_gate.block.alert

trader logs list `
  --config configs/paper/ibkr_aapl_1d_parquet.toml `
  --run-id <RUN_ID> `
  --target runtime.alert_delivery `
  --search reconciliation_gate.block.alert
```

The equivalent API readback is:

```powershell
curl "http://127.0.0.1:3000/api/v1/runs/<RUN_ID>/system-logs?target=runtime.reconciliation_gate&limit=20"
curl "http://127.0.0.1:3000/api/v1/runs/<RUN_ID>/reconciliation-gate-alerts/summary"
curl "http://127.0.0.1:3000/api/v1/runs/<RUN_ID>/system-logs?target=runtime.alert&search=reconciliation_gate.block.alert"
curl "http://127.0.0.1:3000/api/v1/runs/<RUN_ID>/system-logs?target=runtime.alert_delivery&search=reconciliation_gate.block.alert"
```

## Safety

This command reads stored audit evidence only. It does not submit orders.
