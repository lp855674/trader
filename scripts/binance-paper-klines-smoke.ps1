param(
    [string]$Config = "configs/paper/binance_testnet.toml",
    [string]$Symbol = "BTCUSDT",
    [string]$Interval = "1m",
    [int]$Limit = 5
)

$ErrorActionPreference = "Stop"

if ($Limit -lt 1 -or $Limit -gt 1000) {
    throw "Limit must be between 1 and 1000"
}

$repoRoot = Get-Location
$traderExe = Join-Path $repoRoot "target/debug/trader.exe"
$id = [guid]::NewGuid().ToString("N")
$databasePath = Join-Path $env:TEMP "trader-binance-paper-klines-$id.sqlite"
$configPath = Join-Path $env:TEMP "trader-binance-paper-klines-$id.toml"
$barsPath = Join-Path $env:TEMP "trader-binance-paper-klines-$id.parquet"
$databaseUrl = "sqlite://$($databasePath.Replace('\', '/'))"
$barsConfigPath = $barsPath.Replace('\', '/')

$template = Get-Content $Config -Raw
$configText = $template `
    -replace 'run_id = "binance-testnet-readonly"', "run_id = `"binance-paper-klines-$id`"" `
    -replace 'url = "sqlite://data/binance-testnet.sqlite"', "url = `"$databaseUrl`"" `
    -replace 'source = "csv"', 'source = "parquet"' `
    -replace 'path = "datasets/sample/aapl_1d.csv"', "path = `"$barsConfigPath`""
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

    Write-Host "Binance klines config: $configPath"
    Write-Host "Binance klines output: $barsPath"
    Write-Host "Binance klines database: $databaseUrl"
    Write-Host "Binance klines symbol: $Symbol"
    Write-Host "Binance klines interval: $Interval"
    Write-Host "Binance klines limit: $Limit"

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

    [pscustomobject]@{
        config = $configPath
        bars = $barsPath
        database = $databaseUrl
        symbol = $Symbol
        interval = $Interval
        limit = $Limit
        preflight = "ok"
        order_submit = "not_run"
    }
} finally {
    Set-Location $repoRoot
    Remove-Item -LiteralPath $configPath -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $barsPath -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $databasePath -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath "$databasePath-shm" -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath "$databasePath-wal" -Force -ErrorAction SilentlyContinue
}
