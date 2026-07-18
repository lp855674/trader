# IBKR Paper Filled-Order Results: 2026-07-14

## Summary

- Date: 2026-07-14
- Scope: single-symbol IBKR paper filled-order reconciliation against a local Gateway
- Symbol and quantity: AAPL BUY 1
- Gateway: `127.0.0.1:4002`, client id `9`
- Account: redacted as `DU...`
- Status: accepted for the single-symbol IBKR paper filled-order scope
- Boundary: this is not multi-asset, live-account, live-money, or production-readiness evidence

## Evidence

The accepted run is `ibkr-aapl-1d-7fd4aa45f34f`. Generated local artifacts are under:

```text
data/ibkr/paper-runs/ibkr-aapl-1d-7fd4aa45f34f/
```

The `filled-order-evidence-summary.json` result records:

```text
status=completed
failure_class=ok
order_submit=enabled
open_orders_remaining=0
reconciliation_status=ok
reconciliation_open_order_drifts=0
reconciliation_execution_drifts=0
broker_executions=2
matched_executions=1
unmatched_executions=1
execution_field_drifts=0
local_fills=1
qty_delta=0
```

The two Gateway executions consist of one execution attributable to this run and one earlier
paper execution. The earlier execution remained visible as unmatched, while field and quantity
comparison used only the execution attributable to this run.

Local SQLite readback for the accepted run records:

```text
broker_order_id=2
client_order_id=trader-paper-ibkr-aapl-1d-7fd-1
order_qty=1
filled_qty=1
order_status=Filled
fill_side=BUY
fill_qty=1
fill_price=312.94
fill_fee=1.000003
```

A fresh post-run Gateway check reported `open_orders=0`. Reconciliation request id `46`
reconfirmed one local order, one local fill, one matched execution, one external unmatched
execution, zero execution field drift, and `qty_delta=0`.

## Implementation Follow-Up

The accepted run exercised these fixes:

- Preserve IBKR `order_reference` as the execution `client_order_id`.
- Attribute remote executions to the current run by broker order id, client order id, or an
  existing local fill identity.
- Keep external executions visible as unmatched without including them in current-run field or
  quantity drift.
- Normalize a locally aggregated fully executed order to `Filled`, even when the last status
  callback observed before the execution was `PreSubmitted`.
- Do not require a fully filled local order to remain visible in Gateway open orders.

## Verification

The following completed successfully after the implementation changes:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\check\verify.ps1
powershell -ExecutionPolicy Bypass -File scripts\check\clippy.ps1
powershell -ExecutionPolicy Bypass -File scripts\ibkr\ibkr-paper-script-tests.ps1
```

`clippy.ps1` exited successfully with existing non-blocking workspace warnings.

## Remaining Scope

This closes the real Gateway-connected AAPL paper filled-order acceptance gap. The roadmap item
for multi-asset IBKR filled-order reconciliation remains open, as do live-account and live-money
validation.
