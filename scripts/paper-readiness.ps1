param(
    [switch]$SkipCargo,
    [switch]$SkipBinance,
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
$originalBinanceKey = $env:BINANCE_TESTNET_API_KEY
$originalBinanceSecret = $env:BINANCE_TESTNET_SECRET_KEY
$binanceKeyWasSet = $null -ne $originalBinanceKey
$binanceSecretWasSet = $null -ne $originalBinanceSecret

function Invoke-Step {
    param(
        [string]$Name,
        [scriptblock]$Action
    )

    $startedAt = Get-Date
    Write-Host "Paper readiness step: $Name"
    try {
        & $Action
        $script:steps += [pscustomobject]@{
            name = $Name
            status = "ok"
            started_at = $startedAt.ToString("o")
            ended_at = (Get-Date).ToString("o")
            error = ""
        }
    } catch {
        $script:steps += [pscustomobject]@{
            name = $Name
            status = "failed"
            started_at = $startedAt.ToString("o")
            ended_at = (Get-Date).ToString("o")
            error = $_.ToString()
        }
        throw
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
    }

    if (-not $SkipIbkr) {
        Invoke-Step "ibkr paper test plan" {
            powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-test-guide.ps1
            if ($LASTEXITCODE -ne 0) { throw "ibkr-paper-test-guide failed with exit code $LASTEXITCODE" }
        }
        Invoke-Step "ibkr paper soak dry-run" {
            powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-soak.ps1 -Iterations $IbkrSoakIterations -SkipRefresh
            if ($LASTEXITCODE -ne 0) { throw "ibkr-paper-soak failed with exit code $LASTEXITCODE" }
        }
    }

    $summary = [pscustomobject]@{
        readiness_id = $readinessId
        status = "completed"
        cargo = if ($SkipCargo) { "skipped" } else { "ran" }
        binance = if ($SkipBinance) { "skipped" } else { "ran_no_network" }
        ibkr = if ($SkipIbkr) { "skipped" } else { "ran_local_only" }
        ibkr_soak_iterations = if ($SkipIbkr) { 0 } else { $IbkrSoakIterations }
        steps = $steps
    }
    $summary | ConvertTo-Json -Depth 5 | Set-Content -Path $summaryPath -Encoding UTF8
    Write-Host "Paper readiness summary: $summaryPath"
    $summary
} catch {
    $summary = [pscustomobject]@{
        readiness_id = $readinessId
        status = "failed"
        cargo = if ($SkipCargo) { "skipped" } else { "ran" }
        binance = if ($SkipBinance) { "skipped" } else { "ran_no_network" }
        ibkr = if ($SkipIbkr) { "skipped" } else { "ran_local_only" }
        ibkr_soak_iterations = if ($SkipIbkr) { 0 } else { $IbkrSoakIterations }
        steps = $steps
    }
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
