$ErrorActionPreference = "Stop"

$repoRoot = Get-Location
$databasePath = Join-Path $env:TEMP ("trader-mvp-{0}.sqlite" -f [guid]::NewGuid().ToString("N"))
$configPath = Join-Path $env:TEMP ("trader-mvp-{0}.toml" -f [guid]::NewGuid().ToString("N"))
$databaseUrl = "sqlite://$($databasePath.Replace('\', '/'))"

$template = Get-Content "configs/backtest/ma_cross.toml" -Raw
$config = $template -replace 'url = "sqlite://data/trader.sqlite"', "url = `"$databaseUrl`""
Set-Content -Path $configPath -Value $config -Encoding UTF8

try {
    Write-Host "MVP config: $configPath"
    Write-Host "MVP database: $databaseUrl"

    cargo run -p trader-cli -- check-config --config $configPath
    cargo run -p trader-cli -- migrate --config $configPath
    cargo run -p trader-cli -- backtest --config $configPath
    cargo run -p trader-cli -- paper-run --config $configPath
    cargo run -p trader-cli -- replay --config $configPath
    cargo run -p trader-cli -- report --config $configPath

    $env:TRADER_CONFIG = $configPath
    powershell -ExecutionPolicy Bypass -File ".\scripts\server-smoke.ps1"
} finally {
    Set-Location $repoRoot
    Remove-Item -LiteralPath $configPath -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $databasePath -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath "$databasePath-shm" -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath "$databasePath-wal" -Force -ErrorAction SilentlyContinue
}
