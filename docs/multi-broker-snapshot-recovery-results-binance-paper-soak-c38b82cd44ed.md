# Multi-Broker Snapshot Recovery Results: binance-paper-soak-c38b82cd44ed

## Summary

- Broker: Binance Testnet
- Account: redacted testnet account
- Mode: no-submit paper/read-only soak
- Window: 2026-07-12 local operator run
- Status: completed
- Failure class: ok
- Evidence directory: `data/binance-paper-soak/binance-paper-soak-c38b82cd44ed/`

## Command

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\binance-paper-soak.ps1 -Iterations 3 -Limit 100 -DelaySeconds 0 -SkipRefresh
```

## Iterations

| Iteration | Run ID | Status | Failure class | Reconciliation status | Open orders remaining | Order submit |
| ---: | --- | --- | --- | --- | ---: | --- |
| 1 | `binance-btcusdt-1m-c89758fef1da` | completed | ok | ok | 0 | disabled |
| 2 | `binance-btcusdt-1m-fab975c2d94e` | completed | ok | ok | 0 | disabled |
| 3 | `binance-btcusdt-1m-869af30f81e5` | completed | ok | ok | 0 | disabled |

## Snapshot Evidence

| Run ID | Cash snapshots | Position snapshots | Reconciliation audits | Drift events |
| --- | ---: | ---: | ---: | ---: |
| `binance-btcusdt-1m-c89758fef1da` | 101 | 98 | 0 | 0 |
| `binance-btcusdt-1m-fab975c2d94e` | 101 | 98 | 0 | 0 |
| `binance-btcusdt-1m-869af30f81e5` | 101 | 98 | 0 | 0 |

## Recovery Evidence

| Surface | Count | Notes |
| --- | ---: | --- |
| Broker open orders inspected | 3 | Each iteration called Binance open-order cleanup checks |
| Broker executions inspected | 3 | Each iteration called Binance recovery/reconcile checks |
| Recovered orders | 0 | Recovery reported `recovered=0` in all iterations |
| Recovery warnings | 0 | Recovery reported `missing=0` and `remaining=0` in all iterations |

## Decision

This run is accepted as stronger supplemental Binance Testnet no-submit paper/read-only evidence than the earlier one-off recovery check. It proves three broker-connected iterations retained run databases with cash and position snapshot counts, no drift events, no residual open orders, and no order submission.

This run does not close the full external broker-connected snapshot/reconciliation audit gap because `broker_reconciliation_audits` remained at 0 for the Binance paper path. Closing that gap still requires a broker-connected runtime path that persists reconciliation audit rows, currently the IBKR paper Gateway production-reconciliation path.
