param(
    [string]$Config = "configs/paper/binance_testnet.toml",
    [string]$Symbol = "BTCUSDT",
    [string]$Interval = "1m",
    [int]$Limit = 100,
    [switch]$RunPaper,
    [switch]$ConfirmTestnetOrder
)

$ErrorActionPreference = "Stop"

if ($Limit -lt 1 -or $Limit -gt 1000) {
    throw "Limit must be between 1 and 1000"
}

if ($ConfirmTestnetOrder) {
    $RunPaper = $true
}

$repoRoot = Get-Location
$traderExe = Join-Path $repoRoot "target/debug/trader.exe"
$id = [guid]::NewGuid().ToString("N")
$databasePath = Join-Path $env:TEMP "trader-binance-paper-real-$id.sqlite"
$configPath = Join-Path $env:TEMP "trader-binance-paper-real-$id.toml"
$barsPath = Join-Path $env:TEMP "trader-binance-paper-real-$id.parquet"
$databaseUrl = "sqlite://$($databasePath.Replace('\', '/'))"
$barsConfigPath = $barsPath.Replace('\', '/')
$runId = "binance-paper-real-$id"

$template = Get-Content $Config -Raw
$configText = $template `
    -replace 'run_id = "binance-testnet-readonly"', "run_id = `"$runId`"" `
    -replace 'url = "sqlite://data/binance/databases/binance-testnet.sqlite"', "url = `"$databaseUrl`"" `
    -replace 'source = "csv"', 'source = "parquet"' `
    -replace 'path = "datasets/sample/aapl_1d.csv"', "path = `"$barsConfigPath`"" `
    -replace 'max_order_notional = "50"', 'max_order_notional = "200"'

if ($ConfirmTestnetOrder) {
    $configText = $configText -replace 'order_submit_enabled = false', 'order_submit_enabled = true'
}

Set-Content -Path $configPath -Value $configText -Encoding UTF8

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

try {
    $env:CARGO_BUILD_JOBS = "1"
    Invoke-CheckedCargo @("build", "-p", "trader-cli")

    Write-Host "Binance real paper config: $configPath"
    Write-Host "Binance real paper bars: $barsPath"
    Write-Host "Binance real paper database: $databaseUrl"
    Write-Host "Binance real paper symbol: $Symbol"
    Write-Host "Binance real paper interval: $Interval"
    Write-Host "Binance real paper limit: $Limit"
    Write-Host "Run paper: $RunPaper"
    Write-Host "Submit testnet orders: $ConfirmTestnetOrder"

    Invoke-CheckedTrader @("check-config", "--config", $configPath)
    Invoke-CheckedTrader @(
        "binance-paper-klines",
        "--config",
        $configPath,
        "--symbol",
        $Symbol,
        "--interval",
        $Interval,
        "--limit",
        "$Limit",
        "--format",
        "parquet",
        "--output",
        $barsPath
    )
    Invoke-CheckedTrader @("paper-preflight", "--config", $configPath)
    Invoke-CheckedTrader @("migrate", "--config", $configPath)

    if ($RunPaper) {
        Invoke-CheckedTrader @("paper-run", "--config", $configPath)
        Invoke-CheckedTrader @("report", "--config", $configPath, "--run-id", $runId)
        Invoke-CheckedTrader @("binance-paper-open-orders", "--config", $configPath, "--symbol", $Symbol)
    }

    [pscustomobject]@{
        config = $configPath
        run_id = $runId
        bars = $barsPath
        database = $databaseUrl
        symbol = $Symbol
        interval = $Interval
        limit = $Limit
        run_paper = if ($RunPaper) { "ran" } else { "skipped" }
        order_submit = if ($ConfirmTestnetOrder) { "enabled" } else { "disabled" }
        open_orders_checked = if ($RunPaper) { "ok" } else { "skipped" }
    }
} finally {
    Set-Location $repoRoot
    Remove-Item -LiteralPath $configPath -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $barsPath -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $databasePath -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath "$databasePath-shm" -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath "$databasePath-wal" -Force -ErrorAction SilentlyContinue
}
