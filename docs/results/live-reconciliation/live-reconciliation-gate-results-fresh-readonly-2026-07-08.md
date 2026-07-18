# Live Reconciliation Gate Results: fresh-readonly-2026-07-08

## Summary

- Date: 2026-07-08
- Scope: fresh IBKR paper Gateway read-only reconciliation, fresh Binance paper/Testnet no-submit soak, scripted aggregation, and multi-broker gate evaluation from generated local evidence
- Status: completed
- Failure class: ok
- Decision: allow

## Fresh Evidence

| Broker/account | Command scope | Evidence | Result |
| --- | --- | --- | --- |
| `ibkr:DU****91` | 3-iteration production reconciliation soak, `ReadOnly=true`, `order_submit=disabled` | `data/reconciliation/production/production-reconciliation-ibkr-d7003dfd4f98/summary.json` | 3 completed audits; `failure_class=ok`; cash, position, open-order, execution, and stale counters all `0` |
| `binance:binance-testnet` | 3-iteration Binance paper/Testnet soak, `SkipRefresh=true`, `order_submit=disabled` | `data/binance/paper-soak/binance-paper-soak-0997bf7ea21d/summary.json` | 3 completed iterations; `failure_class=ok`; `reconciliation_status=ok`; open orders remaining `0` |

## Gate Input

Generated local operator evidence was stored under `data/reconciliation/live-gate-replay/`.

The reusable aggregation script was verified with these fresh summaries:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\reconciliation\live-reconciliation-gate-evidence-aggregate.ps1 -IbkrSummary data\reconciliation\production\production-reconciliation-ibkr-d7003dfd4f98\summary.json -IbkrAccount DU****91 -BinanceSummary data\binance\paper-soak\binance-paper-soak-0997bf7ea21d\summary.json -BinanceAccount binance-testnet -EvidenceId gate-evidence-fresh-readonly-2026-07-08 -MinSuccessfulAudits 3 -MaxAuditAgeMs 300000
```

Generated files:

- `gate-evidence-fresh-readonly-2026-07-08.sqlite`
- `gate-evidence-fresh-readonly-2026-07-08.toml`

The gate input used 3 clean rows for each required account:

| Broker/account | Clean rows | Drift/stale counters |
| --- | ---: | ---: |
| `ibkr:DU****91` | 3 | 0 |
| `binance:binance-testnet` | 3 | 0 |

The gate replay rows use evaluation ingestion timestamps. The source evidence above is the fresh broker/testnet verification; the replay database is the gate-readable aggregation layer.

## Verification

Fresh IBKR read-only command:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\reconciliation\production-reconciliation-soak.ps1 -Broker ibkr -Iterations 3 -DelaySeconds 10 -ReadOnly -AccountId DU****91 -GatewayHost 127.0.0.1 -Port 4002 -ClientId 1
```

Fresh Binance no-submit command:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\binance\binance-paper-soak.ps1 -Iterations 3 -Limit 100 -DelaySeconds 0 -SkipRefresh
```

Gate command:

```powershell
powershell -ExecutionPolicy Bypass -Command "& .\scripts\reconciliation\live-reconciliation-gate.ps1 -Config 'data/reconciliation/live-gate-replay/gate-evidence-fresh-readonly-2026-07-08.toml' -Account @('ibkr:DU****91','binance:binance-testnet') -MinSuccessfulAudits 3 -MaxAuditAgeMs 300000"
```

Observed gate output:

```text
reconciliation gate ok
```

Exit code: `0`

## Boundary

This check exercised fresh IBKR paper Gateway read-only reconciliation and fresh Binance paper/Testnet no-submit soak evidence before running the live reconciliation gate across both required accounts.

It did not submit IBKR paper orders, Binance Testnet orders, or live-money orders. The gate still consumed a generated local SQLite aggregation of the fresh evidence because the source runs write separate evidence databases; that aggregation is now produced by `scripts/reconciliation/live-reconciliation-gate-evidence-aggregate.ps1`.
