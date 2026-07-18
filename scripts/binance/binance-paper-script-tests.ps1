param()

$ErrorActionPreference = "Stop"

$repoRoot = Get-Location
$testRoot = Join-Path $repoRoot "target/binance-paper-script-tests"
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

function Write-FakeTrader {
    param([string]$Mode)

    @"
`$command = `$args[0]

switch (`$command) {
    "check-config" { Write-Output "config ok" }
    "paper-preflight" { Write-Output "paper preflight ok: real_broker_connection=true order_submit_enabled=false" }
    "migrate" { Write-Output "migrated" }
    "paper-run" { Write-Output "paper completed: signals=1 orders=0" }
    "report" { Write-Output "report ok" }
    "binance-paper-recover" { Write-Output "binance paper recover ok: scanned=0 recovered=0 missing=0 remaining=0" }
    "binance-paper-open-orders" {
        if ("$Mode" -eq "open_orders_remaining") {
            Write-Output "binance paper open orders ok: symbol=BTCUSDT open_orders=2"
        } else {
            Write-Output "binance paper open orders ok: symbol=BTCUSDT open_orders=0"
        }
    }
    "binance-paper-reconcile" { Write-Output "binance paper reconcile ok: local_orders=0 remote_open_orders=0 local_fills=0 remote_trades=0" }
    "risk-events" {
        if ("$Mode" -eq "stale_market_data") {
            Write-Output "risk_event: run_id=binance-script-test ts_ms=1 account=paper symbol=BTCUSDT risk_type=stale_market_data decision=rejected reason=market data age 6000ms exceeds limit 5000ms threshold=5000 observed_value=6000"
        }
        if ("$Mode" -eq "short_selling_disabled") {
            Write-Output "risk_event: run_id=binance-script-test ts_ms=1 account=paper symbol=BTCUSDT risk_type=short_selling_disabled decision=rejected reason=short selling is disabled threshold=0 observed_value=-1"
        }
    }
    "risk-kill-switch" { Write-Output "risk kill switch ok: account_id=paper cancel_open_orders=true cancelled=0 symbol=BTCUSDT" }
    default {
        Write-Error "unexpected command: `$command"
        exit 2
    }
}
"@ | Set-Content -Path $fakeTrader -Encoding UTF8
}

Remove-Item -LiteralPath $testRoot -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $testRoot | Out-Null

$env:TRADER_TEST_EXE = $fakeTrader

Write-FakeTrader "ok"
$successOutput = powershell -ExecutionPolicy Bypass -File .\scripts\binance\binance-paper-run.ps1 -SkipRefresh 2>&1
$successOutput | ForEach-Object { Write-Host $_ }
Assert-True ($LASTEXITCODE -eq 0) "expected binance paper run success with fake trader"
$successSummaryPath = ($successOutput | Select-String -Pattern 'summary\s+:\s+(.+summary\.json)' | Select-Object -Last 1).Matches.Groups[1].Value.Trim()
Assert-True (-not [string]::IsNullOrWhiteSpace($successSummaryPath)) "expected success summary path"
$successSummary = Read-Json $successSummaryPath
Assert-True ($successSummary.status -eq "completed") "expected completed success status"
Assert-True ($successSummary.failure_class -eq "ok") "expected ok success failure class"
Assert-True ($null -eq $successSummary.halt_reason) "expected null halt reason on success"
Assert-True ($successSummary.open_orders_remaining -eq 0) "expected zero open orders remaining on success"
Assert-True (-not [bool]$successSummary.cancel_all_attempted) "expected no cancel-all attempt on success"

Write-FakeTrader "short_selling_disabled"
$nonHaltOutput = powershell -ExecutionPolicy Bypass -File .\scripts\binance\binance-paper-run.ps1 -SkipRefresh 2>&1
$nonHaltOutput | ForEach-Object { Write-Host $_ }
Assert-True ($LASTEXITCODE -eq 0) "expected binance paper run success with non-halt risk rejection"
$nonHaltSummaryPath = ($nonHaltOutput | Select-String -Pattern 'summary\s+:\s+(.+summary\.json)' | Select-Object -Last 1).Matches.Groups[1].Value.Trim()
Assert-True (-not [string]::IsNullOrWhiteSpace($nonHaltSummaryPath)) "expected non-halt summary path"
$nonHaltSummary = Read-Json $nonHaltSummaryPath
Assert-True ($nonHaltSummary.status -eq "completed") "expected completed non-halt status"
Assert-True ($nonHaltSummary.failure_class -eq "ok") "expected ok non-halt failure class"
Assert-True ($null -eq $nonHaltSummary.halt_reason) "expected null halt reason on non-halt risk rejection"
Assert-True ($nonHaltSummary.risk_rejections.Count -ge 1) "expected recorded non-halt risk rejection"
Assert-True ($nonHaltSummary.risk_rejections[0].risk_type -eq "short_selling_disabled") "expected short_selling_disabled risk rejection"

Write-FakeTrader "stale_market_data"
$previousErrorActionPreference = $ErrorActionPreference
$ErrorActionPreference = "Continue"
try {
    $haltOutput = powershell -ExecutionPolicy Bypass -File .\scripts\binance\binance-paper-run.ps1 -SkipRefresh 2>&1
} finally {
    $ErrorActionPreference = $previousErrorActionPreference
}
Assert-True ($LASTEXITCODE -ne 0) "expected binance paper run failure when risk halt is present"
$haltSummaryPath = ($haltOutput | Select-String -Pattern 'summary\s+:\s+(.+summary\.json)' | Select-Object -Last 1).Matches.Groups[1].Value.Trim()
Assert-True (-not [string]::IsNullOrWhiteSpace($haltSummaryPath)) "expected halt summary path"
$haltSummary = Read-Json $haltSummaryPath
Assert-True ($haltSummary.failure_class -eq "stale_market_data") "expected stale_market_data failure class"
Assert-True ($haltSummary.halt_reason -eq "stale_market_data") "expected stale_market_data halt reason"
Assert-True ($haltSummary.risk_rejections.Count -ge 1) "expected recorded risk rejections"

Write-FakeTrader "open_orders_remaining"
$previousErrorActionPreference = $ErrorActionPreference
$ErrorActionPreference = "Continue"
try {
    $openOrdersOutput = powershell -ExecutionPolicy Bypass -File .\scripts\binance\binance-paper-run.ps1 -SkipRefresh 2>&1
} finally {
    $ErrorActionPreference = $previousErrorActionPreference
}
Assert-True ($LASTEXITCODE -ne 0) "expected binance paper run failure when open orders remain"
$openOrdersSummaryPath = ($openOrdersOutput | Select-String -Pattern 'summary\s+:\s+(.+summary\.json)' | Select-Object -Last 1).Matches.Groups[1].Value.Trim()
Assert-True (-not [string]::IsNullOrWhiteSpace($openOrdersSummaryPath)) "expected open-orders summary path"
$openOrdersSummary = Read-Json $openOrdersSummaryPath
Assert-True ($openOrdersSummary.failure_class -eq "open_orders_remaining") "expected open_orders_remaining failure class"
Assert-True ($openOrdersSummary.open_orders_remaining -eq 2) "expected residual open order count"
Assert-True ([bool]$openOrdersSummary.cancel_all_attempted) "expected cancel-all attempt when open orders remain"
Assert-True (-not [bool]$openOrdersSummary.cancel_all_succeeded) "expected failed cancel-all outcome when residual orders remain"

Write-FakeTrader "ok"
$soakOutput = powershell -ExecutionPolicy Bypass -File .\scripts\binance\binance-paper-soak.ps1 -Iterations 2 -SkipRefresh 2>&1
$soakOutput | ForEach-Object { Write-Host $_ }
Assert-True ($LASTEXITCODE -eq 0) "expected binance paper soak success with fake trader"
$soakSummaryPath = ($soakOutput | Select-String -Pattern 'Binance paper soak summary:\s+(.+summary\.json)' | Select-Object -Last 1).Matches.Groups[1].Value.Trim()
Assert-True (-not [string]::IsNullOrWhiteSpace($soakSummaryPath)) "expected binance soak summary path"
$soakSummary = Read-Json $soakSummaryPath
Assert-True ($soakSummary.status -eq "completed") "expected completed binance soak status"
Assert-True ($soakSummary.failure_class -eq "ok") "expected ok binance soak failure class"
Assert-True ($soakSummary.iterations_requested -eq 2) "expected two requested binance soak iterations"
Assert-True ($soakSummary.iterations_completed -eq 2) "expected two completed binance soak iterations"
Assert-True ($soakSummary.iterations.Count -eq 2) "expected two binance soak iteration summaries"
foreach ($iteration in $soakSummary.iterations) {
    Assert-True ($iteration.failure_class -eq "ok") "expected ok binance soak iteration failure class"
    Assert-True ($iteration.open_orders_remaining -eq 0) "expected zero binance soak open orders"
    Assert-True (-not [bool]$iteration.cancel_all_attempted) "expected no binance soak cancel-all attempt"
    Assert-True ($iteration.reconciliation_status -eq "ok") "expected binance soak reconciliation status"
    Assert-True (-not [string]::IsNullOrWhiteSpace([string]$iteration.summary)) "expected binance soak iteration summary path"
}

Write-FakeTrader "open_orders_remaining"
$previousErrorActionPreference = $ErrorActionPreference
$ErrorActionPreference = "Continue"
try {
    $failedSoakOutput = powershell -ExecutionPolicy Bypass -File .\scripts\binance\binance-paper-soak.ps1 -Iterations 2 -SkipRefresh 2>&1
} finally {
    $ErrorActionPreference = $previousErrorActionPreference
}
Assert-True ($LASTEXITCODE -ne 0) "expected binance paper soak failure when open orders remain"
$failedSoakSummaryPath = ($failedSoakOutput | Select-String -Pattern 'Binance paper soak summary:\s+(.+summary\.json)' | Select-Object -Last 1).Matches.Groups[1].Value.Trim()
Assert-True (-not [string]::IsNullOrWhiteSpace($failedSoakSummaryPath)) "expected failed binance soak summary path"
$failedSoakSummary = Read-Json $failedSoakSummaryPath
Assert-True ($failedSoakSummary.status -eq "failed") "expected failed binance soak status"
Assert-True ($failedSoakSummary.failure_class -eq "open_orders_remaining") "expected open_orders_remaining binance soak failure class"
Assert-True ($failedSoakSummary.failed_iteration -eq 1) "expected first binance soak iteration to fail"
Assert-True ($failedSoakSummary.iterations_completed -eq 1) "expected binance soak to stop after first failure"
Assert-True ($failedSoakSummary.iterations[0].open_orders_remaining -eq 2) "expected failed binance soak residual open orders"
Assert-True ([bool]$failedSoakSummary.iterations[0].cancel_all_attempted) "expected failed binance soak cancel-all attempt"
Assert-True (-not [bool]$failedSoakSummary.iterations[0].cancel_all_succeeded) "expected failed binance soak cancel-all outcome"

Remove-Item Env:\TRADER_TEST_EXE -ErrorAction SilentlyContinue
Write-Host "Binance paper script tests passed"
