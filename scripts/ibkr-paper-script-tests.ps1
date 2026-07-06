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
    "check-config" { Write-Output "config ok" }
    "paper-preflight" { Write-Output "paper preflight ok: real_broker_connection=true order_submit_enabled=true" }
    "migrate" { Write-Output "migrated" }
    "paper-run" { Write-Output "paper completed: signals=1 orders=1" }
    "report" { Write-Output "report ok" }
    "risk-events" { }
    "risk-kill-switch" { Write-Output "risk kill switch ok: account_id=DU12345 cancel_open_orders=true cancelled=0 symbol=*" }
    "ibkr-paper-readonly" { Write-Output "ibkr paper readonly ok: connected=true account=DU12345" }
    "ibkr-paper-open-orders" {
        if ($env:TRADER_FAKE_MODE -eq "open_orders_settle_once") {
            $stateDir = if ($env:TRADER_FAKE_STATE_DIR) { $env:TRADER_FAKE_STATE_DIR } else { "." }
            $statePath = Join-Path $stateDir "open-orders-seen.txt"
            if (-not (Test-Path $statePath)) {
                "seen" | Set-Content -Path $statePath -Encoding UTF8
                Write-Output "ibkr paper open orders ok: open_orders=1"
                Write-Output "order: id=1 symbol=AAPL status=PendingCancel remaining=1"
            } else {
                Write-Output "ibkr paper open orders ok: open_orders=0"
            }
        } else {
            Write-Output "ibkr paper open orders ok: open_orders=0"
        }
    }
    "ibkr-paper-executions" { Write-Output "ibkr paper executions ok: executions=0" }
    "ibkr-paper-reconcile" { Write-Output "ibkr paper reconcile ok: local_orders=0 remote_open_orders=0 local_fills=0 remote_executions=0" }
    "ibkr-paper-recover" { Write-Output "ibkr paper recover ok: scanned=0 recovered=0 missing=0 remaining=0" }
    "ibkr-paper-next-order-id" { Write-Output "ibkr paper next order id ok: next_order_id=1" }
    default {
        Write-Error "unexpected command: $command"
        exit 2
    }
}
'@ | Set-Content -Path $fakeTrader -Encoding UTF8

$env:TRADER_TEST_EXE = $fakeTrader
$env:TRADER_FAKE_STATE_DIR = $testRoot
$env:TRADER_TEST_GATEWAY_PORT = "reachable"
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

$env:TRADER_TEST_GATEWAY_PORT = "unreachable"
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
Assert-True ($summary.failed_check -eq "gateway_preflight") "expected failed gateway preflight check"

$env:TRADER_TEST_GATEWAY_PORT = "reachable"
$env:TRADER_FAKE_MODE = "ok"
$runOutput = powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-run.ps1 -SkipRefresh -ConfirmIbkrPaperOrder -AccountId DU12345 2>&1
$runOutput | ForEach-Object { Write-Host $_ }
Assert-True ($LASTEXITCODE -eq 0) "expected ibkr paper run success with fake trader"
$runSummaryPath = ($runOutput | Select-String -Pattern 'summary\s+:\s+(.+summary\.json)' | Select-Object -Last 1).Matches.Groups[1].Value.Trim()
Assert-True (-not [string]::IsNullOrWhiteSpace($runSummaryPath)) "expected ibkr paper run summary path"
$runSummary = Read-Json $runSummaryPath
Assert-True ($runSummary.status -eq "completed") "expected ibkr paper run completed status"
Assert-True ($runSummary.failure_class -eq "ok") "expected ibkr paper run ok failure class"
Assert-True ($null -eq $runSummary.halt_reason) "expected null halt reason on success"
Assert-True ($runSummary.open_orders_remaining -eq 0) "expected zero open orders remaining on success"
Assert-True (-not [bool]$runSummary.cancel_all_attempted) "expected no cancel-all attempt on success"
Assert-True ($runSummary.gateway_checks.status -eq "completed") "expected gateway checks completed status"
Assert-True ($runSummary.gateway_checks.failure_class -eq "ok") "expected gateway checks ok failure class"

Remove-Item -LiteralPath (Join-Path $testRoot "open-orders-seen.txt") -Force -ErrorAction SilentlyContinue
$env:TRADER_TEST_GATEWAY_PORT = "reachable"
$env:TRADER_FAKE_MODE = "open_orders_settle_once"
$settledRunOutput = powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-run.ps1 -SkipRefresh -ConfirmIbkrPaperOrder -AccountId DU12345 -OpenOrdersSettleSeconds 3 -OpenOrdersPollSeconds 1 2>&1
$settledRunOutput | ForEach-Object { Write-Host $_ }
Assert-True ($LASTEXITCODE -eq 0) "expected ibkr paper run success after transient open orders settle"
$settledRunSummaryPath = ($settledRunOutput | Select-String -Pattern 'summary\s+:\s+(.+summary\.json)' | Select-Object -Last 1).Matches.Groups[1].Value.Trim()
Assert-True (-not [string]::IsNullOrWhiteSpace($settledRunSummaryPath)) "expected settled ibkr paper run summary path"
$settledRunSummary = Read-Json $settledRunSummaryPath
Assert-True ($settledRunSummary.status -eq "completed") "expected settled ibkr paper run completed status"
Assert-True ($settledRunSummary.failure_class -eq "ok") "expected settled ibkr paper run ok failure class"
Assert-True ($settledRunSummary.open_orders_remaining -eq 0) "expected settled ibkr paper run zero open orders"
Assert-True (-not [bool]$settledRunSummary.cancel_all_attempted) "expected settled ibkr paper run no cancel-all attempt"
Assert-True ($settledRunSummary.gateway_checks.status -eq "completed") "expected settled gateway checks completed"

$env:TRADER_TEST_GATEWAY_PORT = "unreachable"
$env:TRADER_FAKE_MODE = "ok"
$previousErrorActionPreference = $ErrorActionPreference
$ErrorActionPreference = "Continue"
try {
    $preflightFailedRunOutput = powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-run.ps1 -SkipRefresh -ConfirmIbkrPaperOrder -AccountId DU12345 2>&1
} finally {
    $ErrorActionPreference = $previousErrorActionPreference
}
Assert-True ($LASTEXITCODE -ne 0) "expected ibkr paper run failure when gateway preflight fails"
$preflightFailedRunDir = Get-ChildItem -Path (Join-Path $repoRoot "data/ibkr-paper-runs") -Directory -Filter "ibkr-aapl-1d-*" |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1
$preflightFailedRunSummaryPath = Join-Path $preflightFailedRunDir.FullName "summary.json"
$preflightFailedRunSummary = Read-Json $preflightFailedRunSummaryPath
Assert-True ($preflightFailedRunSummary.status -eq "failed") "expected preflight failed ibkr paper run status"
Assert-True ($preflightFailedRunSummary.failure_class -eq "gateway_unreachable") "expected preflight failed ibkr paper run gateway class"
Assert-True ($preflightFailedRunSummary.open_orders_remaining -eq 0) "expected zero open orders on gateway preflight failure"
Assert-True ($preflightFailedRunSummary.gateway_checks.failed_check -eq "gateway_preflight") "expected failed gateway preflight check name"

$env:TRADER_TEST_GATEWAY_PORT = "reachable"
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
Assert-True ($failedRunSummary.open_orders_remaining -eq 0) "expected zero open orders on gateway check failure"
Assert-True ($failedRunSummary.gateway_checks.failed_check -eq "readonly") "expected failed gateway check name"

$env:TRADER_TEST_GATEWAY_PORT = "reachable"
$env:TRADER_FAKE_MODE = "ok"
$soakOutput = powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-soak.ps1 -Iterations 2 -SkipRefresh -ConfirmIbkrPaperOrder -AccountId DU12345 2>&1
$soakOutput | ForEach-Object { Write-Host $_ }
Assert-True ($LASTEXITCODE -eq 0) "expected ibkr paper soak success with fake trader"
$soakSummaryPath = ($soakOutput | Select-String -Pattern 'IBKR paper soak summary:\s+(.+summary\.json)' | Select-Object -Last 1).Matches.Groups[1].Value.Trim()
Assert-True (-not [string]::IsNullOrWhiteSpace($soakSummaryPath)) "expected ibkr soak summary path"
$soakSummary = Read-Json $soakSummaryPath
Assert-True ($soakSummary.status -eq "completed") "expected completed ibkr soak status"
Assert-True ($soakSummary.failure_class -eq "ok") "expected ok ibkr soak failure class"
Assert-True ($soakSummary.iterations_requested -eq 2) "expected two requested ibkr soak iterations"
Assert-True ($soakSummary.iterations_completed -eq 2) "expected two completed ibkr soak iterations"
Assert-True ($soakSummary.iterations.Count -eq 2) "expected two ibkr soak iteration summaries"
foreach ($iteration in $soakSummary.iterations) {
    Assert-True ($iteration.failure_class -eq "ok") "expected ok ibkr soak iteration failure class"
    Assert-True ($iteration.open_orders_remaining -eq 0) "expected zero ibkr soak open orders"
    Assert-True (-not [bool]$iteration.cancel_all_attempted) "expected no ibkr soak cancel-all attempt"
    Assert-True ($iteration.reconciliation_status -eq "ok") "expected ibkr soak reconciliation status"
    Assert-True (-not [string]::IsNullOrWhiteSpace([string]$iteration.summary)) "expected ibkr soak iteration summary path"
}

$env:TRADER_TEST_GATEWAY_PORT = "reachable"
$env:TRADER_FAKE_MODE = "gateway_checks_down"
$previousErrorActionPreference = $ErrorActionPreference
$ErrorActionPreference = "Continue"
try {
    $failedSoakOutput = powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-soak.ps1 -Iterations 2 -SkipRefresh -ConfirmIbkrPaperOrder -AccountId DU12345 2>&1
} finally {
    $ErrorActionPreference = $previousErrorActionPreference
}
Assert-True ($LASTEXITCODE -ne 0) "expected ibkr paper soak failure when gateway checks fail"
$failedSoakSummaryPath = ($failedSoakOutput | Select-String -Pattern 'IBKR paper soak summary:\s+(.+summary\.json)' | Select-Object -Last 1).Matches.Groups[1].Value.Trim()
Assert-True (-not [string]::IsNullOrWhiteSpace($failedSoakSummaryPath)) "expected failed ibkr soak summary path"
$failedSoakSummary = Read-Json $failedSoakSummaryPath
Assert-True ($failedSoakSummary.status -eq "failed") "expected failed ibkr soak status"
Assert-True ($failedSoakSummary.failure_class -eq "gateway_unreachable") "expected gateway_unreachable ibkr soak failure class"
Assert-True ($failedSoakSummary.failed_iteration -eq 1) "expected first ibkr soak iteration to fail"
Assert-True ($failedSoakSummary.iterations_completed -eq 1) "expected ibkr soak to stop after first failure"
Assert-True ($failedSoakSummary.iterations[0].failure_class -eq "gateway_unreachable") "expected failed ibkr soak iteration class"
Assert-True ($failedSoakSummary.iterations[0].open_orders_remaining -eq 0) "expected failed ibkr soak zero open orders"

Remove-Item Env:\TRADER_TEST_EXE -ErrorAction SilentlyContinue
Remove-Item Env:\TRADER_FAKE_STATE_DIR -ErrorAction SilentlyContinue
Remove-Item Env:\TRADER_TEST_GATEWAY_PORT -ErrorAction SilentlyContinue
Remove-Item Env:\TRADER_FAKE_MODE -ErrorAction SilentlyContinue
Write-Host "IBKR paper script tests passed"
