$ErrorActionPreference = "Stop"

$repoRoot = Get-Location
$id = [guid]::NewGuid().ToString("N")
$databasePath = Join-Path $env:TEMP "trader-paper-$id.sqlite"
$configPath = Join-Path $env:TEMP "trader-paper-$id.toml"
$stdoutPath = Join-Path $env:TEMP "trader-paper-server-$id.out.log"
$stderrPath = Join-Path $env:TEMP "trader-paper-server-$id.err.log"
$databaseUrl = "sqlite://$($databasePath.Replace('\', '/'))"
$traderExe = Join-Path $repoRoot "target/debug/trader.exe"

$template = Get-Content "configs/backtest/slow-paper.toml" -Raw
$config = $template `
    -replace 'run_id = "sample-slow-paper"', "run_id = `"paper-smoke-$id`"" `
    -replace 'url = "sqlite://data/trader.sqlite"', "url = `"$databaseUrl`"" `
    -replace 'bar_delay_ms = 50', 'bar_delay_ms = 1'
Set-Content -Path $configPath -Value $config -Encoding UTF8

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

    for ($i = 0; $i -lt 120; $i++) {
        Start-Sleep -Milliseconds 250
        $status = Invoke-RestMethod "$BaseUrl/api/v1/runs/$RunId/status"
        if ($status.status -eq $Expected) { return $status }
    }
    throw "run $RunId did not reach $Expected"
}

$server = $null
try {
    $env:CARGO_BUILD_JOBS = "1"
    Write-Host "Paper config: $configPath"
    Write-Host "Paper database: $databaseUrl"

    Invoke-CheckedTrader @("check-config", "--config", $configPath)
    Invoke-CheckedTrader @("paper-preflight", "--config", $configPath)
    Invoke-CheckedTrader @("migrate", "--config", $configPath)

    $env:TRADER_CONFIG = $configPath
    $env:TRADER_DATABASE_URL = $databaseUrl
    $server = Start-Process -FilePath "cargo" `
        -ArgumentList @("run", "-p", "trader-server") `
        -WorkingDirectory $repoRoot `
        -PassThru `
        -RedirectStandardOutput $stdoutPath `
        -RedirectStandardError $stderrPath `
        -WindowStyle Hidden

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

    $serverPreflight = Invoke-RestMethod "$baseUrl/api/v1/preflight/paper"
    Assert-True ($serverPreflight.status -eq "ok") "expected server paper preflight ok"
    Assert-True ($serverPreflight.real_broker_connection -eq $false) "expected local fake broker paper"

    $paper = Invoke-RestMethod -Method Post "$baseUrl/api/v1/paper-runs"
    Assert-True ($paper.status -eq "running") "expected paper run to start as running"
    $status = Wait-RunStatus $baseUrl $paper.run_id "completed"

    $run = Invoke-RestMethod "$baseUrl/api/v1/runs/$($paper.run_id)"
    $orders = Invoke-RestMethod "$baseUrl/api/v1/orders"
    $fills = Invoke-RestMethod "$baseUrl/api/v1/fills"
    $balances = Invoke-RestMethod "$baseUrl/api/v1/account-balances"
    $snapshots = Invoke-RestMethod "$baseUrl/api/v1/portfolio/snapshots"
    $metrics = Invoke-RestMethod "$baseUrl/api/v1/metrics"
    $events = Invoke-RestMethod "$baseUrl/api/v1/runs/$($paper.run_id)/events"
    $brokerAccount = Invoke-RestMethod "$baseUrl/api/v1/brokers/account/paper"

    Assert-True ($run.status -eq "completed") "expected completed run"
    Assert-True (@($orders).Count -ge 1) "expected at least one order"
    Assert-True (@($fills).Count -ge 1) "expected at least one fill"
    Assert-True (@($balances).Count -ge 1) "expected at least one account balance"
    Assert-True (@($snapshots).Count -ge 1) "expected at least one portfolio snapshot"
    Assert-True ($metrics.fill_count -ge 1) "expected metrics fill_count >= 1"
    Assert-True (@($events).Count -ge 1) "expected at least one paper lifecycle event"
    Assert-True ($brokerAccount.account_id -eq "paper") "expected broker account id paper"
    Assert-True ($brokerAccount.margin_used -eq "0") "expected fake broker margin_used 0"

    [pscustomobject]@{
        run_id = $paper.run_id
        status = $status.status
        orders = @($orders).Count
        fills = @($fills).Count
        balances = @($balances).Count
        snapshots = @($snapshots).Count
        total_return = $metrics.total_return
        broker_cash = $brokerAccount.cash
        server_preflight = $serverPreflight.status
        events = @($events).Count
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
