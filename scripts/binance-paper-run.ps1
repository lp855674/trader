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
$traderExe = Join-Path $repoRoot "target/debug/trader.exe"
$id = [guid]::NewGuid().ToString("N")
$runId = "binance-btcusdt-1m-$($id.Substring(0, 12))"
$runDir = Join-Path $repoRoot "data/binance-paper-runs/$runId"
$runConfigPath = Join-Path $runDir "config.toml"
$databasePath = Join-Path $runDir "run.sqlite"
$databaseUrl = "sqlite://$($databasePath.Replace('\', '/'))"
$textReportPath = Join-Path $runDir "report.txt"
$csvReportPath = Join-Path $runDir "report.csv"
$htmlReportPath = Join-Path $runDir "report.html"
$tickerPrice = $null

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

function Invoke-BinancePaperCleanup {
    Write-Host "Running Binance paper cleanup checks"

    try {
        Invoke-CheckedTrader @("binance-paper-recover", "--config", $runConfigPath)
    } catch {
        Write-Warning "binance-paper-recover failed during cleanup: $_"
    }

    try {
        Invoke-CheckedTrader @("binance-paper-open-orders", "--config", $runConfigPath, "--symbol", $Symbol)
    } catch {
        Write-Warning "binance-paper-open-orders failed during cleanup: $_"
    }
}

try {
    $env:CARGO_BUILD_JOBS = "1"
    Invoke-CheckedCargo @("build", "-p", "trader-cli")

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
    Invoke-CheckedTrader @("report", "--config", $runConfigPath)
    Invoke-CheckedTrader @("report", "--config", $runConfigPath, "--format", "text", "--output", $textReportPath)
    Invoke-CheckedTrader @("report", "--config", $runConfigPath, "--format", "csv", "--output", $csvReportPath)
    Invoke-CheckedTrader @("report", "--config", $runConfigPath, "--format", "html", "--output", $htmlReportPath)
    Invoke-BinancePaperCleanup

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
        ticker_price = if ($null -ne $tickerPrice) { $tickerPrice.ToString() } else { "not_checked" }
        refreshed = if ($SkipRefresh) { "skipped" } else { "ok" }
        order_submit = if ($ConfirmTestnetOrder) { "enabled" } else { "disabled" }
    }
} finally {
    Set-Location $repoRoot
}
