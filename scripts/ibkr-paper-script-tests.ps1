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
$isMatrixFailureCase = $env:TRADER_FAKE_MODE -eq "matrix_second_failure" -and (($args -join " ") -match "ibkr-msft-1d-")
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
    "paper-run" {
        if ($env:TRADER_FAIL_ON_PAPER_RUN -eq "1") {
            Write-Error "paper-run must not be called"
            exit 3
        }
        Write-Output "paper completed: signals=1 orders=1"
    }
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
    "ibkr-paper-executions" {
        if (($env:TRADER_FAKE_MODE -eq "matrix_second_failure" -and -not $isMatrixFailureCase) -or $env:TRADER_FAKE_MODE -in @("filled_execution", "execution_field_drift", "filled_execution_with_external_unmatched", "multiple_executions", "partial_fill")) {
            $executionCount = if ($env:TRADER_FAKE_MODE -in @("filled_execution_with_external_unmatched", "multiple_executions")) { 2 } else { 1 }
            Write-Output "ibkr paper executions ok: request_id=1 account=DU12345 symbol=AAPL executions=$executionCount order_id=7 trade_id=T1"
        } else {
            Write-Output "ibkr paper executions ok: executions=0"
        }
    }
    "ibkr-paper-reconcile" {
        if ($env:TRADER_FAKE_MODE -eq "reconciliation_drift") {
            Write-Output "ibkr paper reconcile ok: local_orders=1 local_fills=0 matched_orders=0 local_only_orders=1 remote_open_orders=0 remote_open_matched=0 remote_open_unmatched=0 remote_executions=0 remote_execution_matched=0 remote_execution_matched_orders=0 remote_execution_max_per_order=0 remote_execution_unmatched=0 remote_execution_field_drifts=0 local_fully_filled_orders=0 local_partially_filled_orders=0 local_fill_qty=0 remote_execution_qty=0 qty_delta=0"
        } elseif ($env:TRADER_FAKE_MODE -eq "filled_execution" -or ($env:TRADER_FAKE_MODE -eq "matrix_second_failure" -and -not $isMatrixFailureCase)) {
            Write-Output "ibkr paper reconcile ok: symbol=AAPL local_orders=1 local_fills=1 matched_orders=0 local_only_orders=0 remote_open_orders=0 remote_open_matched=0 remote_open_unmatched=0 remote_executions=1 remote_execution_matched=1 remote_execution_matched_orders=1 remote_execution_max_per_order=1 remote_execution_unmatched=0 remote_execution_field_drifts=0 local_fully_filled_orders=1 local_partially_filled_orders=0 local_fill_qty=1 remote_execution_qty=1 qty_delta=0"
        } elseif ($env:TRADER_FAKE_MODE -eq "filled_execution_with_external_unmatched") {
            Write-Output "ibkr paper reconcile ok: symbol=AAPL local_orders=1 local_fills=1 matched_orders=0 local_only_orders=0 remote_open_orders=0 remote_open_matched=0 remote_open_unmatched=0 remote_executions=2 remote_execution_matched=1 remote_execution_matched_orders=1 remote_execution_max_per_order=1 remote_execution_unmatched=1 remote_execution_field_drifts=0 local_fully_filled_orders=1 local_partially_filled_orders=0 local_fill_qty=1 remote_execution_qty=1 qty_delta=0"
        } elseif ($env:TRADER_FAKE_MODE -eq "multiple_executions") {
            Write-Output "ibkr paper reconcile ok: symbol=AAPL local_orders=1 local_fills=1 matched_orders=0 local_only_orders=0 remote_open_orders=0 remote_open_matched=0 remote_open_unmatched=0 remote_executions=2 remote_execution_matched=2 remote_execution_matched_orders=1 remote_execution_max_per_order=2 remote_execution_unmatched=0 remote_execution_field_drifts=0 local_fully_filled_orders=1 local_partially_filled_orders=0 local_fill_qty=1 remote_execution_qty=1 qty_delta=0"
        } elseif ($env:TRADER_FAKE_MODE -eq "partial_fill") {
            Write-Output "ibkr paper reconcile ok: symbol=AAPL local_orders=1 local_fills=1 matched_orders=0 local_only_orders=0 remote_open_orders=0 remote_open_matched=0 remote_open_unmatched=0 remote_executions=1 remote_execution_matched=1 remote_execution_matched_orders=1 remote_execution_max_per_order=1 remote_execution_unmatched=0 remote_execution_field_drifts=0 local_fully_filled_orders=0 local_partially_filled_orders=1 local_fill_qty=0.5 remote_execution_qty=0.5 qty_delta=0"
        } elseif ($env:TRADER_FAKE_MODE -eq "execution_field_drift") {
            Write-Output "ibkr paper reconcile ok: symbol=AAPL local_orders=1 local_fills=1 matched_orders=0 local_only_orders=0 remote_open_orders=0 remote_open_matched=0 remote_open_unmatched=0 remote_executions=1 remote_execution_matched=1 remote_execution_matched_orders=1 remote_execution_max_per_order=1 remote_execution_unmatched=0 remote_execution_field_drifts=1 local_fully_filled_orders=1 local_partially_filled_orders=0 local_fill_qty=1 remote_execution_qty=1 qty_delta=0"
        } else {
            Write-Output "ibkr paper reconcile ok: local_orders=0 local_fills=0 matched_orders=0 local_only_orders=0 remote_open_orders=0 remote_open_matched=0 remote_open_unmatched=0 remote_executions=0 remote_execution_matched=0 remote_execution_matched_orders=0 remote_execution_max_per_order=0 remote_execution_unmatched=0 remote_execution_field_drifts=0 local_fully_filled_orders=0 local_partially_filled_orders=0 local_fill_qty=0 remote_execution_qty=0 qty_delta=0"
        }
    }
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
$env:TRADER_FAIL_ON_PAPER_RUN = "1"
$readOnlyRunOutput = powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-run.ps1 -SkipRefresh -ReadOnly -AccountId DU12345 2>&1
$readOnlyRunOutput | ForEach-Object { Write-Host $_ }
Assert-True ($LASTEXITCODE -eq 0) "expected ibkr read-only run to skip paper-run"
$readOnlyRunSummaryPath = ($readOnlyRunOutput | Select-String -Pattern 'summary\s+:\s+(.+summary\.json)' | Select-Object -Last 1).Matches.Groups[1].Value.Trim()
Assert-True (-not [string]::IsNullOrWhiteSpace($readOnlyRunSummaryPath)) "expected ibkr read-only run summary path"
$readOnlyRunSummary = Read-Json $readOnlyRunSummaryPath
Assert-True ($readOnlyRunSummary.status -eq "completed") "expected ibkr read-only run completed status"
Assert-True ($readOnlyRunSummary.failure_class -eq "ok") "expected ibkr read-only run ok failure class"
Assert-True ($readOnlyRunSummary.order_submit -eq "disabled") "expected ibkr read-only order submit disabled"
Assert-True ($readOnlyRunSummary.reconciliation_status -eq "ok") "expected ibkr read-only reconciliation status"
Assert-True ($readOnlyRunSummary.reconciliation_audits -eq 1) "expected ibkr read-only reconciliation audit"
Assert-True ($readOnlyRunSummary.reconciliation_open_order_drifts -eq 0) "expected ibkr read-only zero open order drifts"
Assert-True ($readOnlyRunSummary.reconciliation_execution_drifts -eq 0) "expected ibkr read-only zero execution drifts"
Remove-Item Env:\TRADER_FAIL_ON_PAPER_RUN -ErrorAction SilentlyContinue

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

$env:TRADER_TEST_GATEWAY_PORT = "reachable"
$env:TRADER_FAKE_MODE = "ok"
$previousErrorActionPreference = $ErrorActionPreference
$ErrorActionPreference = "Continue"
try {
    $missingExecutionEvidenceOutput = powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-filled-order-evidence.ps1 -SkipRefresh -ConfirmIbkrPaperOrder -AccountId DU12345 2>&1
} finally {
    $ErrorActionPreference = $previousErrorActionPreference
}
Assert-True ($LASTEXITCODE -ne 0) "expected filled-order evidence failure without broker executions"
$missingExecutionEvidenceSummaryPath = ($missingExecutionEvidenceOutput | Select-String -Pattern 'filled-order evidence summary:\s+(.+filled-order-evidence-summary\.json)' | Select-Object -Last 1).Matches.Groups[1].Value.Trim()
Assert-True (-not [string]::IsNullOrWhiteSpace($missingExecutionEvidenceSummaryPath)) "expected missing execution evidence summary path"
$missingExecutionEvidenceSummary = Read-Json $missingExecutionEvidenceSummaryPath
Assert-True ($missingExecutionEvidenceSummary.status -eq "failed") "expected missing execution evidence failed status"
Assert-True ($missingExecutionEvidenceSummary.failure_class -eq "broker_execution_missing") "expected missing execution failure class"
Assert-True ($missingExecutionEvidenceSummary.broker_executions -eq 0) "expected zero missing execution broker executions"

$env:TRADER_TEST_GATEWAY_PORT = "reachable"
$env:TRADER_FAKE_MODE = "filled_execution"
$filledEvidenceOutput = powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-filled-order-evidence.ps1 -SkipRefresh -ConfirmIbkrPaperOrder -AccountId DU12345 2>&1
$filledEvidenceOutput | ForEach-Object { Write-Host $_ }
Assert-True ($LASTEXITCODE -eq 0) "expected filled-order evidence success with matched broker execution"
$filledEvidenceSummaryPath = ($filledEvidenceOutput | Select-String -Pattern 'filled-order evidence summary:\s+(.+filled-order-evidence-summary\.json)' | Select-Object -Last 1).Matches.Groups[1].Value.Trim()
Assert-True (-not [string]::IsNullOrWhiteSpace($filledEvidenceSummaryPath)) "expected filled execution evidence summary path"
$filledEvidenceSummary = Read-Json $filledEvidenceSummaryPath
Assert-True ($filledEvidenceSummary.status -eq "completed") "expected filled execution evidence completed status"
Assert-True ($filledEvidenceSummary.failure_class -eq "ok") "expected filled execution evidence ok failure class"
Assert-True ($filledEvidenceSummary.broker_executions -eq 1) "expected one broker execution"
Assert-True ($filledEvidenceSummary.matched_executions -eq 1) "expected one matched broker execution"
Assert-True ($filledEvidenceSummary.matched_execution_orders -eq 1) "expected one matched broker order"
Assert-True ($filledEvidenceSummary.max_executions_per_order -eq 1) "expected one execution per order"
Assert-True ($filledEvidenceSummary.execution_field_drifts -eq 0) "expected zero execution field drifts"
Assert-True ($filledEvidenceSummary.local_fills -eq 1) "expected one local fill"
Assert-True ($filledEvidenceSummary.fully_filled_orders -eq 1) "expected one fully filled local order"
Assert-True ($filledEvidenceSummary.partially_filled_orders -eq 0) "expected zero partially filled local orders"
Assert-True ($filledEvidenceSummary.qty_delta -eq 0) "expected zero filled execution qty delta"

$env:TRADER_TEST_GATEWAY_PORT = "reachable"
$env:TRADER_FAKE_MODE = "multiple_executions"
$aggregateEvidenceOutput = powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-filled-order-evidence.ps1 -SkipRefresh -ConfirmIbkrPaperOrder -AccountId DU12345 -MinBrokerExecutions 2 -MinMatchedExecutions 2 -MinExecutionsPerOrder 2 2>&1
$aggregateEvidenceOutput | ForEach-Object { Write-Host $_ }
Assert-True ($LASTEXITCODE -eq 0) "expected multiple execution aggregation evidence success"
$aggregateEvidenceSummaryPath = ($aggregateEvidenceOutput | Select-String -Pattern 'filled-order evidence summary:\s+(.+filled-order-evidence-summary\.json)' | Select-Object -Last 1).Matches.Groups[1].Value.Trim()
$aggregateEvidenceSummary = Read-Json $aggregateEvidenceSummaryPath
Assert-True ($aggregateEvidenceSummary.matched_executions -eq 2) "expected two matched executions"
Assert-True ($aggregateEvidenceSummary.matched_execution_orders -eq 1) "expected executions to match one order"
Assert-True ($aggregateEvidenceSummary.max_executions_per_order -eq 2) "expected two executions aggregated for one order"
Assert-True ($aggregateEvidenceSummary.local_fills -eq 1) "expected one aggregate local fill"
Assert-True ($aggregateEvidenceSummary.qty_delta -eq 0) "expected zero aggregate quantity delta"

$env:TRADER_FAKE_MODE = "filled_execution"
$previousErrorActionPreference = $ErrorActionPreference
$ErrorActionPreference = "Continue"
try {
    $missingAggregationOutput = powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-filled-order-evidence.ps1 -SkipRefresh -ConfirmIbkrPaperOrder -AccountId DU12345 -MinExecutionsPerOrder 2 2>&1
} finally {
    $ErrorActionPreference = $previousErrorActionPreference
}
Assert-True ($LASTEXITCODE -ne 0) "expected aggregation threshold failure for one execution"
$missingAggregationSummaryPath = ($missingAggregationOutput | Select-String -Pattern 'filled-order evidence summary:\s+(.+filled-order-evidence-summary\.json)' | Select-Object -Last 1).Matches.Groups[1].Value.Trim()
$missingAggregationSummary = Read-Json $missingAggregationSummaryPath
Assert-True ($missingAggregationSummary.failure_class -eq "execution_aggregation_missing") "expected execution aggregation failure class"

$env:TRADER_FAKE_MODE = "partial_fill"
$partialEvidenceOutput = powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-filled-order-evidence.ps1 -SkipRefresh -ConfirmIbkrPaperOrder -AccountId DU12345 -MinFullyFilledOrders 0 -MinPartiallyFilledOrders 1 2>&1
$partialEvidenceOutput | ForEach-Object { Write-Host $_ }
Assert-True ($LASTEXITCODE -eq 0) "expected partial fill evidence success with explicit thresholds"
$partialEvidenceSummaryPath = ($partialEvidenceOutput | Select-String -Pattern 'filled-order evidence summary:\s+(.+filled-order-evidence-summary\.json)' | Select-Object -Last 1).Matches.Groups[1].Value.Trim()
$partialEvidenceSummary = Read-Json $partialEvidenceSummaryPath
Assert-True ($partialEvidenceSummary.fully_filled_orders -eq 0) "expected zero full fills"
Assert-True ($partialEvidenceSummary.partially_filled_orders -eq 1) "expected one partial fill"
Assert-True ($partialEvidenceSummary.qty_delta -eq 0) "expected zero partial fill quantity delta"

$previousErrorActionPreference = $ErrorActionPreference
$ErrorActionPreference = "Continue"
try {
    $partialDefaultOutput = powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-filled-order-evidence.ps1 -SkipRefresh -ConfirmIbkrPaperOrder -AccountId DU12345 2>&1
} finally {
    $ErrorActionPreference = $previousErrorActionPreference
}
Assert-True ($LASTEXITCODE -ne 0) "expected partial fill to fail the default full-fill gate"
$partialDefaultSummaryPath = ($partialDefaultOutput | Select-String -Pattern 'filled-order evidence summary:\s+(.+filled-order-evidence-summary\.json)' | Select-Object -Last 1).Matches.Groups[1].Value.Trim()
$partialDefaultSummary = Read-Json $partialDefaultSummaryPath
Assert-True ($partialDefaultSummary.failure_class -eq "full_fill_missing") "expected full fill missing failure class"

$matrixCasesPath = Join-Path $testRoot "filled-order-matrix-cases.json"
@'
[
  {
    "name": "aapl-1d",
    "config": "configs/paper/ibkr_aapl_1d_parquet.toml",
    "input_csv": "datasets/sample/aapl_1d.csv",
    "output_parquet": "datasets/ibkr/aapl_1d.parquet"
  },
  {
    "name": "msft-1d",
    "config": "configs/paper/ibkr_aapl_1d_parquet.toml",
    "input_csv": "datasets/sample/aapl_1d.csv",
    "output_parquet": "datasets/ibkr/aapl_1d.parquet"
  }
]
'@ | Set-Content -Path $matrixCasesPath -Encoding UTF8
$env:TRADER_FAKE_MODE = "filled_execution"
$matrixOutput = powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-filled-order-matrix.ps1 -CasesPath $matrixCasesPath -SkipRefresh -ConfirmIbkrPaperOrder -AccountId DU12345 2>&1
$matrixOutput | ForEach-Object { Write-Host $_ }
Assert-True ($LASTEXITCODE -eq 0) "expected two-case filled-order matrix success"
$matrixSummaryPath = ($matrixOutput | Select-String -Pattern 'IBKR filled-order matrix summary:\s+(.+summary\.json)' | Select-Object -Last 1).Matches.Groups[1].Value.Trim()
$matrixSummary = Read-Json $matrixSummaryPath
Assert-True ($matrixSummary.status -eq "completed") "expected completed filled-order matrix"
Assert-True ($matrixSummary.cases_requested -eq 2) "expected two requested matrix cases"
Assert-True ($matrixSummary.cases_completed -eq 2) "expected two completed matrix cases"
Assert-True ([string]$matrixSummary.cases[0].run_id -like "ibkr-aapl-1d-*") "expected AAPL run label"
Assert-True ([string]$matrixSummary.cases[1].run_id -like "ibkr-msft-1d-*") "expected MSFT run label"

@'
[
  {
    "name": "aapl-1d",
    "config": "configs/paper/ibkr_aapl_1d_parquet.toml",
    "input_csv": "datasets/sample/aapl_1d.csv",
    "output_parquet": "datasets/ibkr/aapl_1d.parquet"
  },
  {
    "name": "msft-1d",
    "config": "configs/paper/ibkr_aapl_1d_parquet.toml",
    "input_csv": "datasets/sample/aapl_1d.csv",
    "output_parquet": "datasets/ibkr/aapl_1d.parquet"
  },
  {
    "name": "tsla-1d",
    "config": "configs/paper/ibkr_aapl_1d_parquet.toml",
    "input_csv": "datasets/sample/aapl_1d.csv",
    "output_parquet": "datasets/ibkr/aapl_1d.parquet"
  }
]
'@ | Set-Content -Path $matrixCasesPath -Encoding UTF8
$env:TRADER_FAKE_MODE = "matrix_second_failure"
$previousErrorActionPreference = $ErrorActionPreference
$ErrorActionPreference = "Continue"
try {
    $failedMatrixOutput = powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-filled-order-matrix.ps1 -CasesPath $matrixCasesPath -SkipRefresh -ConfirmIbkrPaperOrder -AccountId DU12345 2>&1
} finally {
    $ErrorActionPreference = $previousErrorActionPreference
}
Assert-True ($LASTEXITCODE -ne 0) "expected matrix failure on second case"
$failedMatrixSummaryPath = ($failedMatrixOutput | Select-String -Pattern 'IBKR filled-order matrix summary:\s+(.+summary\.json)' | Select-Object -Last 1).Matches.Groups[1].Value.Trim()
$failedMatrixSummary = Read-Json $failedMatrixSummaryPath
Assert-True ($failedMatrixSummary.status -eq "failed") "expected failed matrix status"
Assert-True ($failedMatrixSummary.cases_requested -eq 3) "expected three requested fail-fast cases"
Assert-True ($failedMatrixSummary.cases_completed -eq 2) "expected matrix to stop after second case"
Assert-True ($failedMatrixSummary.failed_case -eq "msft-1d") "expected MSFT matrix case failure"
Assert-True ($failedMatrixSummary.cases[1].failure_class -eq "broker_execution_missing") "expected missing execution case failure"

$env:TRADER_TEST_GATEWAY_PORT = "reachable"
$env:TRADER_FAKE_MODE = "filled_execution_with_external_unmatched"
$externalExecutionEvidenceOutput = powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-filled-order-evidence.ps1 -SkipRefresh -ConfirmIbkrPaperOrder -AccountId DU12345 2>&1
$externalExecutionEvidenceOutput | ForEach-Object { Write-Host $_ }
Assert-True ($LASTEXITCODE -eq 0) "expected filled-order evidence to ignore external unmatched execution drift"
$externalExecutionEvidenceSummaryPath = ($externalExecutionEvidenceOutput | Select-String -Pattern 'filled-order evidence summary:\s+(.+filled-order-evidence-summary\.json)' | Select-Object -Last 1).Matches.Groups[1].Value.Trim()
Assert-True (-not [string]::IsNullOrWhiteSpace($externalExecutionEvidenceSummaryPath)) "expected external execution evidence summary path"
$externalExecutionEvidenceSummary = Read-Json $externalExecutionEvidenceSummaryPath
Assert-True ($externalExecutionEvidenceSummary.status -eq "completed") "expected external execution evidence completed status"
Assert-True ($externalExecutionEvidenceSummary.failure_class -eq "ok") "expected external execution evidence ok failure class"
Assert-True ($externalExecutionEvidenceSummary.broker_executions -eq 2) "expected external execution to remain visible in broker total"
Assert-True ($externalExecutionEvidenceSummary.matched_executions -eq 1) "expected only current run execution to match"
Assert-True ($externalExecutionEvidenceSummary.unmatched_executions -eq 1) "expected external execution to remain visible as unmatched"
Assert-True ($externalExecutionEvidenceSummary.qty_delta -eq 0) "expected external execution not to affect quantity delta"

$env:TRADER_TEST_GATEWAY_PORT = "reachable"
$env:TRADER_FAKE_MODE = "execution_field_drift"
$previousErrorActionPreference = $ErrorActionPreference
$ErrorActionPreference = "Continue"
try {
    $fieldDriftEvidenceOutput = powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-filled-order-evidence.ps1 -SkipRefresh -ConfirmIbkrPaperOrder -AccountId DU12345 2>&1
} finally {
    $ErrorActionPreference = $previousErrorActionPreference
}
Assert-True ($LASTEXITCODE -ne 0) "expected filled-order evidence failure on execution field drift"
$fieldDriftEvidenceSummaryPath = ($fieldDriftEvidenceOutput | Select-String -Pattern 'filled-order evidence summary:\s+(.+filled-order-evidence-summary\.json)' | Select-Object -Last 1).Matches.Groups[1].Value.Trim()
Assert-True (-not [string]::IsNullOrWhiteSpace($fieldDriftEvidenceSummaryPath)) "expected field drift evidence summary path"
$fieldDriftEvidenceSummary = Read-Json $fieldDriftEvidenceSummaryPath
Assert-True ($fieldDriftEvidenceSummary.status -eq "failed") "expected field drift evidence failed status"
Assert-True ($fieldDriftEvidenceSummary.failure_class -eq "execution_field_drift") "expected execution field drift failure class"
Assert-True ($fieldDriftEvidenceSummary.execution_field_drifts -eq 1) "expected one execution field drift"

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
$env:TRADER_FAKE_MODE = "reconciliation_drift"
$previousErrorActionPreference = $ErrorActionPreference
$ErrorActionPreference = "Continue"
try {
    $driftSoakOutput = powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-soak.ps1 -Iterations 1 -SkipRefresh -ConfirmIbkrPaperOrder -AccountId DU12345 2>&1
} finally {
    $ErrorActionPreference = $previousErrorActionPreference
}
Assert-True ($LASTEXITCODE -ne 0) "expected ibkr paper soak failure on reconciliation drift"
$driftSoakSummaryPath = ($driftSoakOutput | Select-String -Pattern 'IBKR paper soak summary:\s+(.+summary\.json)' | Select-Object -Last 1).Matches.Groups[1].Value.Trim()
Assert-True (-not [string]::IsNullOrWhiteSpace($driftSoakSummaryPath)) "expected drift ibkr soak summary path"
$driftSoakSummary = Read-Json $driftSoakSummaryPath
Assert-True ($driftSoakSummary.status -eq "failed") "expected drift ibkr soak failed status"
Assert-True ($driftSoakSummary.failure_class -eq "reconciliation_drift") "expected drift ibkr soak failure class"
Assert-True ($driftSoakSummary.iterations[0].reconciliation_audits -eq 1) "expected failed drift audit counter"
Assert-True ($driftSoakSummary.iterations[0].reconciliation_open_order_drifts -eq 1) "expected failed drift open order counter"
Assert-True (-not [string]::IsNullOrWhiteSpace([string]$driftSoakSummary.iterations[0].summary)) "expected failed drift run summary path"

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
Remove-Item Env:\TRADER_FAIL_ON_PAPER_RUN -ErrorAction SilentlyContinue
Write-Host "IBKR paper script tests passed"
