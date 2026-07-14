# IBKR Paper Multi-Asset Filled-Order Results: 2026-07-14

## Summary

- Date: 2026-07-14
- Scope: Gateway-connected IBKR paper multi-asset, multi-execution, and partial-fill
  reconciliation
- Gateway: `127.0.0.1:4002`, client id `9`
- Account: redacted as `DU...`
- Status: accepted for the tested IBKR paper scope
- Boundary: this is not live-account, live-money, production-readiness, or broad market coverage
  evidence

## Multi-Asset Evidence

The AAPL and MSFT cases used isolated runs and databases:

```text
AAPL run=ibkr-aapl-multi-asset-1d-a6711ad6c071
broker_order_id=3
requested_qty=1
filled_qty=1
fill_price=314.6
fill_fee=1.000003
matched_executions=1
unmatched_executions=2

MSFT run=ibkr-msft-multi-asset-retry-1d-785cadea1fc0
broker_order_id=8
requested_qty=1
filled_qty=1
fill_price=386.04
fill_fee=1.000003
matched_executions=1
unmatched_executions=0
```

Both evidence summaries record `status=completed`, `failure_class=ok`, `qty_delta=0`, zero
field/open-order drift, and `open_orders_remaining=0`.

The original two-case matrix remains a failed historical record because its first MSFT limit was
not marketable and produced no execution. The accepted MSFT retry was not replaced by another
AAPL submission solely to manufacture a new aggregate summary.

## Multi-Execution Evidence

Accepted matrix:

```text
data/ibkr-filled-order-matrix/ibkr-filled-order-matrix-fcc96889fa37/summary.json
```

Accepted child run:

```text
run_id=ibkr-msft-multi-execution-1d-540697290145
broker_order_id=12
symbol=MSFT
side=BUY
requested_qty=101
filled_qty=101
order_status=Filled
aggregate_fill_price=386.1
aggregate_fill_fee=1.000303
matched_executions=3
matched_execution_orders=1
max_executions_per_order=3
unmatched_executions=7
local_fills=1
qty_delta=0
open_orders_remaining=0
```

The three current-run Gateway executions were aggregated into one local fill. Seven earlier
MSFT executions remained visible as unmatched and did not affect field or quantity comparison.

## Partial-Fill Evidence

Accepted matrix:

```text
data/ibkr-filled-order-matrix/ibkr-filled-order-matrix-62a5173562bf/summary.json
```

Accepted child run:

```text
run_id=ibkr-bset-partial-fill-1d-a68be3ef4043
broker_order_id=20
symbol=BSET
side=BUY
requested_qty=1001
filled_qty=406
order_status=Cancelled
aggregate_fill_price=21.2
aggregate_fill_fee=2.031218
matched_executions=3
matched_execution_orders=1
max_executions_per_order=3
unmatched_executions=5
local_fills=1
fully_filled_orders=0
partially_filled_orders=1
qty_delta=0
open_orders_remaining=0
```

The executor preserved the 406-share partial fill, cancelled the 595-share remainder after the
settlement window, refreshed executions after cancellation, and left no Gateway open order.

## Defects Found

Two real Gateway behaviors required implementation follow-up:

- IBKR API `10147` can be returned when cancellation races with an order that has already
  disappeared. The executor now treats that response as terminal cancellation and still refreshes
  executions afterward.
- Truncating long run ids to the first 16 characters caused distinct runs with a shared prefix to
  reuse the same IBKR client order id. Long prefixes now use a deterministic 64-bit digest of the
  complete run id; short prefixes retain the existing readable format.

The collision was observed in failed run
`ibkr-msft-multi-execution-1d-46f16952bf13`: an older one-share execution was incorrectly
included, producing `remote_execution_qty=102` for a local quantity of 101. That failed evidence
remains unchanged. The accepted rerun matched one broker order and three executions totaling
exactly 101.

## Remaining Scope

This closes the planned real Gateway acceptance for the tested IBKR paper multi-asset,
multi-execution, and partial-fill cases. Live-account, live-money, long-running order lifecycle,
more asset classes, and broader broker coverage remain open.
