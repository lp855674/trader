# Production Reconciliation Results: production-reconciliation-ibkr-83c4db22f6a9

## Summary

- Broker: ibkr
- Mode: read-only
- Account: DU****91
- Gateway: 127.0.0.1:4002
- Window: 2026-07-07T17:39:13+08:00 to 2026-07-07T17:45:11+08:00
- Iterations requested: 30
- Iterations completed: 30
- Status: completed
- Failure class: ok
- Evidence directory: `data/reconciliation/production/production-reconciliation-ibkr-83c4db22f6a9/`

## Audit Counters

| Counter | Value |
| --- | ---: |
| Reconciliation audits | 30 |
| Cash drifts | 0 |
| Position drifts | 0 |
| Open order drifts | 0 |
| Execution drifts | 0 |
| Stale inputs | 0 |

## Broker Coverage

| Surface | Covered | Notes |
| --- | --- | --- |
| Account validation | yes | IBKR Gateway returned the configured paper account every iteration |
| Read-only enforcement | yes | Each iteration skipped `paper-run` and kept order submission disabled |
| Open orders | yes | IBKR returned zero open orders for all thirty iterations |
| Executions | yes | IBKR returned zero AAPL executions for all thirty iterations |
| Recovery checks | yes | Recover scans completed with no missing remote executions |

## Decision

This longer read-only run is acceptable as stability evidence for production reconciliation: all thirty audits completed with `failure_class=ok` and all drift and stale-input counters at zero.
