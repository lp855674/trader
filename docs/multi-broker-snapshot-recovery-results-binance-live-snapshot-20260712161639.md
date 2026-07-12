# Multi-Broker Snapshot Recovery Results: binance-live-snapshot-20260712161639

## Summary

- Broker: Binance Spot Testnet
- Account: redacted testnet account (`binance-testnet`)
- Mode: live-worker read-only, paper broker mode, order submit disabled
- Window: 2026-07-12 local operator run
- Status: completed
- Evidence directory: `data/binance-live-snapshot/binance-live-snapshot-20260712161639/`

## Command Shape

The operator run launched the rebuilt CLI live worker with a JSON launch file:

```powershell
target\debug\trader.exe live-worker --launch-file data\binance-live-snapshot\binance-live-snapshot-20260712161639\launch.json
```

The launch config used:

- `[runtime] mode = "live"`
- `[broker] kind = "binance"`, `mode = "paper"`, `base_url = "https://testnet.binance.vision/api"`
- `api_key_env = "BINANCE_TESTNET_API_KEY"` and `secret_key_env = "BINANCE_TESTNET_SECRET_KEY"`
- `order_submit_enabled = false`
- `[risk] trading_halted = true`
- top-level launch `broker_snapshot_interval_ms = 1000`

The worker was allowed to collect broker snapshots, then stopped over JSONL shutdown:

```json
{"type":"shutdown","request_id":"stop-1","reason":"operator evidence"}
```

## Worker Evidence

| Field | Value |
| --- | --- |
| Run ID | `binance-live-snapshot-20260712161639` |
| Exit code | 0 |
| stderr | empty |
| stdout events | 14 |
| Order submit | disabled |
| Trading halted | true |

## Snapshot And Audit Evidence

CLI readback:

```text
reconciliation: run_id=binance-live-snapshot-20260712161639 status=ok cash_snapshots=5 position_snapshots=0 reconciliation_audits=4 latest_audit_broker=binance latest_audit_account=binance-testnet latest_audit_severity=info drift_events=0
```

Audit table distribution:

| Severity | Cash drift | Position drift | Open-order drift | Execution drift | Stale inputs | Rows |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| info | 0 | 0 | 0 | 0 | 0 | 4 |

Broker snapshot logs:

| Target | Message | Count | Cash |
| --- | --- | ---: | ---: |
| `runtime.broker_snapshot` | `broker.snapshot.cash` | 4 | `9301.30375000 USDT` |

Position snapshots are `0` because this run uses the Binance spot testnet adapter. Spot holdings are not modeled as futures positions in this runtime path.

## Implementation Notes

- The CLI live-worker broker selection now routes Binance paper mode to `BinanceSpotTestnetAdapter` instead of the fake broker.
- The Binance spot adapter no longer calls the futures `/fapi/v2/positionRisk` endpoint when asked for spot position snapshots; it returns an empty position set for this spot runtime path.
- The Binance spot account snapshot exposes USDT base cash for runtime cash reconciliation and sets fresh `source_ts_ms`, avoiding false multi-asset cash and stale-input audit drift.

## Decision

This run closes the previously documented external broker-connected snapshot/reconciliation audit evidence gap for Binance Testnet read-only coverage. It proves the broker-connected live-worker path persisted broker snapshots and `broker_reconciliation_audits` with no drift, no stale inputs, and no order submission.

This does not claim filled-order recovery, live-money trading readiness, or multi-asset spot portfolio valuation. Those remain separate production validation scopes.
