# IBKR Paper Order-Submit Reconciliation Results: 2026-07-08

## Summary

- Date: 2026-07-08
- Scope: IBKR paper-account order-submit reconciliation attempts against local IBKR Gateway
- Status: partial paper-submit evidence; not filled-order acceptance
- Failure classes observed:
  - Initial attempt: `iteration_failed` at the production reconciliation wrapper; child soak classified the run as `gateway_unreachable`
  - Follow-up attempt: order submitted successfully, then wrapper failed cleanup because the generic IBKR paper `Broker::cancel_order` path was not implemented
- Decision: do not promote this evidence to filled-order or live-money readiness

## Command

The operator run used the current local paper account from prior accepted read-only evidence, redacted here as `DU...`, and submitted through the IBKR paper Gateway on `127.0.0.1:4002`:

```powershell
.\scripts\production-reconciliation-soak.ps1 `
  -Broker ibkr `
  -Iterations 1 `
  -DelaySeconds 0 `
  -AccountId DU... `
  -GatewayHost 127.0.0.1 `
  -Port 4002 `
  -ClientId 1
```

This intentionally omitted `-ReadOnly`, so the child script enabled `-ConfirmIbkrPaperOrder`.

## Evidence

Generated local evidence:

Initial timeout attempt:

- Production wrapper summary: `data/production-reconciliation/production-reconciliation-ibkr-3408ad3ffecc/summary.json`
- Production wrapper log: `data/production-reconciliation/production-reconciliation-ibkr-3408ad3ffecc/iteration-1.log`
- Child IBKR paper soak summary: `data/ibkr-paper-soak/ibkr-paper-soak-e83d5fbda472/summary.json`
- Child IBKR paper soak log: `data/ibkr-paper-soak/ibkr-paper-soak-e83d5fbda472/iteration-1.log`
- Generated run directory: `data/ibkr-paper-runs/ibkr-aapl-1d-d9e086b0bab7/`

Follow-up submit-with-open-order attempt:

- Production wrapper summary: `data/production-reconciliation/production-reconciliation-ibkr-c6364318447d/summary.json`
- Child IBKR paper soak summary: `data/ibkr-paper-soak/ibkr-paper-soak-0f4c86d0e26b/summary.json`
- Generated run summary: `data/ibkr-paper-runs/ibkr-aapl-1d-54e6198bdd86/summary.json`
- Generated run config: `data/ibkr-paper-runs/ibkr-aapl-1d-54e6198bdd86/config.toml`

Post-fix cleanup verification:

- Production wrapper summary: `data/production-reconciliation/production-reconciliation-ibkr-d7e9c0474e72/summary.json`
- Child IBKR paper soak summary: `data/ibkr-paper-soak/ibkr-paper-soak-1d07b1cbe1c7/summary.json`
- Generated run summary: `data/ibkr-paper-runs/ibkr-aapl-1d-95105a74805e/summary.json`

Filled-order attempts:

- Marketable-data attempt at limit `420`: `data/ibkr-paper-runs/ibkr-aapl-1d-84018ddbbb9e/summary.json`
- Marketable-data attempt at limit `900`: `data/ibkr-paper-runs/ibkr-aapl-1d-3ee05bc9319e/summary.json`
- Final read-only cleanup / parquet restore check: `data/ibkr-paper-runs/ibkr-aapl-1d-b85b5f668605/summary.json`

The generated run config used:

- Symbol: `US:NASDAQ:AAPL:EQUITY`
- Order quantity: `1`
- `order_submit_enabled = true`
- Gateway: `127.0.0.1:4002`

## Observed Result

The `paper-run` command failed while placing the paper limit order:

```text
broker connection error: IBKR paper gateway place limit order response timed out at 127.0.0.1:4002
```

The exception path then ran IBKR paper Gateway checks. Those checks reported:

- Read-only account check: ok
- Open orders: `0`
- Executions: `0`
- Reconciliation: ok, with local order count `1`, local fills `0`, remote open orders `0`, remote executions `0`, and quantity delta `0`
- Recovery scan: ok, with `remaining=0`

Because the run failed before producing a normal child run summary, the wrapper summary kept reconciliation audit counters at `0` and marked the attempt as failed.

After increasing the IBKR Gateway response timeout, the follow-up run reached order submission:

```text
paper completed: signals=1 orders=1
```

That run left a paper open order visible at IBKR:

- Symbol: `AAPL`
- Quantity: `1`
- Status: `Submitted`
- Limit price: below the then-current AAPL market price, so non-fill was expected for a buy limit order

The wrapper then attempted kill-switch cleanup and failed with `broker order not found` because `IbkrPaperGatewayAdapter` had a dedicated `cancel_ibkr_order` CLI path but the generic `Broker::cancel_order` method still returned `OrderNotFound`. The order was manually cancelled with the generated run config, then verified with:

- Open orders: `0`
- Executions: `0`

Code follow-up added `connect_timeout_ms = 15000` for the paper IBKR config path and implemented the generic IBKR paper `Broker::cancel_order` path used by kill-switch cleanup. Verification:

- `cargo test -p config loads_ibkr_stock_parquet_paper_config_from_file`
- `cargo test -p runtime run_spec`
- `cargo check -p api -p trader-cli -p config -p runtime`
- `cargo test -p broker ibkr_paper_gateway_adapter_cancels_broker_open_order_by_id`
- `cargo test -p broker ibkr_paper_gateway_adapter`
- `cargo check -p broker -p trader-cli`

After that fix, a new production reconciliation wrapper run completed:

- Wrapper: `production-reconciliation-ibkr-d7e9c0474e72`
- Child soak: `ibkr-paper-soak-1d07b1cbe1c7`
- Run id: `ibkr-aapl-1d-95105a74805e`
- Result: `status=completed`, `failure_class=ok`
- Reconciliation: `audits=1`, cash/position/open-order/execution/stale-input drifts all `0`
- Final open orders: `0`
- Final executions: `0`

The default low-price limit path therefore now verifies paper submit plus cleanup with no remaining open orders, but still does not verify filled executions.

Two additional filled-order attempts refreshed the run data to generate higher AAPL limit prices (`420` and `900`) and then restored the default parquet from the original sample data with a read-only Gateway check. Both attempts completed without leaving open orders, but neither produced IBKR executions:

- `ibkr-aapl-1d-84018ddbbb9e`: `local_orders=1`, `local_fills=0`, `remote_executions=0`, `qty_delta=0`
- `ibkr-aapl-1d-3ee05bc9319e`: `local_orders=1`, `local_fills=0`, `remote_executions=0`, `qty_delta=0`
- Direct `ibkr-paper-tiny-order` at limit `900` returned `status=PreSubmitted`, `filled_qty=0`; after a short wait, `open_orders=0` and `executions=0`
- Restore/read-only run `ibkr-aapl-1d-b85b5f668605` confirmed `open_orders=0`, `executions=0`, and reconciliation drift counters all `0`

Follow-up code change after these attempts: the IBKR paper executor now waits through a bounded status/execution polling window before cancelling an open `PreSubmitted` / `Submitted` order. This addresses the observed runner-side race where a marketable paper order could be cancelled immediately after the first empty execution poll. No new filled broker execution evidence has been produced by this code change alone.

2026-07-09 follow-up after the bounded polling fix:

- Read-only precheck `ibkr-aapl-1d-03510914a4ea`: Gateway reachable on `127.0.0.1:4002`, open orders `0`, executions `0`, reconciliation drift counters all `0`
- First submit-enabled attempt `ibkr-aapl-1d-7c8d6fb13416`: no order submitted; `paper-preflight` failed before submit with `IBKR paper gateway managed accounts timed out`
- Read-only retry `ibkr-aapl-1d-5d23040942ea`: Gateway reachable again, open orders `0`, executions `0`, reconciliation drift counters all `0`
- Marketable-data submit attempt `ibkr-aapl-1d-530f3b00ddb9`: generated limit `900`, `paper completed: signals=1 orders=1`, local orders `1`, local fills `0`, remote open orders `0`, remote executions `0`, reconciliation drift counters all `0`
- Local DB inspection for `ibkr-aapl-1d-530f3b00ddb9`: order `ibkr-aapl-1d-530f3b00ddb9-order-1`, broker order `6`, final status `Cancelled`, filled quantity `0`; no rows in `fills`
- Local time at the submit attempt was `2026-07-09T10:07:30+08:00`, which is outside the normal US equity session and after the usual US equity extended-hours evening window. This run therefore still does not provide filled-order reconciliation evidence.

2026-07-09 overnight-routing follow-up:

- Added explicit `outside_rth=true` support to the IBKR limit-order request and reran high-limit AAPL paper submit.
- Run `ibkr-aapl-1d-df57e75f1237`: generated limit `900`, submitted via default SMART stock contract with `outside_rth=true`; result remained local orders `1`, local fills `0`, remote open orders `0`, remote executions `0`. Local DB showed broker order `7`, final status `Cancelled`, filled quantity `0`.
- Added optional `[broker] ibkr_route_exchange = "OVERNIGHT"` support and a script parameter `-IbkrRouteExchange OVERNIGHT`, leaving default IBKR paper routing unchanged unless explicitly configured.
- Run `ibkr-aapl-1d-dcd4e0bb0605`: submitted with `outside_rth=true` and route exchange `OVERNIGHT`; IBKR rejected the submit with API error `10329`, stating the order would be routed directly to `OVERNIGHT` and that the limit must be specified in global configuration / API precautionary settings.
- Post-failure Gateway checks for `ibkr-aapl-1d-dcd4e0bb0605`: read-only ok, open orders `0`, executions `0`, reconciliation local orders `1`, local fills `0`, remote executions `0`, quantity delta `0`, recover remaining `0`.
- Added optional `ibkr_override_percentage_constraints = true` wiring and script support via `-IbkrOverridePercentageConstraints`.
- Run `ibkr-aapl-1d-a4759b7284cc`: submitted with `outside_rth=true`, route exchange `OVERNIGHT`, and order-level `override_percentage_constraints=true`; IBKR still rejected the submit with API error `10329`. The run DB recorded broker status `FAILED`, no broker order id, filled quantity `0`, open orders `0`, executions `0`, and no final run summary because `paper-run` failed before normal summary generation.
- The `10329` runs were caused by the program explicitly setting the stock contract exchange to `OVERNIGHT`. That is a useful diagnostic path, but it is not the preferred acceptance path if IBKR expects overnight stock orders to use the normal SMART contract with `outside_rth=true`.
- Follow-up run `ibkr-aapl-1d-5d5c51ae6e66` removed `-IbkrRouteExchange OVERNIGHT`, so the order path used the default SMART stock contract with `outside_rth=true`; this avoided `10329`, but the run did not reach a clean submit because the Gateway stopped responding to the preflight open-orders request. A read-only retry with client id `2`, `ibkr-aapl-1d-eb03d46a1e5b`, also connected to the account but timed out on open-orders. Filled-order acceptance remains open.
- Client id `9` restored clean read-only Gateway checks in `ibkr-aapl-1d-1235746d1e37`: open orders `0`, executions `0`, reconciliation drift counters all `0`.
- SMART submit run `ibkr-aapl-1d-1910a169d3ee` completed without `10329`: local orders `1`, fills `0`, remote open orders `0`, remote executions `0`, quantity delta `0`. Local DB showed broker order `1`, final status `Cancelled`, filled quantity `0`.
- Direct SMART tiny-order probe using the same config submitted order id `2` at limit `900`; after roughly 60 seconds it remained `Submitted` with executions `0`. It was manually cancelled, then verified open orders `0` and executions `0`.

## Boundary

This is partial paper order-submit evidence, not filled-order acceptance evidence. It shows that the Gateway was reachable for read-only/open-orders/executions/reconcile/recover checks after the submit timeout, that paper orders could be submitted, and that the cleanup path can now finish with no remaining open orders. It also identified and fixed the generic cleanup cancel path used by kill-switch. The filled-order attempts did not produce broker executions, so this still does not prove filled paper order reconciliation, multi-symbol burst behavior, Gateway restart recovery, live-account behavior, or live-money readiness.

## Follow-Up

- Re-run a single paper order-submit attempt only after confirming there are no existing paper open orders.
- For filled-order reconciliation evidence, rerun during a US equity trading window or an IBKR-confirmed overnight venue/liquidity window using the default SMART contract path with `outside_rth=true`; do not pass `-IbkrRouteExchange OVERNIGHT` unless explicitly diagnosing IBKR direct-routing behavior. The acceptance run must produce a broker execution and reconcile it to local fills with zero drift.
- Treat filled-order reconciliation as still open until a paper order produces a broker execution and the reconciliation audit matches local fills to remote executions with zero drift.
