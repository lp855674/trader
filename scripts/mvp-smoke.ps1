$ErrorActionPreference = "Stop"

$repoRoot = Get-Location
$databasePath = Join-Path $env:TEMP ("trader-mvp-{0}.sqlite" -f [guid]::NewGuid().ToString("N"))
$configPath = Join-Path $env:TEMP ("trader-mvp-{0}.toml" -f [guid]::NewGuid().ToString("N"))
$serverConfigPath = Join-Path $env:TEMP ("trader-mvp-server-{0}.toml" -f [guid]::NewGuid().ToString("N"))
$targetDir = Join-Path $env:TEMP ("trader-mvp-target-{0}" -f [guid]::NewGuid().ToString("N"))
$databaseUrl = "sqlite://$($databasePath.Replace('\', '/'))"

$template = Get-Content "configs/backtest/ma_cross.toml" -Raw
$config = $template -replace 'url = "sqlite://data/trader.sqlite"', "url = `"$databaseUrl`""
Set-Content -Path $configPath -Value $config -Encoding UTF8
$serverConfig = @"
[database]
url = "$databaseUrl"

[server]
bind = "127.0.0.1:8080"

[logging]
enabled = true
level = "info"

[run_defaults]
config_path = "$($configPath.Replace('\', '/'))"
"@
Set-Content -Path $serverConfigPath -Value $serverConfig -Encoding UTF8

function Invoke-CheckedCargo {
    param([string[]]$CargoArgs)

    cargo @CargoArgs
    if ($LASTEXITCODE -ne 0) {
        throw "cargo $($CargoArgs -join ' ') failed with exit code $LASTEXITCODE"
    }
}

try {
    $env:CARGO_TARGET_DIR = $targetDir
    $env:TRADER_SMOKE_TARGET_DIR = $targetDir
    Write-Host "MVP config: $configPath"
    Write-Host "MVP database: $databaseUrl"
    Write-Host "MVP target: $targetDir"

    Invoke-CheckedCargo @("run", "-p", "trader-cli", "--", "check-config", "--config", $configPath)
    Invoke-CheckedCargo @("run", "-p", "trader-cli", "--", "migrate", "--config", $configPath)
    Invoke-CheckedCargo @("run", "-p", "trader-cli", "--", "backtest", "--config", $configPath)
    Invoke-CheckedCargo @("run", "-p", "trader-cli", "--", "paper-run", "--config", $configPath)
    Invoke-CheckedCargo @("run", "-p", "trader-cli", "--", "replay", "--config", $configPath)
    Invoke-CheckedCargo @("run", "-p", "trader-cli", "--", "report", "--config", $configPath)

    $env:TRADER_CONFIG = $serverConfigPath
    powershell -ExecutionPolicy Bypass -File ".\scripts\server-smoke.ps1"
    if ($LASTEXITCODE -ne 0) {
        throw "server smoke failed with exit code $LASTEXITCODE"
    }
} finally {
    Set-Location $repoRoot
    Remove-Item -LiteralPath $configPath -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $serverConfigPath -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $databasePath -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath "$databasePath-shm" -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath "$databasePath-wal" -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $targetDir -Recurse -Force -ErrorAction SilentlyContinue
}
