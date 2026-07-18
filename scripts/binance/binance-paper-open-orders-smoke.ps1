param(
    [string]$Config = "configs/paper/binance_testnet.toml",
    [string]$Symbol = "BTCUSDT",
    [switch]$SkipNetwork
)

$ErrorActionPreference = "Stop"

$repoRoot = Get-Location
$traderExe = Join-Path $repoRoot "target/debug/trader.exe"
$id = [guid]::NewGuid().ToString("N")
$databasePath = Join-Path $env:TEMP "trader-binance-paper-open-orders-$id.sqlite"
$configPath = Join-Path $env:TEMP "trader-binance-paper-open-orders-$id.toml"
$databaseUrl = "sqlite://$($databasePath.Replace('\', '/'))"

$template = Get-Content $Config -Raw
$configText = $template `
    -replace 'run_id = "binance-testnet-readonly"', "run_id = `"binance-paper-open-orders-$id`"" `
    -replace 'url = "sqlite://data/binance/databases/binance-testnet.sqlite"', "url = `"$databaseUrl`""
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

    Write-Host "Binance open orders config: $configPath"
    Write-Host "Binance open orders database: $databaseUrl"
    Write-Host "Binance open orders symbol: $Symbol"
    Write-Host "Network open orders check: $(-not $SkipNetwork)"

    Invoke-CheckedTrader @("check-config", "--config", $configPath)
    Invoke-CheckedTrader @("paper-preflight", "--config", $configPath)
    Invoke-CheckedTrader @("migrate", "--config", $configPath)

    if (-not $SkipNetwork) {
        Invoke-CheckedTrader @("binance-paper-open-orders", "--config", $configPath, "--symbol", $Symbol)
    }

    [pscustomobject]@{
        config = $configPath
        database = $databaseUrl
        symbol = $Symbol
        preflight = "ok"
        migrated = "ok"
        open_orders_network = if ($SkipNetwork) { "skipped" } else { "ok" }
        order_submit = "not_run"
    }
} finally {
    Set-Location $repoRoot
    Remove-Item -LiteralPath $configPath -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $databasePath -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath "$databasePath-shm" -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath "$databasePath-wal" -Force -ErrorAction SilentlyContinue
}
