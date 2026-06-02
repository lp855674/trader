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
cargo run -p trader-server
```
