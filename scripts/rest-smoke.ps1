$ErrorActionPreference = "Stop"

$baseUrl = $env:TRADER_BASE_URL
if (-not $baseUrl) {
    $baseUrl = "http://127.0.0.1:8080"
}

Invoke-RestMethod "$baseUrl/api/v1/health" | Out-Null
$paper = Invoke-RestMethod -Method Post "$baseUrl/api/v1/paper-runs"
$fills = Invoke-RestMethod "$baseUrl/api/v1/fills"
$balances = Invoke-RestMethod "$baseUrl/api/v1/account-balances"
$snapshots = Invoke-RestMethod "$baseUrl/api/v1/portfolio/snapshots"
$metrics = Invoke-RestMethod "$baseUrl/api/v1/metrics"

if (@($fills).Count -lt 1) { throw "expected at least one fill" }
if (@($balances).Count -lt 1) { throw "expected at least one account balance" }
if (@($snapshots).Count -lt 1) { throw "expected at least one portfolio snapshot" }
if ($metrics.fill_count -lt 1) { throw "expected metrics fill_count >= 1" }

[pscustomobject]@{
    signals = $paper.signals
    orders = $paper.orders
    fills = @($fills).Count
    balances = @($balances).Count
    snapshots = @($snapshots).Count
    total_return = $metrics.total_return
}
