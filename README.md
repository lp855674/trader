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
```
