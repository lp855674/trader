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
