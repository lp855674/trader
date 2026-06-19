$ErrorActionPreference = "Stop"

$repoRoot = Get-Location
$id = [guid]::NewGuid().ToString("N")
$databasePath = Join-Path $env:TEMP "trader-ops-$id.sqlite"
$configPath = Join-Path $env:TEMP "trader-ops-$id.toml"
$stdoutPath = Join-Path $env:TEMP "trader-ops-server-$id.out.log"
$stderrPath = Join-Path $env:TEMP "trader-ops-server-$id.err.log"
$databaseUrl = "sqlite://$($databasePath.Replace('\', '/'))"
$runId = "ops-live-$id"
$targetDir = $env:TRADER_SMOKE_TARGET_DIR
$targetRoot = if ($targetDir) { $targetDir } else { Join-Path $repoRoot "target" }
$traderExe = Join-Path $targetRoot "debug/trader.exe"
$serverExe = Join-Path $targetRoot "debug/trader-server.exe"

function Invoke-CheckedCargo {
    param([string[]]$CargoArgs)

    cargo @CargoArgs
    if ($LASTEXITCODE -ne 0) {
        throw "cargo $($CargoArgs -join ' ') failed with exit code $LASTEXITCODE"
    }
}

function Invoke-CheckedTrader {
    param([string[]]$TraderArgs)

    if (Test-Path $traderExe) {
        & $traderExe @TraderArgs
        if ($LASTEXITCODE -ne 0) {
            throw "trader $($TraderArgs -join ' ') failed with exit code $LASTEXITCODE"
        }
    } else {
        Invoke-CheckedCargo (@("run", "-p", "trader-cli", "--") + $TraderArgs)
    }
}

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

function Wait-RunStatus {
    param([string]$BaseUrl, [string]$RunId, [string]$Expected)

    for ($i = 0; $i -lt 80; $i++) {
        Start-Sleep -Milliseconds 250
        $status = Invoke-RestMethod "$BaseUrl/api/v1/runs/$RunId/status"
        if ($status.status -eq $Expected) { return $status }
    }
    throw "run $RunId did not reach $Expected"
}

function Wait-ApiArray {
    param([string]$Url, [string]$Description)

    for ($i = 0; $i -lt 80; $i++) {
        Start-Sleep -Milliseconds 250
        $items = Invoke-RestMethod $Url
        if (@($items).Count -ge 1) { return $items }
    }
    throw "expected at least one $Description"
}

function Wait-ApiObject {
    param([string]$Url, [string]$Description)

    for ($i = 0; $i -lt 80; $i++) {
        Start-Sleep -Milliseconds 250
        try {
            return Invoke-RestMethod $Url
        } catch {}
    }
    throw "expected $Description"
}

$template = Get-Content "configs/backtest/ma_cross.toml" -Raw
$config = $template `
    -replace 'mode = "backtest"', 'mode = "live"' `
    -replace 'run_id = "sample-ma-cross"', "run_id = `"$runId`"" `
    -replace 'url = "sqlite://data/trader.sqlite"', "url = `"$databaseUrl`"" `
    -replace 'initial_cash = "100000"', 'initial_cash = "25000"' `
    -replace 'enabled = false', "enabled = true`nbroker_snapshot_interval_ms = 5"
Set-Content -Path $configPath -Value $config -Encoding UTF8

$server = $null
try {
    $env:CARGO_BUILD_JOBS = if ($env:CARGO_BUILD_JOBS) { $env:CARGO_BUILD_JOBS } else { "1" }
    if ($targetDir) {
        $env:CARGO_TARGET_DIR = $targetDir
    }
    Write-Host "Ops smoke config: $configPath"
    Write-Host "Ops smoke database: $databaseUrl"

    Invoke-CheckedCargo @("build", "-p", "trader-cli", "-p", "trader-server")

    $env:TRADER_CONFIG = $configPath
    $env:TRADER_DATABASE_URL = $databaseUrl
    if (Test-Path $serverExe) {
        $server = Start-Process -FilePath $serverExe `
            -WorkingDirectory $repoRoot `
            -PassThru `
            -RedirectStandardOutput $stdoutPath `
            -RedirectStandardError $stderrPath `
            -WindowStyle Hidden
    } else {
        $server = Start-Process -FilePath "cargo" `
            -ArgumentList @("run", "-p", "trader-server") `
            -WorkingDirectory $repoRoot `
            -PassThru `
            -RedirectStandardOutput $stdoutPath `
            -RedirectStandardError $stderrPath `
            -WindowStyle Hidden
    }

    $baseUrl = "http://127.0.0.1:8080"
    $ready = $false
    for ($i = 0; $i -lt 80; $i++) {
        if ($server.HasExited) { break }
        Start-Sleep -Milliseconds 500
        try {
            Invoke-RestMethod "$baseUrl/api/v1/health" | Out-Null
            $ready = $true
            break
        } catch {}
    }
    if (-not $ready) {
        if (Test-Path $stdoutPath) { Get-Content $stdoutPath }
        if (Test-Path $stderrPath) { Get-Content $stderrPath }
        throw "trader-server did not become ready"
    }

    $live = Invoke-RestMethod -Method Post "$baseUrl/api/v1/live-runs"
    Assert-True ($live.run_id -eq $runId) "expected live run id $runId"
    Wait-RunStatus $baseUrl $runId "running" | Out-Null

    $apiCash = Wait-ApiArray "$baseUrl/api/v1/runs/$runId/cash-snapshots?currency=USD" "API cash snapshot"
    $apiPositions = Wait-ApiArray "$baseUrl/api/v1/runs/$runId/position-snapshots?symbol=CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP&position_side=long" "API position snapshot"
    $apiReconciliation = Wait-ApiObject "$baseUrl/api/v1/runs/$runId/reconciliation" "API reconciliation"
    $apiLogs = Wait-ApiArray "$baseUrl/api/v1/runs/$runId/system-logs?target=runtime.broker_snapshot" "API broker snapshot logs"
    $apiConfigVersion = Wait-ApiObject "$baseUrl/api/v1/runs/$runId/config-version" "API config version binding"

    Assert-True (@($apiCash).Count -ge 1) "expected API cash snapshots"
    Assert-True (@($apiPositions).Count -ge 1) "expected API position snapshots"
    Assert-True ($apiReconciliation.status -eq "drift") "expected API reconciliation drift"
    Assert-True (@($apiLogs).Count -ge 1) "expected API system logs"
    Assert-True ($apiConfigVersion.run_id -eq $runId) "expected API config version run id"

    $cliCash = Invoke-CheckedTrader @("snapshots", "cash", "--config", $configPath, "--run-id", $runId, "--currency", "USD") 2>&1 | Out-String
    $cliPositions = Invoke-CheckedTrader @("snapshots", "positions", "--config", $configPath, "--run-id", $runId, "--symbol", "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP", "--position-side", "long") 2>&1 | Out-String
    $cliReconciliation = Invoke-CheckedTrader @("reconciliation", "--config", $configPath, "--run-id", $runId) 2>&1 | Out-String
    $cliLogs = Invoke-CheckedTrader @("logs", "list", "--config", $configPath, "--run-id", $runId, "--target", "runtime.broker_snapshot") 2>&1 | Out-String
    $cliConfigVersion = Invoke-CheckedTrader @("runs", "config-version", "--config", $configPath, "--run-id", $runId) 2>&1 | Out-String

    Assert-True ($cliCash.Contains("cash_snapshot: run_id=$runId")) "expected CLI cash snapshot"
    Assert-True ($cliPositions.Contains("position_snapshot: run_id=$runId")) "expected CLI position snapshot"
    Assert-True ($cliReconciliation.Contains("reconciliation: run_id=$runId status=drift")) "expected CLI reconciliation drift"
    Assert-True ($cliLogs.Contains("system_log: run_id=$runId")) "expected CLI system log"
    Assert-True ($cliConfigVersion.Contains("run_config_version: run_id=$runId")) "expected CLI config version binding"
    Assert-True (-not $cliConfigVersion.Contains("status=missing")) "expected bound CLI config version"

    $stopped = Invoke-RestMethod -Method Post "$baseUrl/api/v1/live-runs/$runId/stop"
    Assert-True ($stopped.status -eq "stopped") "expected live stopped"

    [pscustomobject]@{
        run_id = $runId
        api_cash_snapshots = @($apiCash).Count
        api_position_snapshots = @($apiPositions).Count
        api_reconciliation = $apiReconciliation.status
        api_system_logs = @($apiLogs).Count
        config_version = $apiConfigVersion.version
    }
} finally {
    Set-Location $repoRoot
    if ($server -and -not $server.HasExited) {
        Stop-Process -Id $server.Id -Force
    }
    Remove-Item -LiteralPath $configPath -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $databasePath -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath "$databasePath-shm" -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath "$databasePath-wal" -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $stdoutPath -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $stderrPath -Force -ErrorAction SilentlyContinue
}
