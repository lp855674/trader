param(
    [string]$Config = "configs/paper/binance_testnet.toml",
    [switch]$ConfirmTestnetOrder
)

$ErrorActionPreference = "Stop"

if (-not $ConfirmTestnetOrder) {
    throw "refusing to submit Binance testnet strategy order without -ConfirmTestnetOrder"
}

$repoRoot = Get-Location
$traderExe = Join-Path $repoRoot "target/debug/trader.exe"
$id = [guid]::NewGuid().ToString("N")
$databasePath = Join-Path $env:TEMP "trader-binance-paper-auto-$id.sqlite"
$configPath = Join-Path $env:TEMP "trader-binance-paper-auto-$id.toml"
$barsPath = Join-Path $env:TEMP "trader-binance-paper-auto-$id.csv"
$databaseUrl = "sqlite://$($databasePath.Replace('\', '/'))"
$barsConfigPath = $barsPath.Replace('\', '/')

function Invoke-CheckedTrader {
    param([string[]]$TraderArgs)

    if (Test-Path $traderExe) {
        & $traderExe @TraderArgs
        if ($LASTEXITCODE -ne 0) {
            throw "trader $($TraderArgs -join ' ') failed with exit code $LASTEXITCODE"
        }
    } else {
        $cargoArgs = @("run", "-p", "trader-cli", "--") + $TraderArgs
        cargo @cargoArgs
        if ($LASTEXITCODE -ne 0) {
            throw "cargo run -p trader-cli -- $($TraderArgs -join ' ') failed with exit code $LASTEXITCODE"
        }
    }
}

try {
    $ticker = Invoke-RestMethod -Uri "https://testnet.binance.vision/api/v3/ticker/price?symbol=BTCUSDT"
    $price = [decimal]$ticker.price
    $bar1 = [math]::Round($price * [decimal]0.99, 2)
    $bar2 = [math]::Round($price, 2)
    $bar3 = [math]::Round($price * [decimal]1.01, 2)

    @"
ts_ms,open,high,low,close,volume
1704067200000,$bar1,$bar1,$bar1,$bar1,1
1704153600000,$bar2,$bar2,$bar2,$bar2,1
1704240000000,$bar3,$bar3,$bar3,$bar3,1
"@ | Set-Content -Path $barsPath -Encoding UTF8

    $template = Get-Content $Config -Raw
    $configText = $template `
        -replace 'run_id = "binance-testnet-readonly"', "run_id = `"binance-paper-auto-$id`"" `
        -replace 'url = "sqlite://data/binance-testnet.sqlite"', "url = `"$databaseUrl`"" `
        -replace 'path = "datasets/sample/aapl_1d.csv"', "path = `"$barsConfigPath`"" `
        -replace 'max_order_notional = "50"', 'max_order_notional = "200"' `
        -replace 'order_submit_enabled = false', 'order_submit_enabled = true'
    Set-Content -Path $configPath -Value $configText -Encoding UTF8

    $env:CARGO_BUILD_JOBS = "1"
    Write-Host "Binance auto paper config: $configPath"
    Write-Host "Binance auto paper bars: $barsPath"
    Write-Host "Binance auto paper database: $databaseUrl"
    Write-Host "BTCUSDT ticker price: $price"
    Write-Host "Generated closes: $bar1, $bar2, $bar3"

    Invoke-CheckedTrader @("check-config", "--config", $configPath)
    Invoke-CheckedTrader @("paper-preflight", "--config", $configPath)
    Invoke-CheckedTrader @("migrate", "--config", $configPath)
    Invoke-CheckedTrader @("paper-run", "--config", $configPath)
    Invoke-CheckedTrader @("report", "--config", $configPath)

    [pscustomobject]@{
        config = $configPath
        bars = $barsPath
        database = $databaseUrl
        ticker_price = $price
        order_submit = "ran"
    }
} finally {
    Set-Location $repoRoot
    Remove-Item -LiteralPath $configPath -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $barsPath -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $databasePath -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath "$databasePath-shm" -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath "$databasePath-wal" -Force -ErrorAction SilentlyContinue
}
