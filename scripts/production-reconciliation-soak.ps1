param(
    [ValidateSet("ibkr")]
    [string]$Broker = "ibkr",
    [int]$Iterations = 6,
    [int]$DelaySeconds = 10,
    [switch]$ReadOnly,
    [string]$AccountId = "",
    [string]$GatewayHost = "127.0.0.1",
    [int]$Port = 7497,
    [int]$ClientId = 1
)

$ErrorActionPreference = "Stop"

if ($Iterations -lt 1) {
    throw "Iterations must be at least 1"
}
if ($Broker -eq "ibkr" -and $AccountId.Trim().Length -eq 0) {
    throw "IBKR production reconciliation soak requires -AccountId DU..."
}

$repoRoot = Get-Location
$id = [guid]::NewGuid().ToString("N")
$soakId = "production-reconciliation-$Broker-$($id.Substring(0, 12))"
$soakDir = Join-Path $repoRoot "data/production-reconciliation/$soakId"
$summaryPath = Join-Path $soakDir "summary.json"
New-Item -ItemType Directory -Force -Path $soakDir | Out-Null

$iterationResults = @()
$failed = $false
$failureClass = "ok"
$totalReconciliationAudits = 0
$totalCashDrifts = 0
$totalPositionDrifts = 0
$totalOpenOrderDrifts = 0
$totalExecutionDrifts = 0
$totalStaleInputs = 0

for ($iteration = 1; $iteration -le $Iterations; $iteration++) {
    $iterationLog = Join-Path $soakDir "iteration-$iteration.log"
    $args = @(
        "-ExecutionPolicy", "Bypass",
        "-File", ".\scripts\ibkr-paper-soak.ps1",
        "-Iterations", "1",
        "-SkipRefresh",
        "-AccountId", $AccountId,
        "-GatewayHost", $GatewayHost,
        "-Port", "$Port",
        "-ClientId", "$ClientId"
    )
    if (-not $ReadOnly) {
        $args += "-ConfirmIbkrPaperOrder"
    } else {
        $args += "-ReadOnly"
    }

    Write-Host "Production reconciliation soak $soakId iteration $iteration/$Iterations"
    $previousErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        $output = powershell @args 2>&1
        $exitCode = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }
    $text = $output -join [Environment]::NewLine
    $text | Set-Content -Path $iterationLog -Encoding UTF8
    $output | ForEach-Object { Write-Host $_ }

    $iterationStatus = if ($exitCode -eq 0) { "completed" } else { "failed" }
    $iterationFailureClass = if ($exitCode -eq 0) { "ok" } else { "iteration_failed" }
    $childSummaryPath = ""
    $childSummary = $null
    $childIteration = $null
    $childSummaryMatch = [regex]::Match($text, "IBKR paper soak summary:\s+(.+summary\.json)")
    if ($childSummaryMatch.Success) {
        $childSummaryPath = $childSummaryMatch.Groups[1].Value.Trim()
        if (Test-Path $childSummaryPath) {
            $childSummary = Get-Content -Path $childSummaryPath -Raw | ConvertFrom-Json
            $childIteration = @($childSummary.iterations | Select-Object -First 1)
        }
    }
    if ($text -match "gateway_unreachable") { $iterationFailureClass = "gateway_unreachable" }
    if ($text -match "account_mismatch") { $iterationFailureClass = "account_mismatch" }
    if ($text -match "reconciliation_drift") { $iterationFailureClass = "reconciliation_drift" }
    if ($ReadOnly -and $exitCode -eq 0 -and $null -ne $childIteration -and $childIteration.reconciliation_status -ne "ok") {
        $iterationFailureClass = "reconciliation_not_run"
    }

    $reconciliationAudits = if ($null -ne $childIteration -and $null -ne $childIteration.reconciliation_audits) { [int]$childIteration.reconciliation_audits } else { 0 }
    $cashDrifts = if ($null -ne $childIteration -and $null -ne $childIteration.reconciliation_cash_drifts) { [int]$childIteration.reconciliation_cash_drifts } else { 0 }
    $positionDrifts = if ($null -ne $childIteration -and $null -ne $childIteration.reconciliation_position_drifts) { [int]$childIteration.reconciliation_position_drifts } else { 0 }
    $openOrderDrifts = if ($null -ne $childIteration -and $null -ne $childIteration.reconciliation_open_order_drifts) { [int]$childIteration.reconciliation_open_order_drifts } else { 0 }
    $executionDrifts = if ($null -ne $childIteration -and $null -ne $childIteration.reconciliation_execution_drifts) { [int]$childIteration.reconciliation_execution_drifts } else { 0 }
    $staleInputs = if ($null -ne $childIteration -and $null -ne $childIteration.reconciliation_stale_inputs) { [int]$childIteration.reconciliation_stale_inputs } else { 0 }
    $totalReconciliationAudits += $reconciliationAudits
    $totalCashDrifts += $cashDrifts
    $totalPositionDrifts += $positionDrifts
    $totalOpenOrderDrifts += $openOrderDrifts
    $totalExecutionDrifts += $executionDrifts
    $totalStaleInputs += $staleInputs
    $iterationStatus = if ($iterationFailureClass -eq "ok") { "completed" } else { "failed" }

    $iterationResults += [pscustomobject]@{
        iteration = $iteration
        exit_code = $exitCode
        status = $iterationStatus
        failure_class = $iterationFailureClass
        log = $iterationLog
        child_summary = $childSummaryPath
        reconciliation_status = if ($null -ne $childIteration) { [string]$childIteration.reconciliation_status } else { "" }
        reconciliation_audits = $reconciliationAudits
        reconciliation_cash_drifts = $cashDrifts
        reconciliation_position_drifts = $positionDrifts
        reconciliation_open_order_drifts = $openOrderDrifts
        reconciliation_execution_drifts = $executionDrifts
        reconciliation_stale_inputs = $staleInputs
    }

    if ($iterationFailureClass -ne "ok") {
        $failed = $true
        $failureClass = $iterationFailureClass
        break
    }

    if ($iteration -lt $Iterations -and $DelaySeconds -gt 0) {
        Start-Sleep -Seconds $DelaySeconds
    }
}

$summary = [pscustomobject]@{
    soak_id = $soakId
    broker = $Broker
    read_only = [bool]$ReadOnly
    account_id = $AccountId
    iterations_requested = $Iterations
    iterations_completed = $iterationResults.Count
    status = if ($failed) { "failed" } else { "completed" }
    failure_class = $failureClass
    reconciliation_audits = $totalReconciliationAudits
    reconciliation_cash_drifts = $totalCashDrifts
    reconciliation_position_drifts = $totalPositionDrifts
    reconciliation_open_order_drifts = $totalOpenOrderDrifts
    reconciliation_execution_drifts = $totalExecutionDrifts
    reconciliation_stale_inputs = $totalStaleInputs
    evidence_dir = $soakDir
    iterations = $iterationResults
}
$summary | ConvertTo-Json -Depth 6 | Set-Content -Path $summaryPath -Encoding UTF8
Write-Host "Production reconciliation soak summary: $summaryPath"

if ($failed) {
    throw "Production reconciliation soak failed; see $summaryPath"
}

$summary
