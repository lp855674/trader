$ErrorActionPreference = "Stop"

$script = Join-Path $PSScriptRoot "..\reconciliation\live-reconciliation-gate.ps1"
if (-not (Test-Path $script)) {
    throw "missing live-reconciliation-gate.ps1"
}

$content = Get-Content $script -Raw
if ($content -notmatch "reconciliation-gate") {
    throw "wrapper does not call reconciliation-gate"
}
if ($content -notmatch "MinSuccessfulAudits") {
    throw "wrapper does not expose MinSuccessfulAudits"
}
if ($content -notmatch "MaxAuditAgeMs") {
    throw "wrapper does not expose MaxAuditAgeMs"
}

Write-Host "live reconciliation gate script tests ok"
