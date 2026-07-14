# IBKR Paper Multi-Asset Filled-Order Runbook

## Scope

This runbook drives the next acceptance stage after the accepted single-symbol AAPL paper run.
It covers:

- multiple assets, with one isolated run and database per case;
- multiple Gateway executions aggregated into one local fill;
- partial fills whose open remainder is cancelled and leaves no open order.

The scripts and fake-client contracts are locally verified. Real Gateway acceptance results for
the tested AAPL, MSFT, and BSET paper cases are recorded in
`docs/ibkr-paper-multi-asset-filled-order-results-2026-07-14.md`. This runbook does not establish
live-account or live-money readiness.

## Matrix Input

Create a local JSON file whose top-level value is an array. Each case uses its own config and
market-data paths:

```json
[
  {
    "name": "aapl-1d",
    "config": "configs/paper/ibkr_aapl_1d_parquet.toml",
    "input_csv": "datasets/acceptance/aapl_1d.csv",
    "output_parquet": "datasets/ibkr/aapl_acceptance_1d.parquet",
    "min_broker_executions": 1,
    "min_matched_executions": 1,
    "min_executions_per_order": 1,
    "min_local_fills": 1,
    "min_fully_filled_orders": 1,
    "min_partially_filled_orders": 0
  },
  {
    "name": "msft-1d",
    "config": "configs/paper/ibkr_msft_1d_parquet.toml",
    "input_csv": "datasets/acceptance/msft_1d.csv",
    "output_parquet": "datasets/ibkr/msft_acceptance_1d.parquet"
  }
]
```

`name` must contain only letters, digits, `.`, `_`, or `-`. It becomes part of the generated run
id. The config remains authoritative for account, strategy symbol, quantity, and risk limits.

## Multi-Asset Gate

With Gateway connected to the paper account:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-filled-order-matrix.ps1 `
  -CasesPath .\data\ibkr-filled-order-cases.json `
  -AccountId DU... `
  -GatewayHost 127.0.0.1 `
  -Port 4002 `
  -ClientId 9 `
  -ConfirmIbkrPaperOrder
```

The matrix executes cases sequentially and stops after the first failure. Its aggregate result is
written under `data/ibkr-filled-order-matrix/<matrix-id>/summary.json`. Each child evidence file
remains under `data/ibkr-paper-runs/<run-id>/filled-order-evidence-summary.json`.

## Multiple Executions

To accept only an order represented by at least two matched Gateway executions:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-filled-order-evidence.ps1 `
  -Config <config> -InputCsv <csv> -OutputParquet <parquet> -RunLabel <asset-label> `
  -AccountId DU... -Port 4002 -ClientId 9 -ConfirmIbkrPaperOrder `
  -MinBrokerExecutions 2 -MinMatchedExecutions 2 -MinExecutionsPerOrder 2
```

The expected relationship is `matched_executions >= 2`,
`matched_execution_orders >= 1`, `max_executions_per_order >= 2`, and `local_fills >= 1`.
Execution count and local fill count are intentionally not required to be equal.

## Partial Fill

Use explicit thresholds so a partial fill cannot satisfy the default full-fill gate:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-filled-order-evidence.ps1 `
  -Config <config> -InputCsv <csv> -OutputParquet <parquet> -RunLabel <asset-label> `
  -AccountId DU... -Port 4002 -ClientId 9 -ConfirmIbkrPaperOrder `
  -MinFullyFilledOrders 0 -MinPartiallyFilledOrders 1
```

A passing partial-fill case must have at least one matched execution and local fill,
`partially_filled_orders >= 1`, `qty_delta=0`, zero field/open-order drift, and
`open_orders_remaining=0`. The local order should preserve its partial `filled_qty` and end in
`Cancelled` after the unfilled remainder is cancelled.

## Acceptance Record

For every real Gateway case, archive or redact:

- matrix and child run ids;
- symbol, side, requested quantity, filled quantity, and broker order id;
- `broker_executions`, `matched_executions`, `matched_execution_orders`, and
  `max_executions_per_order`;
- full/partial local order counts, local fill count, fees, and `qty_delta`;
- unmatched historical executions and all drift counters;
- final Gateway open-order count.

Do not accept a new case until its real evidence file records `status=completed`,
`failure_class=ok`, and all required case thresholds.
