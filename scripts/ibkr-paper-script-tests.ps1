param()

$ErrorActionPreference = "Stop"

$repoRoot = Get-Location
$testRoot = Join-Path $repoRoot "target/ibkr-paper-script-tests"
$fakeTrader = Join-Path $testRoot "fake-trader.ps1"

function Assert-True {
    param(
        [bool]$Condition,
        [string]$Message
    )

    if (-not $Condition) {
        throw $Message
    }
}

function Read-Json {
    param([string]$Path)
    return Get-Content -Path $Path -Raw | ConvertFrom-Json
}

Remove-Item -LiteralPath $testRoot -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $testRoot | Out-Null

@'
$command = $args[0]
if ($env:TRADER_FAKE_MODE -eq "gateway_down") {
    Write-Error "unable to connect to IBKR paper gateway"
    exit 1
}
if ($env:TRADER_FAKE_MODE -eq "gateway_checks_down" -and $command -like "ibkr-paper-*") {
    Write-Error "unable to connect to IBKR paper gateway"
    exit 1
}

switch ($command) {
    "check-config" { Write-Host "config ok" }
    "paper-preflight" { Write-Host "paper preflight ok: real_broker_connection=true order_submit_enabled=true" }
    "migrate" { Write-Host "migrated" }
    "paper-run" { Write-Host "paper completed: signals=1 orders=1" }
    "report" { Write-Host "report ok" }
    "ibkr-paper-readonly" { Write-Host "ibkr paper readonly ok: connected=true account=DU12345" }
    "ibkr-paper-open-orders" { Write-Host "ibkr paper open orders ok: open_orders=0" }
    "ibkr-paper-executions" { Write-Host "ibkr paper executions ok: executions=0" }
    "ibkr-paper-reconcile" { Write-Host "ibkr paper reconcile ok: local_orders=0 remote_open_orders=0 local_fills=0 remote_executions=0" }
    "ibkr-paper-recover" { Write-Host "ibkr paper recover ok: scanned=0 recovered=0 missing=0 remaining=0" }
    "ibkr-paper-next-order-id" { Write-Host "ibkr paper next order id ok: next_order_id=1" }
    default {
        Write-Error "unexpected command: $command"
        exit 2
    }
}
'@ | Set-Content -Path $fakeTrader -Encoding UTF8

$env:TRADER_TEST_EXE = $fakeTrader
$env:TRADER_FAKE_MODE = "ok"
$successOutput = powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-test-guide.ps1 -Stage ReadOnly -AccountId DU12345 2>&1
$successOutput | ForEach-Object { Write-Host $_ }
Assert-True ($LASTEXITCODE -eq 0) "expected read-only success with fake trader"
$latest = Get-ChildItem -Path (Join-Path $repoRoot "data/ibkr-paper-test") -Directory -Filter "read-only-*" |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1
Assert-True ($null -ne $latest) "expected read-only summary directory"
$summary = Read-Json (Join-Path $latest.FullName "summary.json")
Assert-True ($summary.status -eq "completed") "expected completed summary"
Assert-True ($summary.failure_class -eq "ok") "expected ok failure class"
Assert-True ($summary.checks.Count -eq 6) "expected six read-only checks"

$env:TRADER_FAKE_MODE = "gateway_down"
$previousErrorActionPreference = $ErrorActionPreference
$ErrorActionPreference = "Continue"
try {
    $failureOutput = powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-test-guide.ps1 -Stage ReadOnly -AccountId DU12345 2>&1
} finally {
    $ErrorActionPreference = $previousErrorActionPreference
}
Assert-True ($LASTEXITCODE -ne 0) "expected read-only failure with fake gateway down"
$latest = Get-ChildItem -Path (Join-Path $repoRoot "data/ibkr-paper-test") -Directory -Filter "read-only-*" |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1
$summary = Read-Json (Join-Path $latest.FullName "summary.json")
Assert-True ($summary.status -eq "failed") "expected failed summary"
Assert-True ($summary.failure_class -eq "gateway_unreachable") "expected gateway_unreachable classification"
Assert-True ($summary.failed_check -eq "readonly") "expected failed readonly check"

$env:TRADER_FAKE_MODE = "ok"
$runOutput = powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-run.ps1 -SkipRefresh -ConfirmIbkrPaperOrder -AccountId DU12345 2>&1
$runOutput | ForEach-Object { Write-Host $_ }
Assert-True ($LASTEXITCODE -eq 0) "expected ibkr paper run success with fake trader"
$runSummaryPath = ($runOutput | Select-String -Pattern 'summary\s+:\s+(.+summary\.json)' | Select-Object -Last 1).Matches.Groups[1].Value.Trim()
Assert-True (-not [string]::IsNullOrWhiteSpace($runSummaryPath)) "expected ibkr paper run summary path"
$runSummary = Read-Json $runSummaryPath
Assert-True ($runSummary.status -eq "completed") "expected ibkr paper run completed status"
Assert-True ($runSummary.failure_class -eq "ok") "expected ibkr paper run ok failure class"
Assert-True ($runSummary.gateway_checks.status -eq "completed") "expected gateway checks completed status"
Assert-True ($runSummary.gateway_checks.failure_class -eq "ok") "expected gateway checks ok failure class"

$env:TRADER_FAKE_MODE = "gateway_checks_down"
$previousErrorActionPreference = $ErrorActionPreference
$ErrorActionPreference = "Continue"
try {
    $failedRunOutput = powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-run.ps1 -SkipRefresh -ConfirmIbkrPaperOrder -AccountId DU12345 2>&1
} finally {
    $ErrorActionPreference = $previousErrorActionPreference
}
Assert-True ($LASTEXITCODE -ne 0) "expected ibkr paper run failure when post-run gateway checks fail"
$failedRunDir = Get-ChildItem -Path (Join-Path $repoRoot "data/ibkr-paper-runs") -Directory -Filter "ibkr-aapl-1d-*" |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1
$failedRunSummaryPath = Join-Path $failedRunDir.FullName "summary.json"
Assert-True (-not [string]::IsNullOrWhiteSpace($failedRunSummaryPath)) "expected failed ibkr paper run summary path"
$failedRunSummary = Read-Json $failedRunSummaryPath
Assert-True ($failedRunSummary.status -eq "failed") "expected failed ibkr paper run status"
Assert-True ($failedRunSummary.failure_class -eq "gateway_unreachable") "expected failed ibkr paper run gateway class"
Assert-True ($failedRunSummary.gateway_checks.failed_check -eq "readonly") "expected failed gateway check name"

Remove-Item Env:\TRADER_TEST_EXE -ErrorAction SilentlyContinue
Remove-Item Env:\TRADER_FAKE_MODE -ErrorAction SilentlyContinue
Write-Host "IBKR paper script tests passed"
