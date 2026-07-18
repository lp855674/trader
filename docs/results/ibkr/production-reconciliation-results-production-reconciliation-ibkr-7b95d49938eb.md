# Production Reconciliation Results: production-reconciliation-ibkr-7b95d49938eb

## Summary

- Broker: ibkr
- Mode: read-only
- Account: DU****91
- Gateway: 127.0.0.1:4002
- Window: 2026-07-07T16:32:55+08:00 to 2026-07-07T16:33:56+08:00
- Iterations requested: 6
- Iterations completed: 6
- Status: completed
- Failure class: ok
- Evidence directory: `data/reconciliation/production/production-reconciliation-ibkr-7b95d49938eb/`

## Audit Counters

| Counter | Value |
| --- | ---: |
| Reconciliation audits | 6 |
| Cash drifts | 0 |
| Position drifts | 0 |
| Open order drifts | 0 |
| Execution drifts | 0 |
| Stale inputs | 0 |

## Broker Coverage

| Surface | Covered | Notes |
| --- | --- | --- |
| Account balances | yes | Read-only Gateway account validation completed each iteration |
| Positions | yes | Reconciliation run used an empty local read-only state and observed no position drift |
| Open orders | yes | IBKR returned zero open orders for all six iterations |
| Executions | yes | IBKR returned zero AAPL executions for all six iterations |
| Liquidation price | partial | Populated for brokers that report it |
| Open interest | partial | Populated when reference-data ingestion supplies it |

## Decision

This read-only run is acceptable for pre-production reconciliation evidence: all six audits completed with `failure_class=ok` and all drift and stale-input counters at zero.
