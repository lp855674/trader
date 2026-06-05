param(
    [string]$Config = "configs/paper/ibkr_aapl_1d_parquet.toml",
    [string]$InputCsv = "datasets/sample/aapl_1d.csv",
    [string]$OutputParquet = "datasets/ibkr/aapl_1d.parquet",
    [switch]$SkipRefresh
)

$ErrorActionPreference = "Stop"

$repoRoot = Get-Location
$traderExe = Join-Path $repoRoot "target/debug/trader.exe"
$id = [guid]::NewGuid().ToString("N")
$runId = "ibkr-aapl-1d-$($id.Substring(0, 12))"
$runDir = Join-Path $repoRoot "data/ibkr-paper-runs/$runId"
$runConfigPath = Join-Path $runDir "config.toml"
$databasePath = Join-Path $runDir "run.sqlite"
$databaseUrl = "sqlite://$($databasePath.Replace('\', '/'))"
$textReportPath = Join-Path $runDir "report.txt"
$csvReportPath = Join-Path $runDir "report.csv"
$htmlReportPath = Join-Path $runDir "report.html"
$refreshConfigPath = Join-Path $runDir "refresh-config.toml"

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

    New-Item -ItemType Directory -Force -Path $runDir | Out-Null
    New-Item -ItemType Directory -Force -Path (Split-Path $OutputParquet -Parent) | Out-Null

    $configText = Get-Content $Config -Raw
    $runConfigText = $configText `
        -replace 'run_id = "ibkr-aapl-1d-paper"', "run_id = `"$runId`"" `
        -replace 'url = "sqlite://data/ibkr-aapl-1d-paper.sqlite"', "url = `"$databaseUrl`""
    Set-Content -Path $runConfigPath -Value $runConfigText -Encoding UTF8

    if (-not $SkipRefresh) {
        $refreshConfigText = $runConfigText `
            -replace 'source = "parquet"', 'source = "csv"' `
            -replace 'path = "datasets/ibkr/aapl_1d.parquet"', "path = `"$($InputCsv.Replace('\', '/'))`""
        Set-Content -Path $refreshConfigPath -Value $refreshConfigText -Encoding UTF8
        Invoke-CheckedTrader @("import-bars", "--config", $refreshConfigPath, "--output-parquet", $OutputParquet)
    }

    Write-Host "IBKR stock paper run id: $runId"
    Write-Host "IBKR stock paper run config: $runConfigPath"
    Write-Host "IBKR stock paper database: $databaseUrl"
    Write-Host "IBKR stock paper parquet: $OutputParquet"
    Write-Host "IBKR stock paper refresh: $(-not $SkipRefresh)"
    Write-Host "Submit IBKR paper orders: False"

    Invoke-CheckedTrader @("check-config", "--config", $runConfigPath)
    Invoke-CheckedTrader @("paper-preflight", "--config", $runConfigPath)
    Invoke-CheckedTrader @("migrate", "--config", $runConfigPath)
    Invoke-CheckedTrader @("paper-run", "--config", $runConfigPath)
    Invoke-CheckedTrader @("report", "--config", $runConfigPath)
    Invoke-CheckedTrader @("report", "--config", $runConfigPath, "--format", "text", "--output", $textReportPath)
    Invoke-CheckedTrader @("report", "--config", $runConfigPath, "--format", "csv", "--output", $csvReportPath)
    Invoke-CheckedTrader @("report", "--config", $runConfigPath, "--format", "html", "--output", $htmlReportPath)

    [pscustomobject]@{
        run_id = $runId
        config = $runConfigPath
        database = $databaseUrl
        parquet = $OutputParquet
        report_text = $textReportPath
        report_csv = $csvReportPath
        report_html = $htmlReportPath
        refreshed = if ($SkipRefresh) { "skipped" } else { "ok" }
        order_submit = "disabled"
    }
} finally {
    Set-Location $repoRoot
}
