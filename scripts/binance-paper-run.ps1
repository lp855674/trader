param(
    [string]$Config = "configs/paper/binance_btcusdt_1m_parquet.toml",
    [string]$Symbol = "BTCUSDT",
    [string]$Interval = "1m",
    [int]$Limit = 1000,
    [switch]$SkipRefresh,
    [switch]$ConfirmTestnetOrder
)

$ErrorActionPreference = "Stop"

if ($Limit -lt 1 -or $Limit -gt 1000) {
    throw "Limit must be between 1 and 1000"
}

if ($ConfirmTestnetOrder -and $SkipRefresh) {
    throw "ConfirmTestnetOrder requires a fresh Binance kline refresh; remove -SkipRefresh"
}

$repoRoot = Get-Location
$traderExe = if ($env:TRADER_TEST_EXE) { $env:TRADER_TEST_EXE } else { Join-Path $repoRoot "target/debug/trader.exe" }
$id = [guid]::NewGuid().ToString("N")
$runId = "binance-btcusdt-1m-$($id.Substring(0, 12))"
$runDir = Join-Path $repoRoot "data/binance-paper-runs/$runId"
$runConfigPath = Join-Path $runDir "config.toml"
$databasePath = Join-Path $runDir "run.sqlite"
$databaseUrl = "sqlite://$($databasePath.Replace('\', '/'))"
$textReportPath = Join-Path $runDir "report.txt"
$csvReportPath = Join-Path $runDir "report.csv"
$htmlReportPath = Join-Path $runDir "report.html"
$summaryPath = Join-Path $runDir "summary.json"
$tickerPrice = $null

function Invoke-CheckedCargo {
    param([string[]]$CargoArgs)

    cargo @CargoArgs
    if ($LASTEXITCODE -ne 0) {
        throw "cargo $($CargoArgs -join ' ') failed with exit code $LASTEXITCODE"
    }
}

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

function Get-OpenOrdersCount {
    param([string]$Text)

    $value = Get-MatchValue $Text 'open_orders=(\d+)'
    if ([string]::IsNullOrWhiteSpace($value)) {
        return 0
    }
    return [int]$value
}

function Get-RiskRejections {
    param([string]$Text)

    $events = @()
    foreach ($line in ($Text -split "`r?`n")) {
        if ($line -match '^risk_event:\s+run_id=(\S+)\s+ts_ms=(\S+)\s+account=(.*?)\s+symbol=(.*?)\s+risk_type=(\S+)\s+decision=(\S+)\s+reason=(.*?)\s+threshold=(.*?)\s+observed_value=(.*)$') {
            $events += [pscustomobject]@{
                run_id = $Matches[1]
                ts_ms = $Matches[2]
                account = $Matches[3].Trim()
                symbol = $Matches[4].Trim()
                risk_type = $Matches[5]
                decision = $Matches[6]
                reason = $Matches[7].Trim()
                threshold = $Matches[8].Trim()
                observed_value = $Matches[9].Trim()
            }
        }
    }
    return $events
}

function Get-FirstHaltReason {
    param([object[]]$RiskRejections)

    foreach ($event in $RiskRejections) {
        if ($event.decision -eq "rejected") {
            return [string]$event.risk_type
        }
    }
    return $null
}

function Invoke-CheckedTrader {
    param([string[]]$TraderArgs)

    $global:LASTEXITCODE = 0
    if (Test-Path $traderExe) {
        & $traderExe @TraderArgs
        $exitCode = if ($null -eq $LASTEXITCODE) { 0 } else { $LASTEXITCODE }
        if ($exitCode -ne 0) {
            throw "trader $($TraderArgs -join ' ') failed with exit code $LASTEXITCODE"
        }
    } else {
        Invoke-CheckedCargo (@("run", "-p", "trader-cli", "--") + $TraderArgs)
    }
}

function Invoke-CapturedTrader {
    param([string[]]$TraderArgs)

    $global:LASTEXITCODE = 0
    if (Test-Path $traderExe) {
        $output = & $traderExe @TraderArgs 2>&1
        $exitCode = if ($null -eq $LASTEXITCODE) { 0 } else { $LASTEXITCODE }
        if ($exitCode -ne 0) {
            throw "trader $($TraderArgs -join ' ') failed with exit code $LASTEXITCODE"
        }
    } else {
        $output = cargo @(@("run", "-p", "trader-cli", "--") + $TraderArgs) 2>&1
        $exitCode = if ($null -eq $LASTEXITCODE) { 0 } else { $LASTEXITCODE }
        if ($exitCode -ne 0) {
            throw "cargo run -p trader-cli -- $($TraderArgs -join ' ') failed with exit code $LASTEXITCODE"
        }
    }

    $output | ForEach-Object { Write-Host $_ }
    return ($output -join [Environment]::NewLine)
}

function Invoke-BinancePaperCleanup {
    Write-Host "Running Binance paper cleanup checks"
    $recoverOutput = ""
    $openOrdersOutput = ""

    try {
        $recoverOutput = Invoke-CapturedTrader @("binance-paper-recover", "--config", $runConfigPath)
    } catch {
        Write-Warning "binance-paper-recover failed during cleanup: $_"
        $recoverOutput = "failed: $_"
    }

    try {
        $openOrdersOutput = Invoke-CapturedTrader @("binance-paper-open-orders", "--config", $runConfigPath, "--symbol", $Symbol)
    } catch {
        Write-Warning "binance-paper-open-orders failed during cleanup: $_"
        $openOrdersOutput = "failed: $_"
    }

    [pscustomobject]@{
        recover = $recoverOutput
        open_orders = $openOrdersOutput
        open_orders_remaining = Get-OpenOrdersCount $openOrdersOutput
    }
}

try {
    $env:CARGO_BUILD_JOBS = "1"
    if (-not $env:TRADER_TEST_EXE) {
        Invoke-CheckedCargo @("build", "-p", "trader-cli")
    }

    New-Item -ItemType Directory -Force -Path $runDir | Out-Null

    if (-not $SkipRefresh) {
        powershell -ExecutionPolicy Bypass -File .\scripts\binance-refresh-klines.ps1 `
            -Config $Config `
            -Symbol $Symbol `
            -Interval $Interval `
            -Limit $Limit
        if ($LASTEXITCODE -ne 0) {
            throw "binance-refresh-klines.ps1 failed with exit code $LASTEXITCODE"
        }
    }

    $configText = Get-Content $Config -Raw
    $configText = $configText `
        -replace 'run_id = "binance-btcusdt-1m-paper"', "run_id = `"$runId`"" `
        -replace 'url = "sqlite://data/binance-btcusdt-1m-paper.sqlite"', "url = `"$databaseUrl`""

    if ($ConfirmTestnetOrder) {
        $ticker = Invoke-RestMethod -Uri "https://testnet.binance.vision/api/v3/ticker/price?symbol=$Symbol"
        $tickerPrice = [decimal]$ticker.price
        $configText = $configText -replace 'order_submit_enabled = false', 'order_submit_enabled = true'
    }

    Set-Content -Path $runConfigPath -Value $configText -Encoding UTF8

    Write-Host "Binance paper run id: $runId"
    Write-Host "Binance paper run config: $runConfigPath"
    Write-Host "Binance paper database: $databaseUrl"
    Write-Host "Binance paper symbol: $Symbol"
    Write-Host "Binance paper refresh: $(-not $SkipRefresh)"
    Write-Host "Submit testnet orders: $ConfirmTestnetOrder"
    if ($null -ne $tickerPrice) {
        Write-Host "Binance paper ticker price: $tickerPrice"
    }

    Invoke-CheckedTrader @("check-config", "--config", $runConfigPath)
    Invoke-CheckedTrader @("paper-preflight", "--config", $runConfigPath)
    Invoke-CheckedTrader @("migrate", "--config", $runConfigPath)
    try {
        Invoke-CheckedTrader @("paper-run", "--config", $runConfigPath)
    } catch {
        if ($ConfirmTestnetOrder) {
            Invoke-BinancePaperCleanup
        }
        throw
    }
    Invoke-CheckedTrader @("report", "--config", $runConfigPath, "--run-id", $runId)
    Invoke-CheckedTrader @("report", "--config", $runConfigPath, "--run-id", $runId, "--format", "text", "--output", $textReportPath)
    Invoke-CheckedTrader @("report", "--config", $runConfigPath, "--run-id", $runId, "--format", "csv", "--output", $csvReportPath)
    Invoke-CheckedTrader @("report", "--config", $runConfigPath, "--run-id", $runId, "--format", "html", "--output", $htmlReportPath)
    $cleanup = Invoke-BinancePaperCleanup
    $riskEventsOutput = Invoke-CapturedTrader @("risk-events", "--config", $runConfigPath, "--run-id", $runId)
    $riskRejections = @(Get-RiskRejections $riskEventsOutput)
    $haltReason = Get-FirstHaltReason $riskRejections
    $openOrdersRemaining = $cleanup.open_orders_remaining
    $cancelAllAttempted = $false
    $cancelAllSucceeded = $true
    $cancelAllOutput = ""
    if ($openOrdersRemaining -gt 0) {
        $cancelAllAttempted = $true
        $cancelAllSucceeded = $false
        try {
            $cancelAllOutput = Invoke-CapturedTrader @(
                "risk-kill-switch",
                "--config", $runConfigPath,
                "--run-id", $runId,
                "--cancel-open-orders",
                "--symbol", $Symbol,
                "--confirm-kill-switch"
            )
        } catch {
            Write-Warning "risk-kill-switch cleanup failed: $_"
            $cancelAllOutput = "failed: $_"
        }
        $cleanup = Invoke-BinancePaperCleanup
        $openOrdersRemaining = $cleanup.open_orders_remaining
        $cancelAllSucceeded = ($openOrdersRemaining -eq 0)
    }
    $reconcileOutput = Invoke-CapturedTrader @("binance-paper-reconcile", "--config", $runConfigPath, "--symbol", $Symbol)
    $failureClass = if ($openOrdersRemaining -gt 0) {
        "open_orders_remaining"
    } elseif ($null -ne $haltReason) {
        $haltReason
    } else {
        "ok"
    }
    $status = if ($failureClass -eq "ok") { "completed" } else { "failed" }

    $summary = [pscustomobject]@{
        run_id = $runId
        status = $status
        failure_class = $failureClass
        config = $runConfigPath
        database = $databaseUrl
        symbol = $Symbol
        interval = $Interval
        limit = $Limit
        data_path = "datasets/binance/btcusdt_1m.parquet"
        reports = [pscustomobject]@{
            text = $textReportPath
            csv = $csvReportPath
            html = $htmlReportPath
        }
        ticker_price = if ($null -ne $tickerPrice) { $tickerPrice.ToString() } else { "not_checked" }
        refreshed = if ($SkipRefresh) { "skipped" } else { "ok" }
        order_submit = if ($ConfirmTestnetOrder) { "enabled" } else { "disabled" }
        halt_reason = $haltReason
        risk_rejections = $riskRejections
        open_orders_remaining = $openOrdersRemaining
        cancel_all_attempted = $cancelAllAttempted
        cancel_all_succeeded = $cancelAllSucceeded
        cleanup = $cleanup
        cancel_all = $cancelAllOutput
        reconciliation = $reconcileOutput
        reconciliation_status = if ($reconcileOutput -match "binance paper reconcile ok:") { "ok" } else { "unknown" }
    }
    $summary | ConvertTo-Json -Depth 5 | Set-Content -Path $summaryPath -Encoding UTF8

    Write-Host "summary : $summaryPath"

    if ($status -ne "completed") {
        throw "Binance paper run failed post-run checks: $failureClass; see $summaryPath"
    }

    [pscustomobject]@{
        run_id = $runId
        config = $runConfigPath
        database = $databaseUrl
        symbol = $Symbol
        interval = $Interval
        limit = $Limit
        report_text = $textReportPath
        report_csv = $csvReportPath
        report_html = $htmlReportPath
        summary = $summaryPath
        ticker_price = if ($null -ne $tickerPrice) { $tickerPrice.ToString() } else { "not_checked" }
        refreshed = if ($SkipRefresh) { "skipped" } else { "ok" }
        order_submit = if ($ConfirmTestnetOrder) { "enabled" } else { "disabled" }
    }
} finally {
    Set-Location $repoRoot
}
