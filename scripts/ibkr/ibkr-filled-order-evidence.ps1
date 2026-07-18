param(
    [string]$Config = "configs/paper/ibkr_aapl_1d_parquet.toml",
    [string]$InputCsv = "datasets/sample/aapl_1d.csv",
    [string]$OutputParquet = "datasets/ibkr/aapl_1d.parquet",
    [ValidatePattern('^[A-Za-z0-9][A-Za-z0-9._-]*$')]
    [string]$RunLabel = "aapl-1d",
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
    [int]$MinBrokerExecutions = 1,
    [int]$MinMatchedExecutions = 1,
    [int]$MinExecutionsPerOrder = 1,
    [int]$MinLocalFills = 1,
    [int]$MinFullyFilledOrders = 1,
    [int]$MinPartiallyFilledOrders = 0
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

function Get-OutputValue {
    param(
        [string]$Text,
        [string]$Name
    )

    $match = [regex]::Match($Text, "$Name=([^\s]+)")
    if ($match.Success) {
        return $match.Groups[1].Value
    }
    return ""
}

function Get-MatchedExecutionValue {
    param(
        [string]$ReconciliationText,
        [string]$ExecutionsText,
        [string]$MatchedName,
        [string]$LegacyName
    )

    $value = Get-OutputValue -Text $ReconciliationText -Name $MatchedName
    if (-not [string]::IsNullOrWhiteSpace($value) -and $value -ne "none") {
        return $value
    }
    return Get-OutputValue -Text $ExecutionsText -Name $LegacyName
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
if ($MinMatchedExecutions -lt 1) {
    throw "-MinMatchedExecutions must be at least 1"
}
if ($MinExecutionsPerOrder -lt 1) {
    throw "-MinExecutionsPerOrder must be at least 1"
}
if ($MinLocalFills -lt 1) {
    throw "-MinLocalFills must be at least 1"
}
if ($MinFullyFilledOrders -lt 0) {
    throw "-MinFullyFilledOrders cannot be negative"
}
if ($MinPartiallyFilledOrders -lt 0) {
    throw "-MinPartiallyFilledOrders cannot be negative"
}

$repoRoot = Get-Location
$runArgs = @(
    "-ExecutionPolicy", "Bypass",
    "-File", ".\scripts\ibkr\ibkr-paper-run.ps1",
    "-Config", $Config,
    "-InputCsv", $InputCsv,
    "-OutputParquet", $OutputParquet,
    "-RunLabel", $RunLabel,
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
$matchedExecutionOrders = Get-OutputInt -Text $reconciliationOutput -Name "remote_execution_matched_orders"
$maxExecutionsPerOrder = Get-OutputInt -Text $reconciliationOutput -Name "remote_execution_max_per_order"
$unmatchedExecutions = Get-OutputInt -Text $reconciliationOutput -Name "remote_execution_unmatched"
$executionFieldDrifts = [Math]::Max(
    (Get-OutputInt -Text $reconciliationOutput -Name "remote_execution_field_drifts"),
    [int]$runSummary.reconciliation_execution_field_drifts
)
$localFills = Get-OutputInt -Text $reconciliationOutput -Name "local_fills"
$fullyFilledOrders = Get-OutputInt -Text $reconciliationOutput -Name "local_fully_filled_orders"
$partiallyFilledOrders = Get-OutputInt -Text $reconciliationOutput -Name "local_partially_filled_orders"
$qtyDelta = Get-OutputDecimal -Text $reconciliationOutput -Name "qty_delta"
$executionOrderId = Get-MatchedExecutionValue `
    -ReconciliationText $reconciliationOutput `
    -ExecutionsText $executionsOutput `
    -MatchedName "remote_execution_order_ids" `
    -LegacyName "order_id"
$executionClientOrderId = Get-MatchedExecutionValue `
    -ReconciliationText $reconciliationOutput `
    -ExecutionsText $executionsOutput `
    -MatchedName "remote_execution_client_order_ids" `
    -LegacyName "client_order_id"
$executionTradeId = Get-MatchedExecutionValue `
    -ReconciliationText $reconciliationOutput `
    -ExecutionsText $executionsOutput `
    -MatchedName "remote_execution_trade_ids" `
    -LegacyName "trade_id"

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
} elseif ($matchedExecutions -lt $MinMatchedExecutions) {
    $failureClass = "execution_match_missing"
} elseif ($maxExecutionsPerOrder -lt $MinExecutionsPerOrder) {
    $failureClass = "execution_aggregation_missing"
} elseif ($localFills -lt $MinLocalFills) {
    $failureClass = "local_fill_missing"
} elseif ($fullyFilledOrders -lt $MinFullyFilledOrders) {
    $failureClass = "full_fill_missing"
} elseif ($partiallyFilledOrders -lt $MinPartiallyFilledOrders) {
    $failureClass = "partial_fill_missing"
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
    matched_execution_orders = $matchedExecutionOrders
    max_executions_per_order = $maxExecutionsPerOrder
    unmatched_executions = $unmatchedExecutions
    execution_field_drifts = $executionFieldDrifts
    local_fills = $localFills
    fully_filled_orders = $fullyFilledOrders
    partially_filled_orders = $partiallyFilledOrders
    qty_delta = $qtyDelta
    execution_order_id = $executionOrderId
    execution_client_order_id = $executionClientOrderId
    execution_trade_id = $executionTradeId
    min_broker_executions = $MinBrokerExecutions
    min_matched_executions = $MinMatchedExecutions
    min_executions_per_order = $MinExecutionsPerOrder
    min_local_fills = $MinLocalFills
    min_fully_filled_orders = $MinFullyFilledOrders
    min_partially_filled_orders = $MinPartiallyFilledOrders
    gateway_executions_output = $executionsOutput
    reconciliation_output = $reconciliationOutput
}
Write-FilledEvidenceSummary -Path $evidencePath -Summary $evidence

if ($failureClass -ne "ok") {
    throw "IBKR filled-order evidence failed: $failureClass; see $evidencePath"
}

$evidence
