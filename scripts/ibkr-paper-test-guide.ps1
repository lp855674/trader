param(
    [ValidateSet("Plan", "DryRun", "ReadOnly", "TinyOrder", "AutoRun", "All")]
    [string]$Stage = "Plan",
    [string]$AccountId = "",
    [string]$GatewayHost = "127.0.0.1",
    [int]$Port = 4002,
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
$traderExe = if ($env:TRADER_TEST_EXE) { $env:TRADER_TEST_EXE } else { Join-Path $repoRoot "target/debug/trader.exe" }
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

function Invoke-TraderCaptured {
    param(
        [string[]]$TraderArgs,
        [string]$LogPath
    )

    $previousErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    $global:LASTEXITCODE = 0
    try {
        if (Test-Path $traderExe) {
            $output = & $traderExe @TraderArgs 2>&1
            $exitCode = $LASTEXITCODE
        } else {
            $output = cargo @("run", "-p", "trader-cli", "--") @TraderArgs 2>&1
            $exitCode = $LASTEXITCODE
        }
    } finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }
    if ($null -eq $exitCode) {
        $exitCode = 0
    }

    $text = $output -join [Environment]::NewLine
    $text | Set-Content -Path $LogPath -Encoding UTF8
    $output | ForEach-Object { Write-Host $_ }

    return [pscustomobject]@{
        exit_code = $exitCode
        output = $text
        log = $LogPath
    }
}

function Get-IbkrFailureClass {
    param(
        [string]$Text,
        [int]$ExitCode,
        [bool]$OpenOrdersFailure = $false
    )

    if ($OpenOrdersFailure) {
        return "open_orders_remaining"
    }
    if ($ExitCode -eq 0) {
        return "ok"
    }
    if ($Text -match "unable to connect to IBKR paper gateway" -or $Text -match "broker connection error" -or $Text -match "connection.*timeout") {
        return "gateway_unreachable"
    }
    if ($Text -match "account.*mismatch" -or $Text -match "account.*not.*returned" -or $Text -match "account.*not.*found") {
        return "account_mismatch"
    }
    return "command_failed"
}

function New-RunId {
    $id = [guid]::NewGuid().ToString("N")
    return $id.Substring(0, 12)
}

function Assert-AccountReady {
    if ($AccountId.Trim().Length -eq 0 -or $AccountId -eq "DU000000") {
        throw "This stage requires a real IBKR paper account id. Pass -AccountId DU..."
    }
}

function Test-GatewayPort {
    param(
        [string]$HostName,
        [int]$PortNumber,
        [int]$TimeoutMilliseconds = 1000
    )

    if ($env:TRADER_TEST_GATEWAY_PORT -eq "reachable") {
        return $true
    }
    if ($env:TRADER_TEST_GATEWAY_PORT -eq "unreachable") {
        return $false
    }

    $client = [System.Net.Sockets.TcpClient]::new()
    try {
        $task = $client.ConnectAsync($HostName, $PortNumber)
        if (-not $task.Wait($TimeoutMilliseconds)) {
            return $false
        }
        return $client.Connected
    } catch {
        return $false
    } finally {
        $client.Dispose()
    }
}

function New-TestConfig {
    param([string]$ConfigDir = $testDir)

    Assert-AccountReady
    New-Item -ItemType Directory -Force -Path $ConfigDir | Out-Null
    $configPath = Join-Path $ConfigDir "config.toml"
    $databasePath = Join-Path $ConfigDir "run.sqlite"
    $databaseUrl = "sqlite://$($databasePath.Replace('\', '/'))"
    $text = Get-Content $baseConfig -Raw
    $text = $text `
        -replace 'run_id = "ibkr-aapl-1d-paper"', 'run_id = "ibkr-paper-test"' `
        -replace 'url = "sqlite://data/ibkr-aapl-1d-paper.sqlite"', "url = `"$databaseUrl`"" `
        -replace 'account_id = "[^"]+"', "account_id = `"$AccountId`"" `
        -replace 'host = "[^"]+"', "host = `"$GatewayHost`"" `
        -replace 'port = \d+', "port = $Port" `
        -replace 'client_id = \d+', "client_id = $ClientId"
    Set-Content -Path $configPath -Value $text -Encoding UTF8
    return $configPath
}

function Write-TestPlan {
    Write-Section "Purpose"
    Write-Host "This script documents and runs the IBKR paper validation flow."
    Write-Host "Default Stage=Plan prints the steps only. It does not connect to IBKR and does not submit orders."

    Write-Section "Prerequisites"
    Write-Host "1. Install and start TWS or IB Gateway in Paper Trading mode."
    Write-Host "2. Enable API socket clients in TWS: Global Configuration -> API -> Settings."
    Write-Host "3. Use the configured IB Gateway paper port, normally 4002."
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
    $runDir = Join-Path $testDir "read-only-$(New-RunId)"
    New-Item -ItemType Directory -Force -Path $runDir | Out-Null
    $summaryPath = Join-Path $runDir "summary.json"
    $startedAt = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
    $config = New-TestConfig -ConfigDir $runDir
    if (-not (Test-GatewayPort -HostName $GatewayHost -PortNumber $Port)) {
        $logPath = Join-Path $runDir "gateway-preflight.log"
        $message = "unable to connect to IBKR paper gateway at $($GatewayHost):$Port"
        $message | Set-Content -Path $logPath -Encoding UTF8
        $completedAt = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
        $summary = [pscustomobject]@{
            stage = "ReadOnly"
            status = "failed"
            failure_class = "gateway_unreachable"
            failed_check = "gateway_preflight"
            account_id = $AccountId
            gateway_host = $GatewayHost
            port = $Port
            client_id = $ClientId
            config = $config
            run_dir = $runDir
            started_at_ms = $startedAt
            completed_at_ms = $completedAt
            checks = @(
                [pscustomobject]@{
                    name = "gateway_preflight"
                    command = "tcp $($GatewayHost):$Port"
                    exit_code = 1
                    status = "failed"
                    failure_class = "gateway_unreachable"
                    log = $logPath
                    output_excerpt = $message
                }
            )
        }
        $summary | ConvertTo-Json -Depth 6 | Set-Content -Path $summaryPath -Encoding UTF8
        Write-Host "IBKR paper read-only summary: $summaryPath"
        throw "IBKR paper read-only failed: gateway_preflight classified as gateway_unreachable; see $summaryPath"
    }
    if (-not $env:TRADER_TEST_EXE) {
        Invoke-CheckedCargo @("build", "-p", "trader-cli")
    }

    $checks = @(
        [pscustomobject]@{ name = "readonly"; args = @("ibkr-paper-readonly", "--config", $config) },
        [pscustomobject]@{ name = "open_orders"; args = @("ibkr-paper-open-orders", "--config", $config) },
        [pscustomobject]@{ name = "executions"; args = @("ibkr-paper-executions", "--config", $config, "--request-id", "1") },
        [pscustomobject]@{ name = "reconcile"; args = @("ibkr-paper-reconcile", "--config", $config, "--request-id", "1") },
        [pscustomobject]@{ name = "recover"; args = @("ibkr-paper-recover", "--config", $config, "--request-id", "1") },
        [pscustomobject]@{ name = "next_order_id"; args = @("ibkr-paper-next-order-id", "--config", $config) }
    )

    $results = @()
    foreach ($check in $checks) {
        $logPath = Join-Path $runDir "$($check.name).log"
        $captured = Invoke-TraderCaptured -TraderArgs $check.args -LogPath $logPath
        $failureClass = Get-IbkrFailureClass -Text $captured.output -ExitCode $captured.exit_code
        $results += [pscustomobject]@{
            name = $check.name
            command = "trader $($check.args -join ' ')"
            exit_code = $captured.exit_code
            status = if ($captured.exit_code -eq 0) { "completed" } else { "failed" }
            failure_class = $failureClass
            log = $captured.log
            output_excerpt = if ($captured.output.Length -gt 500) { $captured.output.Substring(0, 500) } else { $captured.output }
        }
        if ($captured.exit_code -ne 0) {
            break
        }
    }

    $failedCheck = $results | Where-Object { $_.exit_code -ne 0 } | Select-Object -First 1
    $completedAt = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
    $summary = [pscustomobject]@{
        stage = "ReadOnly"
        status = if ($null -eq $failedCheck) { "completed" } else { "failed" }
        failure_class = if ($null -eq $failedCheck) { "ok" } else { $failedCheck.failure_class }
        failed_check = if ($null -eq $failedCheck) { "" } else { $failedCheck.name }
        account_id = $AccountId
        gateway_host = $GatewayHost
        port = $Port
        client_id = $ClientId
        config = $config
        run_dir = $runDir
        started_at_ms = $startedAt
        completed_at_ms = $completedAt
        checks = $results
    }
    $summary | ConvertTo-Json -Depth 6 | Set-Content -Path $summaryPath -Encoding UTF8
    Write-Host "IBKR paper read-only summary: $summaryPath"

    if ($null -ne $failedCheck) {
        throw "IBKR paper read-only failed: $($failedCheck.name) classified as $($failedCheck.failure_class); see $summaryPath"
    }
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
