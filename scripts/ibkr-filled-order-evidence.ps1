param(
    [string]$Config = "configs/paper/ibkr_aapl_1d_parquet.toml",
    [string]$InputCsv = "datasets/sample/aapl_1d.csv",
    [string]$OutputParquet = "datasets/ibkr/aapl_1d.parquet",
    [switch]$SkipRefresh,
    [switch]$ConfirmIbkrPaperOrder,
    [string]$AccountId = "",
    [string]$GatewayHost = "",
    [int]$Port = 0,
    [int]$ClientId = 0,
    [string]$IbkrRouteExchange = "",
    [switch]$IbkrOverridePercentageConstraints,
    [int]$OpenOrdersSettleSeconds = 30,
    [int]$OpenOrdersPollSeconds = 2,
    [int]$MinBrokerExecutions = 1
)

$ErrorActionPreference = "Stop"

function Get-OutputInt {
    param(
        [string]$Text,
        [string]$Name
    )

    $match = [regex]::Match($Text, "$Name=(-?\d+)")
    if ($match.Success) {
        return [int]$match.Groups[1].Value
    }
    return 0
}

function Get-OutputDecimal {
    param(
        [string]$Text,
        [string]$Name
    )

    $match = [regex]::Match($Text, "$Name=([-+]?\d+(?:\.\d+)?)")
    if ($match.Success) {
        return [decimal]$match.Groups[1].Value
    }
    return [decimal]0
}

function Get-SummaryPath {
    param([object[]]$Output)

    $match = ($Output | Select-String -Pattern 'summary\s+:\s+(.+summary\.json)' | Select-Object -Last 1)
    if ($null -eq $match) {
        return ""
    }
    return $match.Matches.Groups[1].Value.Trim()
}

function Write-FilledEvidenceSummary {
    param(
        [string]$Path,
        [object]$Summary
    )

    $Summary | ConvertTo-Json -Depth 8 | Set-Content -Path $Path -Encoding UTF8
    Write-Host "filled-order evidence summary: $Path"
}

if (-not $ConfirmIbkrPaperOrder) {
    throw "IBKR filled-order evidence requires -ConfirmIbkrPaperOrder because it must submit a paper order and observe a broker execution"
}
if ($MinBrokerExecutions -lt 1) {
    throw "-MinBrokerExecutions must be at least 1"
}

$repoRoot = Get-Location
$runArgs = @(
    "-ExecutionPolicy", "Bypass",
    "-File", ".\scripts\ibkr-paper-run.ps1",
    "-Config", $Config,
    "-InputCsv", $InputCsv,
    "-OutputParquet", $OutputParquet,
    "-ConfirmIbkrPaperOrder",
    "-OpenOrdersSettleSeconds", $OpenOrdersSettleSeconds.ToString(),
    "-OpenOrdersPollSeconds", $OpenOrdersPollSeconds.ToString()
)
if ($SkipRefresh) {
    $runArgs += "-SkipRefresh"
}
if ($AccountId.Trim().Length -gt 0) {
    $runArgs += @("-AccountId", $AccountId)
}
if ($GatewayHost.Trim().Length -gt 0) {
    $runArgs += @("-GatewayHost", $GatewayHost)
}
if ($Port -gt 0) {
    $runArgs += @("-Port", $Port.ToString())
}
if ($ClientId -gt 0) {
    $runArgs += @("-ClientId", $ClientId.ToString())
}
if ($IbkrRouteExchange.Trim().Length -gt 0) {
    $runArgs += @("-IbkrRouteExchange", $IbkrRouteExchange)
}
if ($IbkrOverridePercentageConstraints) {
    $runArgs += "-IbkrOverridePercentageConstraints"
}

$previousErrorActionPreference = $ErrorActionPreference
$ErrorActionPreference = "Continue"
try {
    $global:LASTEXITCODE = 0
    $runOutput = powershell @runArgs 2>&1
    $runExitCode = if ($null -eq $LASTEXITCODE) { 0 } else { $LASTEXITCODE }
} finally {
    $ErrorActionPreference = $previousErrorActionPreference
}
$runOutput | ForEach-Object { Write-Host $_ }

$summaryPath = Get-SummaryPath $runOutput
if ([string]::IsNullOrWhiteSpace($summaryPath) -or -not (Test-Path $summaryPath)) {
    throw "IBKR filled-order evidence failed: child run did not produce a summary.json"
}

$runSummary = Get-Content -Path $summaryPath -Raw | ConvertFrom-Json
$runDir = Split-Path $summaryPath -Parent
$evidencePath = Join-Path $runDir "filled-order-evidence-summary.json"
$executionsOutput = [string]$runSummary.gateway_checks.executions
$reconciliationOutput = [string]$runSummary.gateway_checks.reconciliation
$brokerExecutions = [Math]::Max(
    (Get-OutputInt -Text $executionsOutput -Name "executions"),
    (Get-OutputInt -Text $reconciliationOutput -Name "remote_executions")
)
$matchedExecutions = Get-OutputInt -Text $reconciliationOutput -Name "remote_execution_matched"
$executionFieldDrifts = [Math]::Max(
    (Get-OutputInt -Text $reconciliationOutput -Name "remote_execution_field_drifts"),
    [int]$runSummary.reconciliation_execution_field_drifts
)
$localFills = Get-OutputInt -Text $reconciliationOutput -Name "local_fills"
$qtyDelta = Get-OutputDecimal -Text $reconciliationOutput -Name "qty_delta"

$failureClass = "ok"
if ($executionFieldDrifts -ne 0) {
    $failureClass = "execution_field_drift"
} elseif ($runExitCode -ne 0) {
    $failureClass = "paper_run_failed"
} elseif ($runSummary.status -ne "completed") {
    $failureClass = "paper_run_not_completed"
} elseif ($runSummary.order_submit -ne "enabled") {
    $failureClass = "order_submit_disabled"
} elseif ([int]$runSummary.open_orders_remaining -ne 0) {
    $failureClass = "open_orders_remaining"
} elseif ($runSummary.reconciliation_status -ne "ok") {
    $failureClass = "reconciliation_not_ok"
} elseif ([int]$runSummary.reconciliation_open_order_drifts -ne 0 -or [int]$runSummary.reconciliation_execution_drifts -ne 0) {
    $failureClass = "reconciliation_drift"
} elseif ($brokerExecutions -lt $MinBrokerExecutions) {
    $failureClass = "broker_execution_missing"
} elseif ($matchedExecutions -lt $MinBrokerExecutions) {
    $failureClass = "execution_match_missing"
} elseif ($localFills -lt $MinBrokerExecutions) {
    $failureClass = "local_fill_missing"
} elseif ($qtyDelta -ne [decimal]0) {
    $failureClass = "execution_qty_delta"
}

$evidence = [pscustomobject]@{
    status = if ($failureClass -eq "ok") { "completed" } else { "failed" }
    failure_class = $failureClass
    source_summary = $summaryPath
    run_id = $runSummary.run_id
    account_id = $runSummary.account_id
    order_submit = $runSummary.order_submit
    open_orders_remaining = $runSummary.open_orders_remaining
    reconciliation_status = $runSummary.reconciliation_status
    reconciliation_open_order_drifts = $runSummary.reconciliation_open_order_drifts
    reconciliation_execution_drifts = $runSummary.reconciliation_execution_drifts
    broker_executions = $brokerExecutions
    matched_executions = $matchedExecutions
    execution_field_drifts = $executionFieldDrifts
    local_fills = $localFills
    qty_delta = $qtyDelta
    min_broker_executions = $MinBrokerExecutions
    gateway_executions_output = $executionsOutput
    reconciliation_output = $reconciliationOutput
}
Write-FilledEvidenceSummary -Path $evidencePath -Summary $evidence

if ($failureClass -ne "ok") {
    throw "IBKR filled-order evidence failed: $failureClass; see $evidencePath"
}

$evidence
