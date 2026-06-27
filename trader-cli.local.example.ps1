# Copy this file to trader-cli.local.ps1 and adjust it for your machine.
#
# CLI configuration notes:
# - Most trader CLI subcommands accept --config <path>. Use trader-cli.ps1 -Config <path>
#   to append it automatically when you do not pass --config yourself.
# - RUST_LOG controls structured log verbosity during CLI execution.
# - Broker-specific commands may require extra environment variables, for example:
#   BINANCE_TESTNET_API_KEY / BINANCE_TESTNET_SECRET_KEY
#   IBKR connection settings if you use IBKR paper tooling

$env:RUST_LOG = "info"

# Optional examples:
# $env:BINANCE_TESTNET_API_KEY = "replace-me"
# $env:BINANCE_TESTNET_SECRET_KEY = "replace-me"
