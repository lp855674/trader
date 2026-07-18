# Production Reconciliation Runbook

## Purpose

This runbook verifies broker-reported account balances, positions, open orders, and executions against runtime state before any live-money claim.

## Preconditions

- IBKR paper Gateway is running on `127.0.0.1:4002`.
- API mode is ReadOnly unless the run explicitly uses `-ConfirmIbkrPaperOrder`.
- Account id is a real paper account such as `DU...`.
- Runtime config enables broker snapshot and production reconciliation intervals.

## Read-Only Soak

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\production-reconciliation-soak.ps1 -Broker ibkr -Iterations 6 -DelaySeconds 10 -ReadOnly -AccountId DU... -GatewayHost 127.0.0.1 -Port 4002 -ClientId 1
```

## Order-Recovery Soak

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\production-reconciliation-soak.ps1 -Broker ibkr -Iterations 3 -DelaySeconds 10 -AccountId DU... -GatewayHost 127.0.0.1 -Port 4002 -ClientId 1
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
