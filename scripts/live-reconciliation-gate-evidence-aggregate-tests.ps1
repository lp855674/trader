$ErrorActionPreference = "Stop"

$script = Join-Path $PSScriptRoot "live-reconciliation-gate-evidence-aggregate.ps1"
if (-not (Test-Path $script)) {
    throw "missing live-reconciliation-gate-evidence-aggregate.ps1"
}

$content = Get-Content $script -Raw
if ($content -notmatch "Validate-IbkrSummary") {
    throw "aggregate script does not validate IBKR summary"
}
if ($content -notmatch "Validate-BinanceSummary") {
    throw "aggregate script does not validate Binance summary"
}
if ($content -notmatch "order_submit must be disabled") {
    throw "aggregate script does not reject Binance order submission evidence"
}
if ($content -notmatch "broker_reconciliation_audits") {
    throw "aggregate script does not write reconciliation audit rows"
}
if ($content -notmatch "required_accounts") {
    throw "aggregate script does not write gate required accounts"
}

Write-Host "live reconciliation gate evidence aggregate script tests ok"
