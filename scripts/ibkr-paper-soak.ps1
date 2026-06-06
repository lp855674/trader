param(
    [int]$Iterations = 3,
    [int]$DelaySeconds = 0,
    [switch]$SkipRefresh,
    [switch]$ConfirmIbkrPaperOrder,
    [string]$AccountId = "",
    [string]$GatewayHost = "127.0.0.1",
    [int]$Port = 7497,
    [int]$ClientId = 1
)

$ErrorActionPreference = "Stop"

if ($Iterations -lt 1) {
    throw "Iterations must be at least 1"
}

if ($ConfirmIbkrPaperOrder -and ($AccountId.Trim().Length -eq 0 -or $AccountId -eq "DU000000")) {
    throw "ConfirmIbkrPaperOrder requires a real IBKR paper account id; pass -AccountId DU..."
}

$repoRoot = Get-Location
$id = [guid]::NewGuid().ToString("N")
$soakId = "ibkr-paper-soak-$($id.Substring(0, 12))"
$soakDir = Join-Path $repoRoot "data/ibkr-paper-soak/$soakId"
$soakSummaryPath = Join-Path $soakDir "summary.json"
$iterationSummaries = @()
$failed = $false

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

try {
    New-Item -ItemType Directory -Force -Path $soakDir | Out-Null

    for ($iteration = 1; $iteration -le $Iterations; $iteration++) {
        $logPath = Join-Path $soakDir "iteration-$iteration.log"
        $args = @(
            "-ExecutionPolicy", "Bypass",
            "-File", ".\scripts\ibkr-paper-run.ps1"
        )
        if ($SkipRefresh) {
            $args += "-SkipRefresh"
        }
        if ($ConfirmIbkrPaperOrder) {
            $args += @(
                "-ConfirmIbkrPaperOrder",
                "-AccountId", $AccountId,
                "-GatewayHost", $GatewayHost,
                "-Port", $Port,
                "-ClientId", $ClientId
            )
        }

        Write-Host "IBKR paper soak iteration $iteration/$Iterations"
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
            $runId = Get-MatchValue $text 'IBKR stock paper run id:\s+(\S+)'
        }
        $openOrders = Get-MatchValue $text 'ibkr paper open orders ok: open_orders=(\d+)'
        $reconciliation = Get-MatchValue $text '(ibkr paper reconcile ok:.+)'
        $recover = Get-MatchValue $text '(ibkr paper recover ok:.+)'

        $iterationSummary = [pscustomobject]@{
            iteration = $iteration
            exit_code = $exitCode
            run_id = $runId
            log = $logPath
            summary = $summaryPath
            open_orders = $openOrders
            reconciliation = $reconciliation
            recover = $recover
        }
        $iterationSummaries += $iterationSummary

        if ($exitCode -ne 0) {
            $failed = $true
            break
        }
        if ($ConfirmIbkrPaperOrder -and $openOrders -ne "0") {
            $failed = $true
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
        skipped_refresh = [bool]$SkipRefresh
        account_id = if ($ConfirmIbkrPaperOrder) { $AccountId } else { "not_used" }
        order_submit = if ($ConfirmIbkrPaperOrder) { "enabled" } else { "disabled" }
        status = if ($failed) { "failed" } else { "completed" }
        iterations = $iterationSummaries
    }
    $summary | ConvertTo-Json -Depth 5 | Set-Content -Path $soakSummaryPath -Encoding UTF8

    Write-Host "IBKR paper soak summary: $soakSummaryPath"

    if ($failed) {
        throw "IBKR paper soak failed; see $soakSummaryPath"
    }

    $summary
} finally {
    Set-Location $repoRoot
}
