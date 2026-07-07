# Production Reconciliation Acceptance Summary

## Scope

This summary records the IBKR production-reconciliation evidence captured on 2026-07-07 for the paper account `DU****91` through IBKR Gateway `127.0.0.1:4002`.

The evidence covers two operating modes:

- Read-only reconciliation, with order submission disabled.
- Paper order-submit reconciliation, with protected AAPL paper orders submitted through the paper account.

## Evidence Matrix

| Run | Mode | Window | Audits | Status | Failure class | Drift or stale counters | Evidence |
| --- | --- | --- | ---: | --- | --- | --- | --- |
| `production-reconciliation-ibkr-7b95d49938eb` | read-only | 2026-07-07T16:32:55+08:00 to 2026-07-07T16:33:56+08:00 | 6 | completed | ok | all zero | `data/production-reconciliation/production-reconciliation-ibkr-7b95d49938eb/` |
| `production-reconciliation-ibkr-5c6291757824` | paper order-submit | 2026-07-07T16:56:52+08:00 to 2026-07-07T16:57:18+08:00 | 3 | completed | ok | all zero | `data/production-reconciliation/production-reconciliation-ibkr-5c6291757824/` |
| `production-reconciliation-ibkr-83c4db22f6a9` | read-only | 2026-07-07T17:39:13+08:00 to 2026-07-07T17:45:11+08:00 | 30 | completed | ok | all zero | `data/production-reconciliation/production-reconciliation-ibkr-83c4db22f6a9/` |

## Aggregate Result

Across the accepted runs:

| Counter | Total |
| --- | ---: |
| Reconciliation audits | 39 |
| Cash drifts | 0 |
| Position drifts | 0 |
| Open order drifts | 0 |
| Execution drifts | 0 |
| Stale inputs | 0 |

The paper order-submit run submitted one protected AAPL paper order per iteration. IBKR reported no fills, no remaining remote open orders after post-order checks, and no missing remote executions during recovery scans.

## Acceptance Decision

The production reconciliation hardening evidence is accepted for the current IBKR paper-account gate.

The accepted evidence demonstrates:

- IBKR account validation succeeds against the configured paper account.
- Read-only reconciliation remains stable across both short and extended soak windows.
- Paper order submission can be exercised without producing cash, position, open-order, execution, or stale-input drift.
- Recovery scans complete without detecting missing remote executions.

## Residual Boundaries

This evidence does not prove live-account trading behavior. It also does not cover filled paper orders, multi-symbol order bursts, IBKR Gateway restarts during a run, or longer overnight soak behavior.

Those scenarios should remain separate gates before expanding beyond the current paper-account reconciliation scope.
