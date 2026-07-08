# Live Reconciliation Gate Results: real-broker-replay-2026-07-08

## Summary

- Date: 2026-07-08
- Scope: archival replay of accepted real-broker-derived IBKR paper and Binance Testnet evidence through `scripts/live-reconciliation-gate.ps1`
- Status: completed
- Failure class: ok
- Decision: allow

## Source Evidence

| Broker/account | Source evidence | Accepted evidence used for replay |
| --- | --- | --- |
| `ibkr:DU****91` | `docs/production-reconciliation-acceptance-summary.md`; `data/production-reconciliation/production-reconciliation-ibkr-83c4db22f6a9/summary.json` | 30 read-only paper-account audits, `status=completed`, `failure_class=ok`, all drift and stale counters `0` |
| `binance:binance-testnet` | `data/binance-paper-soak/binance-paper-soak-8e077496f463/summary.json` | 3 Binance Testnet/paper iterations, `status=completed`, `failure_class=ok`, `order_submit=disabled` |

## Replay Seed

Generated local operator evidence was stored under `data/live-reconciliation-gate-replay/`.

| Broker/account | Clean replay rows | Drift/stale counters |
| --- | ---: | ---: |
| `ibkr:DU****91` | 3 | 0 |
| `binance:binance-testnet` | 3 | 0 |

The replay row timestamps are evaluation ingestion timestamps used to satisfy the gate's recency policy during this operator check. They are not the original broker connection timestamps.

## Verification

Command executed:

```powershell
powershell -ExecutionPolicy Bypass -Command "& .\scripts\live-reconciliation-gate.ps1 -Config 'data/live-reconciliation-gate-replay/real-broker-replay-2026-07-08.toml' -Account @('ibkr:DU****91','binance:binance-testnet') -MinSuccessfulAudits 3 -MaxAuditAgeMs 300000"
```

Observed output:

```text
reconciliation gate ok
```

Exit code: `0`

## Additional Verification

The following checks were rerun on 2026-07-08:

| Check | Result |
| --- | --- |
| `scripts/live-reconciliation-gate-tests.ps1` | pass |
| `cargo test -p broker reconciliation_gate` | pass: 4 passed |
| `cargo test -p config live_reconciliation_gate` | pass: 2 passed |
| `cargo test -p storage lists_latest_reconciliation_audits_for_gate` | pass: 1 passed |
| `cargo test -p trader-cli gate_account_requirement` | pass: 2 passed |
| `cargo check --workspace` | pass |
| Replay DB clean-row query | pass: `ibkr:DU****91` and `binance:binance-testnet` each had 3 clean rows with drift/stale total `0` |
| Gate with stale replay timestamps | blocked as expected with `audit_too_old` and `insufficient_clean_recent_audits` |
| Gate after refreshing replay ingestion timestamps | pass: exit code `0`, `reconciliation gate ok` |

## Boundary

This check closes the narrower operator-evidence gap: the live reconciliation gate can consume multi-broker clean audit evidence derived from accepted IBKR paper and Binance Testnet runs and allow when every required account passes.

This was not a fresh broker connection. It did not connect to IBKR Gateway, TWS, or Binance Testnet during the gate run, and it did not submit orders. A fresh real-broker read-only gate run remains a separate pre-live-enablement check.
