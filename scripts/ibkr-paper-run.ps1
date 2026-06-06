param(
    [string]$Config = "configs/paper/ibkr_aapl_1d_parquet.toml",
    [string]$InputCsv = "datasets/sample/aapl_1d.csv",
    [string]$OutputParquet = "datasets/ibkr/aapl_1d.parquet",
    [switch]$SkipRefresh,
    [switch]$ConfirmIbkrPaperOrder,
    [string]$AccountId = "",
    [string]$GatewayHost = "",
    [int]$Port = 0,
    [int]$ClientId = 0
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
$summaryPath = Join-Path $runDir "summary.json"
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

function Invoke-CapturedTrader {
    param([string[]]$TraderArgs)

    if (Test-Path $traderExe) {
        $output = & $traderExe @TraderArgs 2>&1
        if ($LASTEXITCODE -ne 0) {
            throw "trader $($TraderArgs -join ' ') failed with exit code $LASTEXITCODE"
        }
    } else {
        $output = cargo @(@("run", "-p", "trader-cli", "--") + $TraderArgs) 2>&1
        if ($LASTEXITCODE -ne 0) {
            throw "cargo run -p trader-cli -- $($TraderArgs -join ' ') failed with exit code $LASTEXITCODE"
        }
    }

    $output | ForEach-Object { Write-Host $_ }
    return ($output -join [Environment]::NewLine)
}

function Get-IbkrAccountId {
    param([string]$ConfigText)

    if ($ConfigText -match 'account_id\s*=\s*"([^"]+)"') {
        return $Matches[1]
    }
    return ""
}

function Invoke-IbkrPaperGatewayChecks {
    Write-Host "Running IBKR paper Gateway checks"
    $readonlyOutput = ""
    $openOrdersOutput = ""
    $executionsOutput = ""

    try {
        $readonlyOutput = Invoke-CapturedTrader @("ibkr-paper-readonly", "--config", $runConfigPath)
    } catch {
        Write-Warning "ibkr-paper-readonly failed during checks: $_"
        $readonlyOutput = "failed: $_"
    }

    try {
        $openOrdersOutput = Invoke-CapturedTrader @("ibkr-paper-open-orders", "--config", $runConfigPath)
    } catch {
        Write-Warning "ibkr-paper-open-orders failed during checks: $_"
        $openOrdersOutput = "failed: $_"
    }

    try {
        $executionsOutput = Invoke-CapturedTrader @("ibkr-paper-executions", "--config", $runConfigPath, "--request-id", "1")
    } catch {
        Write-Warning "ibkr-paper-executions failed during checks: $_"
        $executionsOutput = "failed: $_"
    }

    [pscustomobject]@{
        readonly = $readonlyOutput
        open_orders = $openOrdersOutput
        executions = $executionsOutput
    }
}

try {
    $env:CARGO_BUILD_JOBS = "1"
    Invoke-CheckedCargo @("build", "-p", "trader-cli")

    New-Item -ItemType Directory -Force -Path $runDir | Out-Null
    New-Item -ItemType Directory -Force -Path (Split-Path $OutputParquet -Parent) | Out-Null

    $configText = Get-Content $Config -Raw
    if ($AccountId.Trim().Length -gt 0) {
        $configText = $configText -replace 'account_id = "[^"]+"', "account_id = `"$AccountId`""
    }
    if ($GatewayHost.Trim().Length -gt 0) {
        $configText = $configText -replace 'host = "[^"]+"', "host = `"$GatewayHost`""
    }
    if ($Port -gt 0) {
        $configText = $configText -replace 'port = \d+', "port = $Port"
    }
    if ($ClientId -gt 0) {
        $configText = $configText -replace 'client_id = \d+', "client_id = $ClientId"
    }

    $effectiveAccountId = Get-IbkrAccountId $configText
    if ($ConfirmIbkrPaperOrder -and ($effectiveAccountId.Length -eq 0 -or $effectiveAccountId -eq "DU000000")) {
        throw "ConfirmIbkrPaperOrder requires a real IBKR paper account id; pass -AccountId DU... or update the config"
    }

    $runConfigText = $configText `
        -replace 'run_id = "ibkr-aapl-1d-paper"', "run_id = `"$runId`"" `
        -replace 'url = "sqlite://data/ibkr-aapl-1d-paper.sqlite"', "url = `"$databaseUrl`""
    if ($ConfirmIbkrPaperOrder) {
        $runConfigText = $runConfigText -replace 'order_submit_enabled = false', 'order_submit_enabled = true'
    }
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
    Write-Host "IBKR paper account: $effectiveAccountId"
    Write-Host "Submit IBKR paper orders: $ConfirmIbkrPaperOrder"

    Invoke-CheckedTrader @("check-config", "--config", $runConfigPath)
    Invoke-CheckedTrader @("paper-preflight", "--config", $runConfigPath)
    Invoke-CheckedTrader @("migrate", "--config", $runConfigPath)
    try {
        Invoke-CheckedTrader @("paper-run", "--config", $runConfigPath)
    } catch {
        if ($ConfirmIbkrPaperOrder) {
            Invoke-IbkrPaperGatewayChecks
        }
        throw
    }
    Invoke-CheckedTrader @("report", "--config", $runConfigPath)
    Invoke-CheckedTrader @("report", "--config", $runConfigPath, "--format", "text", "--output", $textReportPath)
    Invoke-CheckedTrader @("report", "--config", $runConfigPath, "--format", "csv", "--output", $csvReportPath)
    Invoke-CheckedTrader @("report", "--config", $runConfigPath, "--format", "html", "--output", $htmlReportPath)
    $gatewayChecks = if ($ConfirmIbkrPaperOrder) { Invoke-IbkrPaperGatewayChecks } else { $null }

    $summary = [pscustomobject]@{
        run_id = $runId
        config = $runConfigPath
        database = $databaseUrl
        parquet = $OutputParquet
        account_id = $effectiveAccountId
        reports = [pscustomobject]@{
            text = $textReportPath
            csv = $csvReportPath
            html = $htmlReportPath
        }
        refreshed = if ($SkipRefresh) { "skipped" } else { "ok" }
        order_submit = if ($ConfirmIbkrPaperOrder) { "enabled" } else { "disabled" }
        gateway_checks = $gatewayChecks
    }
    $summary | ConvertTo-Json -Depth 5 | Set-Content -Path $summaryPath -Encoding UTF8

    [pscustomobject]@{
        run_id = $runId
        config = $runConfigPath
        database = $databaseUrl
        parquet = $OutputParquet
        report_text = $textReportPath
        report_csv = $csvReportPath
        report_html = $htmlReportPath
        summary = $summaryPath
        account_id = $effectiveAccountId
        refreshed = if ($SkipRefresh) { "skipped" } else { "ok" }
        order_submit = if ($ConfirmIbkrPaperOrder) { "enabled" } else { "disabled" }
    }
} finally {
    Set-Location $repoRoot
}
