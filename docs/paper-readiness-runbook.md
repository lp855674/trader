# Paper Readiness Runbook

This runbook covers the local paper-readiness gate and the IBKR paper validation flow. It is designed for two modes:

- no local IBKR TWS / Gateway available
- local IBKR TWS / Gateway running with a real paper account id

## No Gateway Local Gate

Run this before treating the paper path as ready for local validation:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\paper-readiness.ps1
```

The default gate does not connect to a real IBKR Gateway and does not submit orders. It runs:

```text
reference_data_observable
reference_data_retry_tests
ibkr_paper_local_dry_run
ibkr_read_only_summary_behavior
ibkr_soak_summary_behavior
```

It also runs the existing cargo checks/tests and Binance no-network paper smokes. The summary is written to:

```text
data/paper-readiness/{readiness_id}/summary.json
```

Expected local result:

```json
{
  "status": "completed",
  "gates": {
    "reference_data_observable": { "status": "ok" },
    "reference_data_retry_tests": { "status": "ok" },
    "ibkr_paper_local_dry_run": { "status": "ok" },
    "ibkr_read_only_summary_behavior": { "status": "ok" },
    "ibkr_soak_summary_behavior": { "status": "ok" }
  }
}
```

Useful narrower runs:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\paper-readiness.ps1 -SkipBinance -SkipIbkr
powershell -ExecutionPolicy Bypass -File .\scripts\paper-readiness.ps1 -SkipCargo -SkipBinance
powershell -ExecutionPolicy Bypass -File .\scripts\paper-readiness.ps1 -SkipCargo -SkipReferenceData -SkipBinance -SkipIbkr
```

Use `-SkipBinance -SkipIbkr` when checking only Rust and reference-data readiness. Use `-SkipCargo -SkipBinance` when checking only local IBKR script behavior.

## With IBKR Gateway

Prerequisites:

- Start TWS or IB Gateway in Paper Trading mode.
- Enable API socket clients.
- Use paper port `7497` unless your local setup differs.
- Use the real paper account id returned by Gateway, usually `DU...`.
- Keep the account id out of committed config files; pass it as a parameter.

Read-only validation:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-test-guide.ps1 `
  -Stage ReadOnly `
  -AccountId DU12345 `
  -GatewayHost 127.0.0.1 `
  -Port 7497 `
  -ClientId 1
```

Expected summary:

```text
data/ibkr-paper-test/read-only-{id}/summary.json
status = completed
failure_class = ok
failed_check = ""
```

Automatic paper runner with order submission enabled:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-test-guide.ps1 `
  -Stage AutoRun `
  -AccountId DU12345 `
  -GatewayHost 127.0.0.1 `
  -Port 7497 `
  -ClientId 1 `
  -ConfirmAutoRun
```

This enables `order_submit_enabled` only in a generated temporary run config. The runner writes:

```text
data/ibkr-paper-runs/{run_id}/summary.json
```

Expected summary fields:

```text
status = completed
failure_class = ok
order_submit = enabled
gateway_checks.status = completed
gateway_checks.failure_class = ok
```

If the Gateway socket is not reachable before order submission, the runner exits non-zero and writes the same summary path with `failure_class = gateway_unreachable` and `gateway_checks.failed_check = gateway_preflight`.

Soak validation:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-soak.ps1 `
  -Iterations 3 `
  -AccountId DU12345 `
  -GatewayHost 127.0.0.1 `
  -Port 7497 `
  -ClientId 1 `
  -ConfirmIbkrPaperOrder
```

Expected summary:

```text
data/ibkr-paper-soak/{soak_id}/summary.json
status = completed
failure_class = ok
iterations_completed = iterations_requested
```

## Failure Classes

`ok`

The gate or Gateway check passed.

`gateway_unreachable`

The script could not connect to TWS / IB Gateway, or the socket timed out. Check that Gateway is running in Paper Trading mode, API socket clients are enabled, host/port match the script parameters, and no other session is blocking the configured `client_id`.

`account_mismatch`

Gateway responded, but the configured account id was not returned by managed accounts. Re-run read-only validation with the real `DU...` account id from TWS / Gateway, or update only the generated local config.

`command_failed`

A read-only command failed for a reason that was not classified as connection or account mismatch. Open the command log beside the read-only `summary.json` and inspect the exact stderr/stdout.

`iteration_failed`

An IBKR soak iteration failed outside the more specific connection/account/open-order classes. Open `first_failed_log` in the soak summary, then open the iteration runner summary referenced by that log.

`open_orders_remaining`

The Gateway run completed but left remote open orders. Inspect the soak iteration summary, then use the read-only open-orders command to confirm the remote state. Cancel only with an explicit confirmation command:

```powershell
trader ibkr-paper-cancel-order `
  --config configs/paper/ibkr_aapl_1d_parquet.toml `
  --order-id <ORDER_ID> `
  --confirm-ibkr-paper-cancel
```

## Generated Evidence

The readiness and IBKR scripts intentionally generate evidence under `data/`:

```text
data/paper-readiness/{readiness_id}/summary.json
data/ibkr-paper-test/read-only-{id}/summary.json
data/ibkr-paper-runs/{run_id}/summary.json
data/ibkr-paper-soak/{soak_id}/summary.json
```

Keep the latest passing summary for handoff or incident notes. Do not commit generated `data/` output.
