param(
    [ValidateSet("Plan", "DryRun", "ReadOnly", "TinyOrder", "AutoRun", "All")]
    [string]$Stage = "Plan",
    [string]$AccountId = "",
    [string]$GatewayHost = "127.0.0.1",
    [int]$Port = 7497,
    [int]$ClientId = 1,
    [string]$Symbol = "AAPL",
    [string]$Side = "buy",
    [string]$Qty = "1",
    [string]$Price = "185.25",
    [switch]$ConfirmTinyOrder,
    [switch]$ConfirmAutoRun,
    [switch]$SkipRefresh
)

$ErrorActionPreference = "Stop"

$repoRoot = Get-Location
$traderExe = Join-Path $repoRoot "target/debug/trader.exe"
$baseConfig = "configs/paper/ibkr_aapl_1d_parquet.toml"
$testDir = Join-Path $repoRoot "data/ibkr-paper-test"
$testConfig = Join-Path $testDir "config.toml"

function Write-Section {
    param([string]$Title)
    Write-Host ""
    Write-Host "== $Title =="
}

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

function Assert-AccountReady {
    if ($AccountId.Trim().Length -eq 0 -or $AccountId -eq "DU000000") {
        throw "This stage requires a real IBKR paper account id. Pass -AccountId DU..."
    }
}

function New-TestConfig {
    Assert-AccountReady
    New-Item -ItemType Directory -Force -Path $testDir | Out-Null
    $databasePath = Join-Path $testDir "run.sqlite"
    $databaseUrl = "sqlite://$($databasePath.Replace('\', '/'))"
    $text = Get-Content $baseConfig -Raw
    $text = $text `
        -replace 'run_id = "ibkr-aapl-1d-paper"', 'run_id = "ibkr-paper-test"' `
        -replace 'url = "sqlite://data/ibkr-aapl-1d-paper.sqlite"', "url = `"$databaseUrl`"" `
        -replace 'account_id = "[^"]+"', "account_id = `"$AccountId`"" `
        -replace 'host = "[^"]+"', "host = `"$GatewayHost`"" `
        -replace 'port = \d+', "port = $Port" `
        -replace 'client_id = \d+', "client_id = $ClientId"
    Set-Content -Path $testConfig -Value $text -Encoding UTF8
    return $testConfig
}

function Write-TestPlan {
    Write-Section "Purpose"
    Write-Host "This script documents and runs the IBKR paper validation flow."
    Write-Host "Default Stage=Plan prints the steps only. It does not connect to IBKR and does not submit orders."

    Write-Section "Prerequisites"
    Write-Host "1. Install and start TWS or IB Gateway in Paper Trading mode."
    Write-Host "2. Enable API socket clients in TWS: Global Configuration -> API -> Settings."
    Write-Host "3. Use the paper port, normally 7497."
    Write-Host "4. Find the paper account id, normally DU..., and pass it with -AccountId."
    Write-Host "5. Keep this account id out of committed config files; pass it as a script parameter."

    Write-Section "Commands"
    Write-Host "Local dry-run, no IBKR connection:"
    Write-Host "powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-test-guide.ps1 -Stage DryRun"
    Write-Host ""
    Write-Host "Read-only Gateway validation:"
    Write-Host "powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-test-guide.ps1 -Stage ReadOnly -AccountId DU..."
    Write-Host ""
    Write-Host "Manual tiny paper order. This submits a real IBKR paper LMT order:"
    Write-Host "powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-test-guide.ps1 -Stage TinyOrder -AccountId DU... -ConfirmTinyOrder"
    Write-Host ""
    Write-Host "Automatic paper-run order path. This enables order_submit_enabled only in a generated run config:"
    Write-Host "powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-test-guide.ps1 -Stage AutoRun -AccountId DU... -ConfirmAutoRun"
    Write-Host ""
    Write-Host "Multi-iteration soak. Default is local-only; add AccountId and confirmation after Gateway is ready:"
    Write-Host "powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-soak.ps1 -Iterations 3 -SkipRefresh"
    Write-Host "powershell -ExecutionPolicy Bypass -File .\scripts\ibkr-paper-soak.ps1 -Iterations 3 -AccountId DU... -ConfirmIbkrPaperOrder"

    Write-Section "Expected Results"
    Write-Host "DryRun: order_submit=disabled, reports and summary are generated under data/ibkr-paper-runs/."
    Write-Host "ReadOnly: ibkr-paper-readonly prints connected=true and account=<your DU account>."
    Write-Host "ReadOnly also runs ibkr-paper-reconcile and prints local/remote order and execution match counts."
    Write-Host "ReadOnly also runs ibkr-paper-recover, which only updates local recoverable orders if any exist."
    Write-Host "TinyOrder: ibkr-paper-tiny-order prints order_id and status from Gateway."
    Write-Host "AutoRun: paper-preflight prints real_broker_connection=true, runner summary has order_submit=enabled."

    Write-Section "Safety"
    Write-Host "TinyOrder and AutoRun require explicit confirmation switches."
    Write-Host "The project only writes fills from real executions; no execution means no fake fill."
    Write-Host "If an auto-run order has no execution and remains open, the executor attempts to cancel it."
}

function Invoke-DryRun {
    Write-Section "DryRun"
    $args = @("-ExecutionPolicy", "Bypass", "-File", ".\scripts\ibkr-paper-run.ps1")
    if ($SkipRefresh) {
        $args += "-SkipRefresh"
    }
    powershell @args
    if ($LASTEXITCODE -ne 0) {
        throw "ibkr-paper-run.ps1 dry-run failed with exit code $LASTEXITCODE"
    }
}

function Invoke-ReadOnly {
    Write-Section "ReadOnly"
    $config = New-TestConfig
    Invoke-CheckedCargo @("build", "-p", "trader-cli")
    Invoke-CheckedTrader @("ibkr-paper-readonly", "--config", $config)
    Invoke-CheckedTrader @("ibkr-paper-open-orders", "--config", $config)
    Invoke-CheckedTrader @("ibkr-paper-executions", "--config", $config, "--request-id", "1")
    Invoke-CheckedTrader @("ibkr-paper-reconcile", "--config", $config, "--request-id", "1")
    Invoke-CheckedTrader @("ibkr-paper-recover", "--config", $config, "--request-id", "1")
    Invoke-CheckedTrader @("ibkr-paper-next-order-id", "--config", $config)
}

function Invoke-TinyOrder {
    Write-Section "TinyOrder"
    if (-not $ConfirmTinyOrder) {
        throw "TinyOrder submits a real IBKR paper order. Re-run with -ConfirmTinyOrder."
    }
    $config = New-TestConfig
    Invoke-CheckedCargo @("build", "-p", "trader-cli")
    Invoke-CheckedTrader @(
        "ibkr-paper-tiny-order",
        "--config",
        $config,
        "--symbol",
        $Symbol,
        "--side",
        $Side,
        "--qty",
        $Qty,
        "--price",
        $Price,
        "--confirm-ibkr-paper-order"
    )
    Write-Host "If Gateway leaves the order open, cancel it with:"
    Write-Host "cargo run -p trader-cli -- ibkr-paper-cancel-order --config $config --order-id <ORDER_ID> --confirm-ibkr-paper-cancel"
}

function Invoke-AutoRun {
    Write-Section "AutoRun"
    if (-not $ConfirmAutoRun) {
        throw "AutoRun enables strategy order submission to IBKR paper. Re-run with -ConfirmAutoRun."
    }
    Assert-AccountReady
    $args = @(
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        ".\scripts\ibkr-paper-run.ps1",
        "-AccountId",
        $AccountId,
        "-GatewayHost",
        $GatewayHost,
        "-Port",
        $Port,
        "-ClientId",
        $ClientId,
        "-ConfirmIbkrPaperOrder"
    )
    if ($SkipRefresh) {
        $args += "-SkipRefresh"
    }
    powershell @args
    if ($LASTEXITCODE -ne 0) {
        throw "ibkr-paper-run.ps1 auto-run failed with exit code $LASTEXITCODE"
    }
}

try {
    switch ($Stage) {
        "Plan" {
            Write-TestPlan
        }
        "DryRun" {
            Invoke-DryRun
        }
        "ReadOnly" {
            Invoke-ReadOnly
        }
        "TinyOrder" {
            Invoke-TinyOrder
        }
        "AutoRun" {
            Invoke-AutoRun
        }
        "All" {
            Invoke-DryRun
            Invoke-ReadOnly
            Invoke-TinyOrder
            Invoke-AutoRun
        }
    }
} finally {
    Set-Location $repoRoot
}
