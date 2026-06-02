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
cargo run -p trader-server
cargo run -p trader-cli -- check-config
```

## Paper MVP

```powershell
cargo run -p trader-cli -- migrate --config configs/backtest/ma_cross.toml
cargo run -p trader-cli -- backtest --config configs/backtest/ma_cross.toml
cargo run -p trader-cli -- paper-run --config configs/backtest/ma_cross.toml
cargo run -p trader-server
```

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

Or run the smoke script:

```powershell
$env:TRADER_DATABASE_URL = "sqlite://data/rest-smoke.sqlite"
cargo run -p trader-server

# In another shell:
powershell -ExecutionPolicy Bypass -File .\scripts\rest-smoke.ps1
```

To build, start, and smoke-test the server in one command with an isolated Cargo target directory and temporary SQLite database:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\server-smoke.ps1
```
