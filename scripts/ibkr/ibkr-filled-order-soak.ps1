param(
    [Parameter(Mandatory = $true)]
    [string]$CasesPath,
    [int]$Iterations = 3,
    [int]$DelaySeconds = 0,
    [switch]$SkipRefresh,
    [switch]$ConfirmIbkrPaperOrder,
    [string]$AccountId = "",
    [string]$GatewayHost = "",
    [int]$Port = 0,
    [int]$ClientId = 0,
    [string]$IbkrRouteExchange = "",
    [switch]$IbkrOverridePercentageConstraints,
    [int]$OpenOrdersSettleSeconds = 30,
    [int]$OpenOrdersPollSeconds = 2
)

$ErrorActionPreference = "Stop"

function Get-MatrixSummaryPath {
    param([object[]]$Output)

    $match = $Output |
        Select-String -Pattern 'IBKR filled-order matrix summary:\s+(.+summary\.json)' |
        Select-Object -Last 1
    if ($null -eq $match) {
        return ""
    }
    return $match.Matches.Groups[1].Value.Trim()
}

function Read-Json {
    param([string]$Path)

    if ([string]::IsNullOrWhiteSpace($Path) -or -not (Test-Path -LiteralPath $Path)) {
        return $null
    }
    return Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
}

if (-not $ConfirmIbkrPaperOrder) {
    throw "IBKR filled-order soak requires -ConfirmIbkrPaperOrder because every iteration may submit paper orders"
}
if ($Iterations -lt 1) {
    throw "-Iterations must be at least 1"
}
if ($DelaySeconds -lt 0) {
    throw "-DelaySeconds cannot be negative"
}
if (-not (Test-Path -LiteralPath $CasesPath)) {
    throw "IBKR filled-order soak cases file not found: $CasesPath"
}

$repoRoot = Get-Location
$soakId = "ibkr-filled-order-soak-$(([guid]::NewGuid().ToString('N')).Substring(0, 12))"
$soakDir = Join-Path $repoRoot "data/ibkr/filled-order-soak/$soakId"
$summaryPath = Join-Path $soakDir "summary.json"
$iterationResults = @()
$seenRunIds = [System.Collections.Generic.HashSet[string]]::new(
    [System.StringComparer]::Ordinal
)
$seenClientOrderIds = [System.Collections.Generic.HashSet[string]]::new(
    [System.StringComparer]::Ordinal
)
$failedIteration = 0
$failureClass = "ok"
$totalCases = 0
$totalMatchedExecutions = 0
$totalLocalFills = 0
$totalOpenOrdersRemaining = 0

New-Item -ItemType Directory -Force -Path $soakDir | Out-Null

for ($iteration = 1; $iteration -le $Iterations; $iteration++) {
    $logPath = Join-Path $soakDir "iteration-$iteration.log"
    $matrixArgs = @(
        "-ExecutionPolicy", "Bypass",
        "-File", ".\scripts\ibkr\ibkr-filled-order-matrix.ps1",
        "-CasesPath", $CasesPath,
        "-ConfirmIbkrPaperOrder",
        "-OpenOrdersSettleSeconds", $OpenOrdersSettleSeconds.ToString(),
        "-OpenOrdersPollSeconds", $OpenOrdersPollSeconds.ToString()
    )
    if ($SkipRefresh) {
        $matrixArgs += "-SkipRefresh"
    }
    if ($AccountId.Trim().Length -gt 0) {
        $matrixArgs += @("-AccountId", $AccountId)
    }
    if ($GatewayHost.Trim().Length -gt 0) {
        $matrixArgs += @("-GatewayHost", $GatewayHost)
    }
    if ($Port -gt 0) {
        $matrixArgs += @("-Port", $Port.ToString())
    }
    if ($ClientId -gt 0) {
        $matrixArgs += @("-ClientId", $ClientId.ToString())
    }
    if ($IbkrRouteExchange.Trim().Length -gt 0) {
        $matrixArgs += @("-IbkrRouteExchange", $IbkrRouteExchange)
    }
    if ($IbkrOverridePercentageConstraints) {
        $matrixArgs += "-IbkrOverridePercentageConstraints"
    }

    Write-Host "Running IBKR filled-order soak iteration $iteration/$Iterations"
    $previousErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        $global:LASTEXITCODE = 0
        $matrixOutput = powershell @matrixArgs 2>&1
        $matrixExitCode = if ($null -eq $LASTEXITCODE) { 0 } else { $LASTEXITCODE }
    } finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }
    $matrixOutput | Set-Content -Path $logPath -Encoding UTF8
    $matrixOutput | ForEach-Object { Write-Host $_ }

    $matrixSummaryPath = Get-MatrixSummaryPath -Output $matrixOutput
    $matrixSummary = Read-Json -Path $matrixSummaryPath
    $iterationFailureClass = "ok"
    if ($null -eq $matrixSummary) {
        $iterationFailureClass = "matrix_summary_missing"
    } elseif ($matrixExitCode -ne 0 -or [string]$matrixSummary.status -ne "completed") {
        $iterationFailureClass = "matrix_failed"
    }

    $runIds = @()
    $clientOrderIds = @()
    $iterationMatchedExecutions = 0
    $iterationLocalFills = 0
    $iterationOpenOrdersRemaining = 0
    if ($null -ne $matrixSummary) {
        foreach ($case in @($matrixSummary.cases)) {
            $evidence = Read-Json -Path ([string]$case.evidence_summary)
            if ($null -eq $evidence) {
                if ($iterationFailureClass -eq "ok") {
                    $iterationFailureClass = "evidence_summary_missing"
                }
                continue
            }

            $runId = [string]$evidence.run_id
            $clientOrderId = [string]$evidence.execution_client_order_id
            $runIds += $runId
            if (-not [string]::IsNullOrWhiteSpace($clientOrderId) -and $clientOrderId -ne "none") {
                $clientOrderIds += $clientOrderId
            }
            $iterationMatchedExecutions += [int]$evidence.matched_executions
            $iterationLocalFills += [int]$evidence.local_fills
            $iterationOpenOrdersRemaining += [int]$evidence.open_orders_remaining

            if ([string]::IsNullOrWhiteSpace($runId) -or -not $seenRunIds.Add($runId)) {
                if ($iterationFailureClass -eq "ok") {
                    $iterationFailureClass = "duplicate_run_id"
                }
            }
            if (
                -not [string]::IsNullOrWhiteSpace($clientOrderId) -and
                $clientOrderId -ne "none" -and
                -not $seenClientOrderIds.Add($clientOrderId)
            ) {
                if ($iterationFailureClass -eq "ok") {
                    $iterationFailureClass = "duplicate_client_order_id"
                }
            }
            if ([int]$evidence.open_orders_remaining -ne 0 -and $iterationFailureClass -eq "ok") {
                $iterationFailureClass = "open_orders_remaining"
            }
        }
    }

    $caseCount = if ($null -ne $matrixSummary) { @($matrixSummary.cases).Count } else { 0 }
    $totalCases += $caseCount
    $totalMatchedExecutions += $iterationMatchedExecutions
    $totalLocalFills += $iterationLocalFills
    $totalOpenOrdersRemaining += $iterationOpenOrdersRemaining
    $iterationResults += [pscustomobject]@{
        iteration = $iteration
        status = if ($iterationFailureClass -eq "ok") { "completed" } else { "failed" }
        failure_class = $iterationFailureClass
        exit_code = $matrixExitCode
        log = $logPath
        matrix_summary = $matrixSummaryPath
        matrix_id = if ($null -ne $matrixSummary) { $matrixSummary.matrix_id } else { $null }
        cases_completed = $caseCount
        matched_executions = $iterationMatchedExecutions
        local_fills = $iterationLocalFills
        open_orders_remaining = $iterationOpenOrdersRemaining
        run_ids = $runIds
        execution_client_order_ids = $clientOrderIds
    }

    if ($iterationFailureClass -ne "ok") {
        $failedIteration = $iteration
        $failureClass = $iterationFailureClass
        break
    }
    if ($iteration -lt $Iterations -and $DelaySeconds -gt 0) {
        Start-Sleep -Seconds $DelaySeconds
    }
}

$summary = [pscustomobject]@{
    soak_id = $soakId
    status = if ($failureClass -eq "ok") { "completed" } else { "failed" }
    failure_class = $failureClass
    iterations_requested = $Iterations
    iterations_completed = $iterationResults.Count
    failed_iteration = $failedIteration
    cases_completed = $totalCases
    unique_run_ids = $seenRunIds.Count
    unique_execution_client_order_ids = $seenClientOrderIds.Count
    matched_executions = $totalMatchedExecutions
    local_fills = $totalLocalFills
    open_orders_remaining = $totalOpenOrdersRemaining
    iterations = $iterationResults
}
$summary | ConvertTo-Json -Depth 10 | Set-Content -Path $summaryPath -Encoding UTF8
Write-Host "IBKR filled-order soak summary: $summaryPath"

if ($failureClass -ne "ok") {
    throw "IBKR filled-order soak failed: $failureClass; see $summaryPath"
}

$summary
