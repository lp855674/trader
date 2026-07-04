param(
    [switch]$SkipCargo,
    [switch]$SkipBinance,
    [switch]$SkipReferenceData,
    [switch]$SkipIbkr,
    [int]$IbkrSoakIterations = 1
)

$ErrorActionPreference = "Stop"

if ($IbkrSoakIterations -lt 1) {
    throw "IbkrSoakIterations must be at least 1"
}

$repoRoot = Get-Location
$id = [guid]::NewGuid().ToString("N")
$readinessId = "paper-readiness-$($id.Substring(0, 12))"
$readinessDir = Join-Path $repoRoot "data/paper-readiness/$readinessId"
$summaryPath = Join-Path $readinessDir "summary.json"
$steps = @()
$gates = [ordered]@{}
$originalBinanceKey = $env:BINANCE_TESTNET_API_KEY
$originalBinanceSecret = $env:BINANCE_TESTNET_SECRET_KEY
$binanceKeyWasSet = $null -ne $originalBinanceKey
$binanceSecretWasSet = $null -ne $originalBinanceSecret

function Invoke-Step {
    param(
        [string]$Name,
        [scriptblock]$Action,
        [string]$Gate = ""
    )

    $startedAt = Get-Date
    Write-Host "Paper readiness step: $Name"
    try {
        & $Action
        $endedAt = Get-Date
        $step = [pscustomobject]@{
            name = $Name
            status = "ok"
            started_at = $startedAt.ToString("o")
            ended_at = $endedAt.ToString("o")
            error = ""
        }
        $script:steps += $step
        if (-not [string]::IsNullOrWhiteSpace($Gate)) {
            $script:gates[$Gate] = [pscustomobject]@{
                status = "ok"
                step = $Name
                started_at = $startedAt.ToString("o")
                ended_at = $endedAt.ToString("o")
                error = ""
            }
        }
    } catch {
        $endedAt = Get-Date
        $step = [pscustomobject]@{
            name = $Name
            status = "failed"
            started_at = $startedAt.ToString("o")
            ended_at = $endedAt.ToString("o")
            error = $_.ToString()
        }
        $script:steps += $step
        if (-not [string]::IsNullOrWhiteSpace($Gate)) {
            $script:gates[$Gate] = [pscustomobject]@{
                status = "failed"
                step = $Name
                started_at = $startedAt.ToString("o")
                ended_at = $endedAt.ToString("o")
                error = $_.ToString()
            }
        }
        throw
    }
}

function Set-SkippedGate {
    param(
        [string]$Gate,
        [string]$Reason
    )

    $script:gates[$Gate] = [pscustomobject]@{
        status = "skipped"
        step = ""
        started_at = ""
        ended_at = ""
        error = $Reason
    }
}

function New-ReadinessSummary {
    param([string]$Status)

    return [pscustomobject]@{
        readiness_id = $readinessId
        status = $Status
        cargo = if ($SkipCargo) { "skipped" } else { "ran" }
        reference_data = if ($SkipReferenceData -or $SkipCargo) { "skipped" } else { "ran" }
        binance = if ($SkipBinance) { "skipped" } else { "ran_no_network" }
        ibkr = if ($SkipIbkr) { "skipped" } else { "ran_local_only" }
        ibkr_soak_iterations = if ($SkipIbkr) { 0 } else { $IbkrSoakIterations }
        gates = [pscustomobject]$gates
        steps = $steps
    }
}

try {
    New-Item -ItemType Directory -Force -Path $readinessDir | Out-Null

    if (-not $SkipCargo) {
        Invoke-Step "cargo fmt" {
            cargo fmt --all -- --check
            if ($LASTEXITCODE -ne 0) { throw "cargo fmt failed with exit code $LASTEXITCODE" }
        }
        Invoke-Step "cargo check workspace" {
            cargo check --workspace --locked -j 1
            if ($LASTEXITCODE -ne 0) { throw "cargo check failed with exit code $LASTEXITCODE" }
        }
        Invoke-Step "cargo test trader-cli" {
            cargo test -p trader-cli -j 1
            if ($LASTEXITCODE -ne 0) { throw "cargo test -p trader-cli failed with exit code $LASTEXITCODE" }
        }
        Invoke-Step "cargo test paper" {
            cargo test -p paper -j 1
            if ($LASTEXITCODE -ne 0) { throw "cargo test -p paper failed with exit code $LASTEXITCODE" }
        }
    }

    if ($SkipCargo -or $SkipReferenceData) {
        Set-SkippedGate "reference_data_observable" "Skipped by SkipCargo or SkipReferenceData"
        Set-SkippedGate "reference_data_retry_tests" "Skipped by SkipCargo or SkipReferenceData"
    } else {
        Invoke-Step "reference data stale alert test" {
            cargo test -p data ingestion_tracker_marks_stale_reference_data_and_logs_alert -j 1
            if ($LASTEXITCODE -ne 0) { throw "reference data stale alert test failed with exit code $LASTEXITCODE" }
        } -Gate "reference_data_observable"
        Invoke-Step "reference data retry/backoff tests" {
            cargo test -p data ingestion_http_retry -j 1
            if ($LASTEXITCODE -ne 0) { throw "reference data retry/backoff tests failed with exit code $LASTEXITCODE" }
        } -Gate "reference_data_retry_tests"
    }

    if (-not $SkipBinance) {
        if (-not $binanceKeyWasSet) {
            $env:BINANCE_TESTNET_API_KEY = "paper-readiness-key"
        }
        if (-not $binanceSecretWasSet) {
            $env:BINANCE_TESTNET_SECRET_KEY = "paper-readiness-secret"
        }
        Invoke-Step "binance paper smoke no network" {
            powershell -ExecutionPolicy Bypass -File .\scripts\binance-paper-smoke.ps1 -SkipNetwork
            if ($LASTEXITCODE -ne 0) { throw "binance-paper-smoke failed with exit code $LASTEXITCODE" }
        }
        Invoke-Step "binance recover smoke no network" {
            powershell -ExecutionPolicy Bypass -File .\scripts\binance-paper-recover-smoke.ps1 -SkipNetwork
            if ($LASTEXITCODE -ne 0) { throw "binance-paper-recover-smoke failed with exit code $LASTEXITCODE" }
        }
        Invoke-Step "binance paper run summary behavior" {
            powershell -ExecutionPolicy Bypass -File .\scripts\binance-paper-script-tests.ps1
            if ($LASTEXITCODE -ne 0) { throw "binance-paper-script-tests failed with exit code $LASTEXITCODE" }
        } -Gate "binance_paper_summary_behavior"
    } else {
        Set-SkippedGate "binance_paper_summary_behavior" "Skipped by SkipBinance"
    }

    if (-not $SkipIbkr) {
        Invoke-Step "ibkr paper local dry-run" {
            powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-run.ps1 -SkipRefresh
            if ($LASTEXITCODE -ne 0) { throw "ibkr-paper-run dry-run failed with exit code $LASTEXITCODE" }
        } -Gate "ibkr_paper_local_dry_run"
        Invoke-Step "ibkr read-only and run summary behavior" {
            powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-script-tests.ps1
            if ($LASTEXITCODE -ne 0) { throw "ibkr-paper-script-tests failed with exit code $LASTEXITCODE" }
        } -Gate "ibkr_read_only_summary_behavior"
        Invoke-Step "ibkr paper test plan" {
            powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-test-guide.ps1
            if ($LASTEXITCODE -ne 0) { throw "ibkr-paper-test-guide failed with exit code $LASTEXITCODE" }
        }
        Invoke-Step "ibkr paper soak dry-run" {
            powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-soak.ps1 -Iterations $IbkrSoakIterations -SkipRefresh
            if ($LASTEXITCODE -ne 0) { throw "ibkr-paper-soak failed with exit code $LASTEXITCODE" }
        } -Gate "ibkr_soak_summary_behavior"
    } else {
        Set-SkippedGate "ibkr_paper_local_dry_run" "Skipped by SkipIbkr"
        Set-SkippedGate "ibkr_read_only_summary_behavior" "Skipped by SkipIbkr"
        Set-SkippedGate "ibkr_soak_summary_behavior" "Skipped by SkipIbkr"
    }

    $summary = New-ReadinessSummary -Status "completed"
    $summary | ConvertTo-Json -Depth 5 | Set-Content -Path $summaryPath -Encoding UTF8
    Write-Host "Paper readiness summary: $summaryPath"
    $summary
} catch {
    $summary = New-ReadinessSummary -Status "failed"
    $summary | ConvertTo-Json -Depth 5 | Set-Content -Path $summaryPath -Encoding UTF8
    Write-Host "Paper readiness summary: $summaryPath"
    throw
} finally {
    if ($binanceKeyWasSet) {
        $env:BINANCE_TESTNET_API_KEY = $originalBinanceKey
    } else {
        Remove-Item Env:\BINANCE_TESTNET_API_KEY -ErrorAction SilentlyContinue
    }
    if ($binanceSecretWasSet) {
        $env:BINANCE_TESTNET_SECRET_KEY = $originalBinanceSecret
    } else {
        Remove-Item Env:\BINANCE_TESTNET_SECRET_KEY -ErrorAction SilentlyContinue
    }
    Set-Location $repoRoot
}
