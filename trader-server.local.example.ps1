# Copy this file to trader-server.local.ps1 and adjust it for your machine.
#
# Deployment-facing variables:
# - TRADER_SERVER_CONFIG: Deployment TOML used by trader-server.
# - TRADER_DATABASE_URL:  Optional override for [database].url in the deployment config file.
# - TRADER_SERVER_BIND:   Optional override for [server].bind.
# - RUST_LOG:             Rust tracing filter, for example info or trader_server=debug,api=debug.
#
# Production recommendation:
# - Keep TRADER_SERVER_BIND behind reverse proxy / firewall.
# - Prefer SQLite file paths under a dedicated writable data directory.
# - Do not commit the copied trader-server.local.ps1 if it contains secrets or machine-local paths.

$env:TRADER_SERVER_CONFIG = "configs/deploy/trader-server.example.toml"
$env:TRADER_DATABASE_URL = "sqlite://data/trader.sqlite"
$env:TRADER_SERVER_BIND = "127.0.0.1:8080"
$env:RUST_LOG = "info"
