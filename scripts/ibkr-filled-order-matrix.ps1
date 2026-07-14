param(
    [Parameter(Mandatory = $true)]
    [string]$CasesPath,
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

function Get-CaseValue {
    param(
        [object]$Case,
        [string]$Name,
        [object]$DefaultValue
    )

    if ($Case.PSObject.Properties.Name -contains $Name -and $null -ne $Case.$Name) {
        return $Case.$Name
    }
    return $DefaultValue
}

function Get-EvidencePath {
    param([object[]]$Output)

    $match = $Output |
        Select-String -Pattern 'filled-order evidence summary:\s+(.+filled-order-evidence-summary\.json)' |
        Select-Object -Last 1
    if ($null -eq $match) {
        return ""
    }
    return $match.Matches.Groups[1].Value.Trim()
}

if (-not $ConfirmIbkrPaperOrder) {
    throw "IBKR filled-order matrix requires -ConfirmIbkrPaperOrder because every case may submit a paper order"
}
if (-not (Test-Path -LiteralPath $CasesPath)) {
    throw "IBKR filled-order matrix cases file not found: $CasesPath"
}

$repoRoot = Get-Location
$matrixId = "ibkr-filled-order-matrix-$(([guid]::NewGuid().ToString('N')).Substring(0, 12))"
$matrixDir = Join-Path $repoRoot "data/ibkr-filled-order-matrix/$matrixId"
$summaryPath = Join-Path $matrixDir "summary.json"
$parsedCases = Get-Content -LiteralPath $CasesPath -Raw | ConvertFrom-Json
$cases = @()
foreach ($parsedCase in $parsedCases) {
    $cases += $parsedCase
}
if ($cases.Count -eq 0) {
    throw "IBKR filled-order matrix requires at least one case"
}

New-Item -ItemType Directory -Force -Path $matrixDir | Out-Null
$results = @()
$failedCase = $null

foreach ($case in $cases) {
    $name = [string](Get-CaseValue -Case $case -Name "name" -DefaultValue "")
    $config = [string](Get-CaseValue -Case $case -Name "config" -DefaultValue "")
    $inputCsv = [string](Get-CaseValue -Case $case -Name "input_csv" -DefaultValue "")
    $outputParquet = [string](Get-CaseValue -Case $case -Name "output_parquet" -DefaultValue "")
    if ($name -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]*$') {
        throw "IBKR filled-order matrix case name must be a path-safe run label: $name"
    }
    if ([string]::IsNullOrWhiteSpace($config) -or [string]::IsNullOrWhiteSpace($inputCsv) -or [string]::IsNullOrWhiteSpace($outputParquet)) {
        throw "IBKR filled-order matrix case '$name' requires config, input_csv, and output_parquet"
    }

    $evidenceArgs = @(
        "-ExecutionPolicy", "Bypass",
        "-File", ".\scripts\ibkr-filled-order-evidence.ps1",
        "-Config", $config,
        "-InputCsv", $inputCsv,
        "-OutputParquet", $outputParquet,
        "-RunLabel", $name,
        "-ConfirmIbkrPaperOrder",
        "-OpenOrdersSettleSeconds", $OpenOrdersSettleSeconds.ToString(),
        "-OpenOrdersPollSeconds", $OpenOrdersPollSeconds.ToString(),
        "-MinBrokerExecutions", ([int](Get-CaseValue $case "min_broker_executions" 1)).ToString(),
        "-MinMatchedExecutions", ([int](Get-CaseValue $case "min_matched_executions" 1)).ToString(),
        "-MinExecutionsPerOrder", ([int](Get-CaseValue $case "min_executions_per_order" 1)).ToString(),
        "-MinLocalFills", ([int](Get-CaseValue $case "min_local_fills" 1)).ToString(),
        "-MinFullyFilledOrders", ([int](Get-CaseValue $case "min_fully_filled_orders" 1)).ToString(),
        "-MinPartiallyFilledOrders", ([int](Get-CaseValue $case "min_partially_filled_orders" 0)).ToString()
    )
    if ($SkipRefresh) {
        $evidenceArgs += "-SkipRefresh"
    }
    if ($AccountId.Trim().Length -gt 0) {
        $evidenceArgs += @("-AccountId", $AccountId)
    }
    if ($GatewayHost.Trim().Length -gt 0) {
        $evidenceArgs += @("-GatewayHost", $GatewayHost)
    }
    if ($Port -gt 0) {
        $evidenceArgs += @("-Port", $Port.ToString())
    }
    if ($ClientId -gt 0) {
        $evidenceArgs += @("-ClientId", $ClientId.ToString())
    }
    if ($IbkrRouteExchange.Trim().Length -gt 0) {
        $evidenceArgs += @("-IbkrRouteExchange", $IbkrRouteExchange)
    }
    if ($IbkrOverridePercentageConstraints) {
        $evidenceArgs += "-IbkrOverridePercentageConstraints"
    }

    Write-Host "Running IBKR filled-order matrix case: $name"
    $previousErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        $global:LASTEXITCODE = 0
        $caseOutput = powershell @evidenceArgs 2>&1
        $caseExitCode = if ($null -eq $LASTEXITCODE) { 0 } else { $LASTEXITCODE }
    } finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }
    $caseOutput | ForEach-Object { Write-Host $_ }

    $evidencePath = Get-EvidencePath $caseOutput
    $evidence = if (-not [string]::IsNullOrWhiteSpace($evidencePath) -and (Test-Path -LiteralPath $evidencePath)) {
        Get-Content -LiteralPath $evidencePath -Raw | ConvertFrom-Json
    } else {
        $null
    }
    $failureClass = if ($null -ne $evidence) {
        [string]$evidence.failure_class
    } elseif ($caseExitCode -ne 0) {
        "evidence_command_failed"
    } else {
        "evidence_summary_missing"
    }
    $result = [pscustomobject]@{
        name = $name
        status = if ($caseExitCode -eq 0 -and $failureClass -eq "ok") { "completed" } else { "failed" }
        failure_class = $failureClass
        exit_code = $caseExitCode
        evidence_summary = $evidencePath
        run_id = if ($null -ne $evidence) { $evidence.run_id } else { $null }
    }
    $results += $result
    if ($result.status -ne "completed") {
        $failedCase = $name
        break
    }
}

$summary = [pscustomobject]@{
    matrix_id = $matrixId
    status = if ($null -eq $failedCase) { "completed" } else { "failed" }
    failure_class = if ($null -eq $failedCase) { "ok" } else { "case_failed" }
    cases_requested = $cases.Count
    cases_completed = $results.Count
    failed_case = $failedCase
    cases = $results
}
$summary | ConvertTo-Json -Depth 8 | Set-Content -Path $summaryPath -Encoding UTF8
Write-Host "IBKR filled-order matrix summary: $summaryPath"

if ($null -ne $failedCase) {
    throw "IBKR filled-order matrix failed at case '$failedCase'; see $summaryPath"
}

$summary
