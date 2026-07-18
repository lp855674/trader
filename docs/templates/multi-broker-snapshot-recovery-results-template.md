# Multi-Broker Snapshot Recovery Results: <run_id>

## Summary

- Broker: <broker_kind>
- Account: <redacted_account_id>
- Mode: read-only or no-submit
- Window: <start_iso> to <end_iso>
- Status: completed
- Failure class: ok
- Evidence directory: `data/multi-broker-snapshot-recovery/<run_id>/`

## Snapshot Evidence

| Surface | Count | Notes |
| --- | ---: | --- |
| Account balance snapshots | 0 | Broker-reported account/cash rows persisted through `Broker::snapshot_bundle` |
| Position snapshots | 0 | Broker-reported position rows persisted through `Broker::snapshot_bundle` |
| Reconciliation audits | 0 | Run-scoped broker reconciliation audit rows |
| Broker snapshot system logs | 0 | `runtime.broker_snapshot` records |

## Recovery Evidence

| Surface | Count | Notes |
| --- | ---: | --- |
| Broker open orders inspected | 0 | Unknown remote open orders must be zero for an accepted run |
| Broker executions inspected | 0 | Missing runtime executions must be zero for an accepted run |
| Recovered orders | 0 | `broker.order.recovered` projection rows, if any |
| Recovery warnings | 0 | Must be explained if non-zero |

## Drift Counters

| Counter | Value |
| --- | ---: |
| Cash drifts | 0 |
| Position drifts | 0 |
| Open order drifts | 0 |
| Execution drifts | 0 |
| Stale inputs | 0 |

## Decision

This run is acceptable broker-connected evidence only when `failure_class = ok`, all drift counters are zero, unknown remote open orders are zero, and the account id is redacted in committed documentation.
