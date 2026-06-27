# Trader

Rust quant trading system.

## Verify

```powershell
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
```

## Run

```powershell
.\trader-server.ps1
.\trader-cli.ps1 check-config
```

Windows helper scripts:

- `trader-server.ps1`
  Loads optional `trader-server.local.ps1`, then runs `trader-server` with `TRADER_CONFIG`, `TRADER_DATABASE_URL`, `TRADER_SERVER_BIND`, and `RUST_LOG`.
- `trader-cli.ps1`
  Loads optional `trader-cli.local.ps1`, then runs `trader` CLI commands. Use `-Config <path>` to append `--config <path>` automatically.

Examples:

```powershell
.\trader-server.ps1 -Config configs/deploy/trader-server.example.toml -Bind 127.0.0.1:8080
.\trader-cli.ps1 backtest -Config configs/backtest/ma_cross.toml
.\trader-cli.ps1 logs metrics -Config configs/backtest/ma_cross.toml
```

## Paper MVP

```powershell
.\trader-cli.ps1 migrate -Config configs/backtest/ma_cross.toml
.\trader-cli.ps1 backtest -Config configs/backtest/ma_cross.toml
.\trader-cli.ps1 paper-run -Config configs/backtest/ma_cross.toml
.\trader-server.ps1
```

Paper runtime now enforces MVP core order rules before simulated broker fills: market rules, order-level risk, execution delta, and OMS lifecycle.

## REST API

After starting `trader-server`, run a local paper workflow and query persisted state:

```powershell
Invoke-RestMethod -Method Post http://127.0.0.1:8080/api/v1/backtests
Invoke-RestMethod -Method Post http://127.0.0.1:8080/api/v1/paper-runs
Invoke-RestMethod http://127.0.0.1:8080/api/v1/orders
Invoke-RestMethod http://127.0.0.1:8080/api/v1/fills
Invoke-RestMethod http://127.0.0.1:8080/api/v1/positions
Invoke-RestMethod http://127.0.0.1:8080/api/v1/account-balances
Invoke-RestMethod http://127.0.0.1:8080/api/v1/portfolio/snapshots
Invoke-RestMethod http://127.0.0.1:8080/api/v1/metrics
Invoke-RestMethod http://127.0.0.1:8080/api/v1/runs
Invoke-RestMethod http://127.0.0.1:8080/api/v1/runs/sample-ma-cross/status
Invoke-RestMethod -Method Post http://127.0.0.1:8080/api/v1/runs/sample-ma-cross/cancel
```

`POST /api/v1/paper-runs` starts a background paper run and returns `{ run_id, status }`.
Poll `GET /api/v1/runs/{run_id}/status` until `completed`, `failed`, or `cancelled`.
Use `POST /api/v1/runs/{run_id}/cancel` to request cancellation of an active run.

Or run the smoke script:

```powershell
$env:TRADER_DATABASE_URL = "sqlite://data/rest-smoke.sqlite"
.\trader-server.ps1

# In another shell:
powershell -ExecutionPolicy Bypass -File .\scripts\rest-smoke.ps1
```

To build, start, and smoke-test the server in one command with an isolated Cargo target directory and temporary SQLite database:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\server-smoke.ps1
```
