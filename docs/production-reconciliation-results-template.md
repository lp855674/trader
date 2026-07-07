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
