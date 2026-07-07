# Production Reconciliation Results: production-reconciliation-ibkr-5c6291757824

## Summary

- Broker: ibkr
- Mode: paper order-submit
- Account: DU****91
- Gateway: 127.0.0.1:4002
- Window: 2026-07-07T16:56:52+08:00 to 2026-07-07T16:57:18+08:00
- Iterations requested: 3
- Iterations completed: 3
- Status: completed
- Failure class: ok
- Evidence directory: `data/production-reconciliation/production-reconciliation-ibkr-5c6291757824/`

## Audit Counters

| Counter | Value |
| --- | ---: |
| Reconciliation audits | 3 |
| Cash drifts | 0 |
| Position drifts | 0 |
| Open order drifts | 0 |
| Execution drifts | 0 |
| Stale inputs | 0 |

## Broker Coverage

| Surface | Covered | Notes |
| --- | --- | --- |
| Account validation | yes | IBKR Gateway returned the configured paper account every iteration |
| Paper order submit | yes | Each iteration enabled `order_submit_enabled` and submitted one AAPL paper order |
| Open orders | yes | IBKR returned zero open orders after each run's post-order checks |
| Executions | yes | IBKR returned zero AAPL executions; the submitted orders did not fill |
| Recovery checks | yes | Recover scans completed with no missing remote executions |

## Order Outcome

Each iteration submitted one protected IBKR paper order. The orders did not fill, no remote open orders remained, and no executions were reported. Local terminal or cancelling zero-fill orders are not counted as open-order drift when IBKR no longer reports them as open and no execution exists.

## Decision

This paper order-submit run is acceptable for pre-production order-recovery reconciliation evidence: all three audits completed with `failure_class=ok` and all drift and stale-input counters at zero.
