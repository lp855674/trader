$ErrorActionPreference = "Stop"
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
