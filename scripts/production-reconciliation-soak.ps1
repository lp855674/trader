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

$iterations = @()
$failed = $false
$failureClass = "ok"

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
    if ($text -match "gateway_unreachable") { $iterationFailureClass = "gateway_unreachable" }
    if ($text -match "account_mismatch") { $iterationFailureClass = "account_mismatch" }
    if ($text -match "reconciliation_drift") { $iterationFailureClass = "reconciliation_drift" }

    $iterations += [pscustomobject]@{
        iteration = $iteration
        exit_code = $exitCode
        status = $iterationStatus
        failure_class = $iterationFailureClass
        log = $iterationLog
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
    iterations_completed = $iterations.Count
    status = if ($failed) { "failed" } else { "completed" }
    failure_class = $failureClass
    evidence_dir = $soakDir
    iterations = $iterations
}
$summary | ConvertTo-Json -Depth 6 | Set-Content -Path $summaryPath -Encoding UTF8
Write-Host "Production reconciliation soak summary: $summaryPath"

if ($failed) {
    throw "Production reconciliation soak failed; see $summaryPath"
}

$summary
