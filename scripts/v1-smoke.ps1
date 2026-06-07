$ErrorActionPreference = "Stop"

$repoRoot = Get-Location
$id = [guid]::NewGuid().ToString("N")
$databasePath = Join-Path $env:TEMP "trader-v1-$id.sqlite"
$configPath = Join-Path $env:TEMP "trader-v1-$id.toml"
$parquetConfigPath = Join-Path $env:TEMP "trader-v1-$id-parquet.toml"
$parquetPath = Join-Path $env:TEMP "trader-v1-$id.parquet"
$csvReportPath = Join-Path $env:TEMP "trader-v1-$id-report.csv"
$htmlReportPath = Join-Path $env:TEMP "trader-v1-$id-report.html"
$targetDir = $env:TRADER_SMOKE_TARGET_DIR
$stdoutPath = Join-Path $env:TEMP "trader-v1-server-$id.out.log"
$stderrPath = Join-Path $env:TEMP "trader-v1-server-$id.err.log"
$databaseUrl = "sqlite://$($databasePath.Replace('\', '/'))"
$traderExe = Join-Path $repoRoot "target/debug/trader.exe"
$serverExe = Join-Path $repoRoot "target/debug/trader-server.exe"

$template = Get-Content "configs/backtest/ma_cross.toml" -Raw
$config = $template -replace 'url = "sqlite://data/trader.sqlite"', "url = `"$databaseUrl`""
$parquetConfig = $config `
    -replace 'source = "csv"', 'source = "parquet"' `
    -replace 'path = "datasets/sample/aapl_1d.csv"', "path = `"$($parquetPath.Replace('\', '/'))`""
Set-Content -Path $configPath -Value $config -Encoding UTF8
Set-Content -Path $parquetConfigPath -Value $parquetConfig -Encoding UTF8

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

function Receive-WebSocketText {
    param([System.Net.WebSockets.ClientWebSocket]$Socket)

    $buffer = [System.Array]::CreateInstance([byte], 8192)
    $segment = [System.ArraySegment[byte]]::new($buffer)
    $result = $Socket.ReceiveAsync($segment, [Threading.CancellationToken]::None).GetAwaiter().GetResult()
    if ($result.Count -le 0) { return "" }
    [Text.Encoding]::UTF8.GetString($buffer, 0, $result.Count)
}

function Send-WebSocketText {
    param([System.Net.WebSockets.ClientWebSocket]$Socket, [string]$Text)

    $bytes = [Text.Encoding]::UTF8.GetBytes($Text)
    $segment = [System.ArraySegment[byte]]::new($bytes)
    $null = $Socket.SendAsync($segment, [System.Net.WebSockets.WebSocketMessageType]::Text, $true, [Threading.CancellationToken]::None).GetAwaiter().GetResult()
}

function Receive-WebSocketUntil {
    param([System.Net.WebSockets.ClientWebSocket]$Socket, [string]$Fragment)

    for ($i = 0; $i -lt 20; $i++) {
        $text = Receive-WebSocketText $Socket
        if ($text.Contains($Fragment)) { return $text }
    }
    throw "websocket message did not contain $Fragment"
}

$server = $null
try {
    $env:CARGO_BUILD_JOBS = "1"
    if ($targetDir) {
        $env:CARGO_TARGET_DIR = $targetDir
    }
    Write-Host "V1 config: $configPath"
    Write-Host "V1 parquet config: $parquetConfigPath"
    Write-Host "V1 database: $databaseUrl"
    if ($targetDir) {
        Write-Host "V1 target: $targetDir"
    } else {
        Write-Host "V1 target: default workspace target"
    }

    Invoke-CheckedTrader @("check-config", "--config", $configPath)
    Invoke-CheckedTrader @("migrate", "--config", $configPath)
    Invoke-CheckedTrader @("import-bars", "--config", $configPath, "--output-parquet", $parquetPath)
    Assert-True (Test-Path $parquetPath) "expected parquet output"
    Invoke-CheckedTrader @("backtest", "--config", $parquetConfigPath)
    Invoke-CheckedTrader @("paper-run", "--config", $configPath)
    Invoke-CheckedTrader @("replay", "--config", $configPath)
    Invoke-CheckedTrader @("report", "--config", $configPath, "--format", "csv", "--output", $csvReportPath)
    Invoke-CheckedTrader @("report", "--config", $configPath, "--format", "html", "--output", $htmlReportPath)
    Assert-True ((Get-Content $csvReportPath -Raw).Contains("sample-ma-cross")) "expected CSV report run id"
    Assert-True ((Get-Content $htmlReportPath -Raw).Contains("<h1>Trader Report</h1>")) "expected HTML report title"

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

    $brokerStatus = Invoke-RestMethod "$baseUrl/api/v1/brokers/status"
    Assert-True (@($brokerStatus).Count -ge 4) "expected fake broker statuses"

    $paper = Invoke-RestMethod -Method Post "$baseUrl/api/v1/paper-runs"
    Wait-RunStatus $baseUrl $paper.run_id "completed" | Out-Null
    $fills = Invoke-RestMethod "$baseUrl/api/v1/fills"
    $snapshots = Invoke-RestMethod "$baseUrl/api/v1/portfolio/snapshots"
    $metrics = Invoke-RestMethod "$baseUrl/api/v1/metrics"
    Assert-True (@($fills).Count -ge 1) "expected fills"
    Assert-True (@($snapshots).Count -ge 1) "expected portfolio snapshots"
    Assert-True ($metrics.fill_count -ge 1) "expected metrics fill_count"

    $replay = Invoke-RestMethod -Method Post "$baseUrl/api/v1/replays"
    Assert-True ($replay.bars -ge 1) "expected replay bars"
    $pause = Invoke-RestMethod -Method Post "$baseUrl/api/v1/replay/$($paper.run_id)/pause"
    $seek = Invoke-RestMethod -Method Post "$baseUrl/api/v1/replay/$($paper.run_id)/seek/2"
    $speed = Invoke-RestMethod -Method Post "$baseUrl/api/v1/replay/$($paper.run_id)/speed/25"
    $resume = Invoke-RestMethod -Method Post "$baseUrl/api/v1/replay/$($paper.run_id)/resume"
    Assert-True ($pause.status -eq "paused") "expected replay paused"
    Assert-True ($seek.offset -eq 2) "expected replay offset"
    Assert-True ($speed.speed -eq 25) "expected replay speed"
    Assert-True ($resume.status -eq "running") "expected replay running"

    $live = Invoke-RestMethod -Method Post "$baseUrl/api/v1/live-runs"
    Assert-True ($live.status -eq "running") "expected live running"
    Wait-RunStatus $baseUrl $live.run_id "running" | Out-Null
    $stopped = Invoke-RestMethod -Method Post "$baseUrl/api/v1/live-runs/$($live.run_id)/stop"
    Assert-True ($stopped.status -eq "stopped") "expected live stopped"

    $socket = [System.Net.WebSockets.ClientWebSocket]::new()
    $null = $socket.ConnectAsync([Uri]"ws://127.0.0.1:8080/ws", [Threading.CancellationToken]::None).GetAwaiter().GetResult()
    try {
        Send-WebSocketText $socket (@{ type = "subscribe"; run_id = $paper.run_id } | ConvertTo-Json -Compress)
        $eventText = Receive-WebSocketUntil $socket '"type":"event"'
        Assert-True ($eventText.Contains('"type":"event"')) "expected websocket event replay"
    } finally {
        $socket.Abort()
        $socket.Dispose()
    }

    $controlSocket = [System.Net.WebSockets.ClientWebSocket]::new()
    $null = $controlSocket.ConnectAsync([Uri]"ws://127.0.0.1:8080/ws", [Threading.CancellationToken]::None).GetAwaiter().GetResult()
    try {
        Send-WebSocketText $controlSocket (@{ type = "replay_control"; run_id = $paper.run_id; action = "pause" } | ConvertTo-Json -Compress)
        $replayText = Receive-WebSocketUntil $controlSocket '"type":"replay_state"'
        Assert-True ($replayText.Contains('"type":"replay_state"')) "expected websocket replay_state"
    } finally {
        $controlSocket.Abort()
        $controlSocket.Dispose()
    }

    [pscustomobject]@{
        run_id = $paper.run_id
        fills = @($fills).Count
        snapshots = @($snapshots).Count
        total_return = $metrics.total_return
        replay_bars = $replay.bars
        live_status = $stopped.status
        brokers = @($brokerStatus).Count
        parquet = Test-Path $parquetPath
        csv_report = Test-Path $csvReportPath
        html_report = Test-Path $htmlReportPath
    }
} finally {
    Set-Location $repoRoot
    if ($server -and -not $server.HasExited) {
        Stop-Process -Id $server.Id -Force
    }
    Remove-Item -LiteralPath $configPath -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $parquetConfigPath -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $databasePath -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath "$databasePath-shm" -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath "$databasePath-wal" -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $parquetPath -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $csvReportPath -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $htmlReportPath -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $stdoutPath -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $stderrPath -Force -ErrorAction SilentlyContinue
    if ($targetDir -and $env:TRADER_SMOKE_CLEAN_TARGET -eq "1") {
        Remove-Item -LiteralPath $targetDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}
