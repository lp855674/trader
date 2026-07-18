param(
    [string]$Config = "configs/paper/ibkr_aapl_1d_parquet.toml",
    [string[]]$Account = @(),
    [int]$MinSuccessfulAudits = 1,
    [int64]$MaxAuditAgeMs = 300000
)

$ErrorActionPreference = "Stop"

$args = @(
    "run", "-p", "trader-cli", "--",
    "reconciliation-gate",
    "--config", $Config,
    "--min-successful-audits", $MinSuccessfulAudits,
    "--max-audit-age-ms", $MaxAuditAgeMs
)

foreach ($item in $Account) {
    $args += @("--account", $item)
}

cargo @args
exit $LASTEXITCODE
