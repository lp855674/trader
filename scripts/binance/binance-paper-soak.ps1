param(
    [int]$Iterations = 3,
    [int]$Limit = 100,
    [int]$DelaySeconds = 0,
    [switch]$SkipRefresh,
    [switch]$ConfirmTestnetOrder
)

$ErrorActionPreference = "Stop"

if ($Iterations -lt 1) {
    throw "Iterations must be at least 1"
}

if ($Limit -lt 1 -or $Limit -gt 1000) {
    throw "Limit must be between 1 and 1000"
}

if ($ConfirmTestnetOrder -and $SkipRefresh) {
    throw "ConfirmTestnetOrder requires fresh Binance kline refresh; remove -SkipRefresh"
}

$repoRoot = Get-Location
$id = [guid]::NewGuid().ToString("N")
$soakId = "binance-paper-soak-$($id.Substring(0, 12))"
$soakDir = Join-Path $repoRoot "data/binance/paper-soak/$soakId"
$soakSummaryPath = Join-Path $soakDir "summary.json"
$iterationSummaries = @()
$failed = $false
$failureClass = "ok"
$failedIteration = 0
$firstFailedLog = ""

function Get-MatchValue {
    param(
        [string]$Text,
        [string]$Pattern
    )

    $match = [regex]::Match($Text, $Pattern, [System.Text.RegularExpressions.RegexOptions]::Multiline)
    if ($match.Success) {
        return $match.Groups[1].Value.Trim()
    }
    return ""
}

function Read-Json {
    param([string]$Path)

    if ([string]::IsNullOrWhiteSpace($Path) -or -not (Test-Path $Path)) {
        return $null
    }
    return Get-Content -Path $Path -Raw | ConvertFrom-Json
}

try {
    New-Item -ItemType Directory -Force -Path $soakDir | Out-Null

    for ($iteration = 1; $iteration -le $Iterations; $iteration++) {
        $logPath = Join-Path $soakDir "iteration-$iteration.log"
        $args = @(
            "-ExecutionPolicy", "Bypass",
            "-File", ".\scripts\binance\binance-paper-run.ps1",
            "-Limit", $Limit
        )
        if ($SkipRefresh) {
            $args += "-SkipRefresh"
        }
        if ($ConfirmTestnetOrder) {
            $args += "-ConfirmTestnetOrder"
        }

        Write-Host "Binance paper soak iteration $iteration/$Iterations"
        $previousErrorActionPreference = $ErrorActionPreference
        $ErrorActionPreference = "Continue"
        try {
            $output = powershell @args 2>&1
            $exitCode = $LASTEXITCODE
        } finally {
            $ErrorActionPreference = $previousErrorActionPreference
        }
        $text = $output -join [Environment]::NewLine
        $text | Set-Content -Path $logPath -Encoding UTF8
        $output | ForEach-Object { Write-Host $_ }

        $summaryPath = Get-MatchValue $text 'summary\s+:\s+(.+summary\.json)'
        $runId = Get-MatchValue $text 'run_id\s+:\s+(\S+)'
        if ([string]::IsNullOrWhiteSpace($runId)) {
            $runId = Get-MatchValue $text 'Binance paper run id:\s+(\S+)'
        }
        $runSummary = Read-Json $summaryPath
        $openOrdersRemaining = if ($null -ne $runSummary) { [int]$runSummary.open_orders_remaining } else { 0 }
        $iterationFailureClass = if ($null -ne $runSummary -and -not [string]::IsNullOrWhiteSpace([string]$runSummary.failure_class)) {
            [string]$runSummary.failure_class
        } elseif ($exitCode -ne 0) {
            "iteration_failed"
        } elseif ($openOrdersRemaining -gt 0) {
            "open_orders_remaining"
        } else {
            "ok"
        }

        $iterationSummary = [pscustomobject]@{
            iteration = $iteration
            exit_code = $exitCode
            status = if ($iterationFailureClass -eq "ok") { "completed" } else { "failed" }
            failure_class = $iterationFailureClass
            run_id = $runId
            log = $logPath
            summary = $summaryPath
            halt_reason = if ($null -ne $runSummary) { $runSummary.halt_reason } else { $null }
            risk_rejections = if ($null -ne $runSummary) { @($runSummary.risk_rejections) } else { @() }
            open_orders_remaining = $openOrdersRemaining
            cancel_all_attempted = if ($null -ne $runSummary) { [bool]$runSummary.cancel_all_attempted } else { $false }
            cancel_all_succeeded = if ($null -ne $runSummary) { [bool]$runSummary.cancel_all_succeeded } else { $false }
            reconciliation_status = if ($null -ne $runSummary) { [string]$runSummary.reconciliation_status } else { "" }
        }
        $iterationSummaries += $iterationSummary

        if ($iterationFailureClass -ne "ok") {
            $failed = $true
            $failureClass = $iterationFailureClass
            $failedIteration = $iteration
            $firstFailedLog = $logPath
            break
        }

        if ($iteration -lt $Iterations -and $DelaySeconds -gt 0) {
            Start-Sleep -Seconds $DelaySeconds
        }
    }

    $summary = [pscustomobject]@{
        soak_id = $soakId
        iterations_requested = $Iterations
        iterations_completed = $iterationSummaries.Count
        limit = $Limit
        skipped_refresh = [bool]$SkipRefresh
        order_submit = if ($ConfirmTestnetOrder) { "enabled" } else { "disabled" }
        status = if ($failed) { "failed" } else { "completed" }
        failure_class = $failureClass
        failed_iteration = $failedIteration
        first_failed_log = $firstFailedLog
        iterations = $iterationSummaries
    }
    $summary | ConvertTo-Json -Depth 5 | Set-Content -Path $soakSummaryPath -Encoding UTF8

    Write-Host "Binance paper soak summary: $soakSummaryPath"

    if ($failed) {
        throw "Binance paper soak failed; see $soakSummaryPath"
    }

    $summary
} finally {
    Set-Location $repoRoot
}
