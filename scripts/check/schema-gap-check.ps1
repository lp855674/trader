$ErrorActionPreference = "Stop"

$ProjectRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$MigrationsPath = Join-Path $ProjectRoot "migrations"

$migrationFiles = Get-ChildItem -LiteralPath $MigrationsPath -Filter "*.sql" | Sort-Object Name
$implemented = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)

foreach ($file in $migrationFiles) {
    $content = Get-Content -LiteralPath $file.FullName -Raw
    $matches = [regex]::Matches($content, "CREATE\s+TABLE\s+IF\s+NOT\s+EXISTS\s+([a-zA-Z0-9_]+)", "IgnoreCase")

    foreach ($match in $matches) {
        [void]$implemented.Add($match.Groups[1].Value)
    }
}

$target = @(
    "strategy_runs",
    "instruments",
    "market_calendars",
    "trading_sessions",
    "fee_rules",
    "lot_size_rules",
    "price_limit_rules",
    "crypto_market_meta",
    "funding_rates",
    "corporate_actions_meta",
    "orders",
    "order_events",
    "fills",
    "positions",
    "crypto_positions",
    "account_balances",
    "cash_snapshots",
    "position_snapshots",
    "portfolio_snapshots",
    "risk_events",
    "insights",
    "portfolio_targets",
    "configs",
    "system_logs"
)

$implementedTargets = @($target | Where-Object { $implemented.Contains($_) })
$missing = @($target | Where-Object { -not $implemented.Contains($_) })
$extra = @($implemented | Where-Object { $target -notcontains $_ } | Sort-Object)

Write-Host "Migration tables: $($implemented.Count)"
Write-Host "Target tables implemented: $($implementedTargets.Count) / $($target.Count)"

if ($missing.Count -gt 0) {
    Write-Host "Missing target tables:"
    foreach ($table in $missing) {
        Write-Host "  - $table"
    }
} else {
    Write-Host "All target tables are implemented."
}

if ($extra.Count -gt 0) {
    Write-Host "Additional migration tables:"
    foreach ($table in $extra) {
        Write-Host "  - $table"
    }
}
