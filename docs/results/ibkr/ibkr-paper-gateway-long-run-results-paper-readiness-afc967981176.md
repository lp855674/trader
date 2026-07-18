# IBKR Paper Gateway Long Run Results: paper-readiness-afc967981176

## Scope

- Account: `DU...`
- Gateway host: `127.0.0.1`
- Gateway port: `4002`
- Client id: `1`
- Soak iterations: `3`

## Evidence

| Stage | Summary | Status | failure_class | Notes |
| --- | --- | --- | --- | --- |
| Local readiness | `data/verification/paper-readiness/paper-readiness-afc967981176/summary.json` | completed | ok | All five local gates passed. |
| ReadOnly | `data/ibkr/paper-test/read-only-414fa8a031fb/summary.json` | completed | ok | Gateway read-only account, open orders, executions, reconcile, recover, and next-order-id checks passed. |
| AutoRun | `data/ibkr/paper-runs/ibkr-aapl-1d-afb4fdab9323/summary.json` | completed | ok | Confirmed paper order run completed with Gateway checks ok, no halt, and no residual open orders. |
| Soak | `data/ibkr/paper-soak/ibkr-paper-soak-af20e6620229/summary.json` | completed | ok | Three confirmed paper order iterations completed with no halt, no residual open orders, and reconciliation ok. |

## Local Readiness Gates

| Gate | Status |
| --- | --- |
| `reference_data_observable` | ok |
| `reference_data_retry_tests` | ok |
| `ibkr_paper_local_dry_run` | ok |
| `ibkr_read_only_summary_behavior` | ok |
| `ibkr_soak_summary_behavior` | ok |

## Decision

IBKR paper Gateway verification passed for Local readiness, ReadOnly, AutoRun, and Soak. The remaining gap is broader production and real-money readiness, not the paper Gateway validation path.
