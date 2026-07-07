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
