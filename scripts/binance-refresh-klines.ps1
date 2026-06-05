param(
    [string]$Config = "configs/paper/binance_btcusdt_1m_parquet.toml",
    [string]$Symbol = "BTCUSDT",
    [string]$Interval = "1m",
    [int]$Limit = 1000,
    [string]$Output = "datasets/binance/btcusdt_1m.parquet"
)

$ErrorActionPreference = "Stop"

if ($Limit -lt 1 -or $Limit -gt 1000) {
    throw "Limit must be between 1 and 1000"
}

$repoRoot = Get-Location
$traderExe = Join-Path $repoRoot "target/debug/trader.exe"

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

    Write-Host "Binance klines config: $Config"
    Write-Host "Binance klines symbol: $Symbol"
    Write-Host "Binance klines interval: $Interval"
    Write-Host "Binance klines limit: $Limit"
    Write-Host "Binance klines output: $Output"

    Invoke-CheckedTrader @("check-config", "--config", $Config)
    Invoke-CheckedTrader @(
        "binance-paper-klines",
        "--config",
        $Config,
        "--symbol",
        $Symbol,
        "--interval",
        $Interval,
        "--limit",
        "$Limit",
        "--format",
        "parquet",
        "--output",
        $Output
    )
    Invoke-CheckedTrader @("paper-preflight", "--config", $Config)

    [pscustomobject]@{
        config = $Config
        symbol = $Symbol
        interval = $Interval
        limit = $Limit
        output = $Output
        format = "parquet"
        preflight = "ok"
    }
} finally {
    Set-Location $repoRoot
}
