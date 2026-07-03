# IBKR Paper Gateway Long Run Results: paper-readiness-afc967981176

## Scope

- Account: `DU...`
- Gateway host: `127.0.0.1`
- Gateway port: `7497`
- Client id: `1`
- Soak iterations: `3`

## Evidence

| Stage | Summary | Status | failure_class | Notes |
| --- | --- | --- | --- | --- |
| Local readiness | `data/paper-readiness/paper-readiness-afc967981176/summary.json` | completed | ok | All five local gates passed. |
| ReadOnly | pending | pending | pending | Requires running IBKR TWS / Gateway in Paper Trading mode and a real `DU...` account id. |
| AutoRun | pending | pending | pending | Blocked until ReadOnly passes against the real Gateway. |
| Soak | pending | pending | pending | Blocked until AutoRun passes against the real Gateway. |

## Local Readiness Gates

| Gate | Status |
| --- | --- |
| `reference_data_observable` | ok |
| `reference_data_retry_tests` | ok |
| `ibkr_paper_local_dry_run` | ok |
| `ibkr_read_only_summary_behavior` | ok |
| `ibkr_soak_summary_behavior` | ok |

## Decision

Local paper readiness passed. Gateway verification is not complete until ReadOnly, AutoRun, and Soak all report `failure_class = ok` against a running IBKR paper Gateway.
