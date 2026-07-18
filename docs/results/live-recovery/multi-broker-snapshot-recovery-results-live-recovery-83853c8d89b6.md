# Multi-Broker Snapshot Recovery Results: live-recovery-83853c8d89b6

## Summary

- Broker: Binance Testnet
- Account: redacted testnet account
- Mode: no-submit read-only recovery
- Window: 2026-07-12 local run
- Status: completed
- Failure class: ok
- Evidence directory: `data/verification/live-recovery/live-recovery-83853c8d89b6/`

## Command

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\check\verify-live-recovery.ps1 -Iterations 1 -IncludeBinanceReadOnly -IncludeBinanceNetwork
```

## Snapshot Evidence

| Surface | Count | Notes |
| --- | ---: | --- |
| Account balance snapshots | 0 | Not exercised by this no-submit recovery check |
| Position snapshots | 0 | Not exercised by this no-submit recovery check |
| Reconciliation audits | 0 | Not exercised by this no-submit recovery check |
| Broker snapshot system logs | 0 | Not exercised by this no-submit recovery check |

## Recovery Evidence

| Surface | Count | Notes |
| --- | ---: | --- |
| Broker open orders inspected | 0 | Adapter reported `scanned=0` |
| Broker executions inspected | 0 | Adapter reported `trades=0` |
| Recovered orders | 0 | Adapter reported `recovered=0` |
| Recovery warnings | 0 | Adapter reported `missing=0` and `remaining=0` |

## Drift Counters

| Counter | Value |
| --- | ---: |
| Cash drifts | 0 |
| Position drifts | 0 |
| Open order drifts | 0 |
| Execution drifts | 0 |
| Stale inputs | 0 |

## Local Verification In Same Task 5 Pass

- `powershell -ExecutionPolicy Bypass -File .\scripts\check\verify.ps1`: exit 0.
- `powershell -ExecutionPolicy Bypass -File .\scripts\check\clippy.ps1`: exit 0 with existing warnings.
- `cargo test -p broker`: exit 0, 65 tests passed across broker unit/integration tests.
- `cargo test -p runtime`: exit 0, 53 tests passed across runtime unit/integration tests.
- `cargo test -p storage`: exit 0, 48 tests passed across storage integration tests.
- `powershell -ExecutionPolicy Bypass -File .\scripts\check\check-db-boundary.ps1`: exit 0.
- `powershell -ExecutionPolicy Bypass -File .\scripts\check\check-storage-dto-boundary.ps1`: exit 0.
- `powershell -ExecutionPolicy Bypass -File .\scripts\check\check-api-read-model-boundary.ps1`: exit 0.

## Decision

This run is accepted as partial broker-connected no-submit recovery evidence only. It proves the Binance Testnet recovery/read-only path was reachable, order submission stayed disabled, and no remote open orders or executions required recovery in this account at run time.

It is not full broker-connected acceptance evidence for persisted account snapshots, position snapshots, or reconciliation audit rows. That remaining gap requires an external broker-connected snapshot/reconciliation run that preserves the generated evidence database or summary counts.
