param(
    [string]$Config = "configs/paper/ibkr_aapl_1d_parquet.toml",
    [string]$InputCsv = "datasets/sample/aapl_1d.csv",
    [string]$OutputParquet = "datasets/ibkr/aapl_1d.parquet",
    [switch]$SkipRefresh,
    [switch]$ReadOnly,
    [switch]$ConfirmIbkrPaperOrder,
    [string]$AccountId = "",
    [string]$GatewayHost = "",
    [int]$Port = 0,
    [int]$ClientId = 0,
    [string]$IbkrRouteExchange = "",
    [switch]$IbkrOverridePercentageConstraints,
    [int]$OpenOrdersSettleSeconds = 30,
    [int]$OpenOrdersPollSeconds = 2
)

$ErrorActionPreference = "Stop"

$repoRoot = Get-Location
$traderExe = if ($env:TRADER_TEST_EXE) { $env:TRADER_TEST_EXE } else { Join-Path $repoRoot "target/debug/trader.exe" }
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

    $global:LASTEXITCODE = 0
    if (Test-Path $traderExe) {
        & $traderExe @TraderArgs
        $exitCode = if ($null -eq $LASTEXITCODE) { 0 } else { $LASTEXITCODE }
        if ($exitCode -ne 0) {
            throw "trader $($TraderArgs -join ' ') failed with exit code $LASTEXITCODE"
        }
    } else {
        Invoke-CheckedCargo (@("run", "-p", "trader-cli", "--") + $TraderArgs)
    }
}

function Invoke-CapturedTrader {
    param([string[]]$TraderArgs)

    $global:LASTEXITCODE = 0
    if (Test-Path $traderExe) {
        $output = & $traderExe @TraderArgs 2>&1
        $exitCode = if ($null -eq $LASTEXITCODE) { 0 } else { $LASTEXITCODE }
        if ($exitCode -ne 0) {
            throw "trader $($TraderArgs -join ' ') failed with exit code $LASTEXITCODE"
        }
    } else {
        $output = cargo @(@("run", "-p", "trader-cli", "--") + $TraderArgs) 2>&1
        $exitCode = if ($null -eq $LASTEXITCODE) { 0 } else { $LASTEXITCODE }
        if ($exitCode -ne 0) {
            throw "cargo run -p trader-cli -- $($TraderArgs -join ' ') failed with exit code $LASTEXITCODE"
        }
    }

    $output | ForEach-Object { Write-Host $_ }
    return ($output -join [Environment]::NewLine)
}

function Invoke-CapturedTraderCheck {
    param([string[]]$TraderArgs)

    $previousErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    $global:LASTEXITCODE = 0
    try {
        if (Test-Path $traderExe) {
            $output = & $traderExe @TraderArgs 2>&1
            $exitCode = $LASTEXITCODE
        } else {
            $output = cargo @(@("run", "-p", "trader-cli", "--") + $TraderArgs) 2>&1
            $exitCode = $LASTEXITCODE
        }
    } finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }
    if ($null -eq $exitCode) {
        $exitCode = 0
    }

    $output | ForEach-Object { Write-Host $_ }
    return [pscustomobject]@{
        exit_code = $exitCode
        output = ($output -join [Environment]::NewLine)
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

function Test-OpenOrdersRemaining {
    param([string]$Text)

    $match = [regex]::Match($Text, 'open_orders=(\d+)')
    return ($match.Success -and [int]$match.Groups[1].Value -gt 0)
}

function Get-OpenOrdersCount {
    param([string]$Text)

    $match = [regex]::Match($Text, 'open_orders=(\d+)')
    if ($match.Success) {
        return [int]$match.Groups[1].Value
    }
    return 0
}

function Get-RiskRejections {
    param([string]$Text)

    $events = @()
    foreach ($line in ($Text -split "`r?`n")) {
        if ($line -match '^risk_event:\s+run_id=(\S+)\s+ts_ms=(\S+)\s+account=(.*?)\s+symbol=(.*?)\s+risk_type=(\S+)\s+decision=(\S+)\s+reason=(.*?)\s+threshold=(.*?)\s+observed_value=(.*)$') {
            $events += [pscustomobject]@{
                run_id = $Matches[1]
                ts_ms = $Matches[2]
                account = $Matches[3].Trim()
                symbol = $Matches[4].Trim()
                risk_type = $Matches[5]
                decision = $Matches[6]
                reason = $Matches[7].Trim()
                threshold = $Matches[8].Trim()
                observed_value = $Matches[9].Trim()
            }
        }
    }
    return $events
}

function Get-FirstHaltReason {
    param([object[]]$RiskRejections)

    foreach ($event in $RiskRejections) {
        if ($event.decision -eq "rejected") {
            return [string]$event.risk_type
        }
    }
    return $null
}

function Get-IbkrAccountId {
    param([string]$ConfigText)

    if ($ConfigText -match 'account_id\s*=\s*"([^"]+)"') {
        return $Matches[1]
    }
    return ""
}

function Get-IbkrGatewayValue {
    param(
        [string]$ConfigText,
        [string]$Name,
        [string]$DefaultValue
    )

    if ($ConfigText -match "$Name\s*=\s*`"([^`"]+)`"") {
        return $Matches[1]
    }
    if ($ConfigText -match "$Name\s*=\s*(\d+)") {
        return $Matches[1]
    }
    return $DefaultValue
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

function Write-GatewayPreflightFailureSummary {
    param(
        [string]$HostName,
        [int]$PortNumber
    )

    $logPath = Join-Path $runDir "gateway-preflight.log"
    $message = "unable to connect to IBKR paper gateway at $($HostName):$PortNumber"
    $message | Set-Content -Path $logPath -Encoding UTF8

    $gatewayChecks = [pscustomobject]@{
        status = "failed"
        failure_class = "gateway_unreachable"
        failed_check = "gateway_preflight"
        checks = @(
            [pscustomobject]@{
                name = "gateway_preflight"
                command = "tcp $($HostName):$PortNumber"
                exit_code = 1
                status = "failed"
                failure_class = "gateway_unreachable"
                output = $message
                log = $logPath
            }
        )
    }

    $summary = [pscustomobject]@{
        run_id = $runId
        status = "failed"
        failure_class = "gateway_unreachable"
        config = $runConfigPath
        database = $databaseUrl
        parquet = $OutputParquet
        account_id = $effectiveAccountId
        reports = [pscustomobject]@{
            text = $textReportPath
            csv = $csvReportPath
            html = $htmlReportPath
        }
        refreshed = "not_started"
        order_submit = "enabled"
        halt_reason = $null
        risk_rejections = @()
        open_orders_remaining = 0
        cancel_all_attempted = $false
        cancel_all_succeeded = $true
        reconciliation_status = "not_started"
        gateway_checks = $gatewayChecks
    }
    $summary | ConvertTo-Json -Depth 5 | Set-Content -Path $summaryPath -Encoding UTF8
    Write-Host "summary : $summaryPath"
}

function Invoke-IbkrPaperGatewayChecks {
    Write-Host "Running IBKR paper Gateway checks"
    $checkSpecs = @(
        [pscustomobject]@{ name = "readonly"; args = @("ibkr-paper-readonly", "--config", $runConfigPath) },
        [pscustomobject]@{ name = "open_orders"; args = @("ibkr-paper-open-orders", "--config", $runConfigPath) },
        [pscustomobject]@{ name = "executions"; args = @("ibkr-paper-executions", "--config", $runConfigPath, "--request-id", "1") },
        [pscustomobject]@{ name = "reconcile"; args = @("ibkr-paper-reconcile", "--config", $runConfigPath, "--request-id", "1") },
        [pscustomobject]@{ name = "recover"; args = @("ibkr-paper-recover", "--config", $runConfigPath, "--request-id", "1") }
    )

    $results = @()
    foreach ($spec in $checkSpecs) {
        $captured = Invoke-CapturedTraderCheck $spec.args
        $openOrdersFailure = ($spec.name -eq "open_orders" -and (Test-OpenOrdersRemaining $captured.output))
        $failureClass = Get-IbkrFailureClass -Text $captured.output -ExitCode $captured.exit_code -OpenOrdersFailure $openOrdersFailure
        $results += [pscustomobject]@{
            name = $spec.name
            command = "trader $($spec.args -join ' ')"
            exit_code = $captured.exit_code
            status = if ($failureClass -eq "ok") { "completed" } else { "failed" }
            failure_class = $failureClass
            output = $captured.output
        }
        if ($failureClass -ne "ok") {
            Write-Warning "ibkr-paper $($spec.name) check classified as $failureClass"
            break
        }
    }

    $failedCheck = $results | Where-Object { $_.failure_class -ne "ok" } | Select-Object -First 1

    [pscustomobject]@{
        status = if ($null -eq $failedCheck) { "completed" } else { "failed" }
        failure_class = if ($null -eq $failedCheck) { "ok" } else { $failedCheck.failure_class }
        failed_check = if ($null -eq $failedCheck) { "" } else { $failedCheck.name }
        checks = $results
        readonly = (($results | Where-Object { $_.name -eq "readonly" } | Select-Object -First 1).output)
        open_orders = (($results | Where-Object { $_.name -eq "open_orders" } | Select-Object -First 1).output)
        executions = (($results | Where-Object { $_.name -eq "executions" } | Select-Object -First 1).output)
        reconciliation = (($results | Where-Object { $_.name -eq "reconcile" } | Select-Object -First 1).output)
        recover = (($results | Where-Object { $_.name -eq "recover" } | Select-Object -First 1).output)
    }
}

function Invoke-IbkrPaperGatewayChecksUntilNoOpenOrders {
    param([string]$Reason)

    $deadline = [DateTimeOffset]::UtcNow.AddSeconds([Math]::Max(0, $OpenOrdersSettleSeconds))
    $pollSeconds = [Math]::Max(1, $OpenOrdersPollSeconds)
    $gatewayChecks = $null

    do {
        $gatewayChecks = Invoke-IbkrPaperGatewayChecks
        $openOrdersRemaining = Get-OpenOrdersCount ([string]$gatewayChecks.open_orders)
        if ($openOrdersRemaining -eq 0) {
            return $gatewayChecks
        }
        if ($gatewayChecks.failure_class -ne "open_orders_remaining") {
            return $gatewayChecks
        }
        if ([DateTimeOffset]::UtcNow -ge $deadline) {
            return $gatewayChecks
        }
        Write-Host "IBKR paper open orders still settling after $Reason; open_orders=$openOrdersRemaining"
        Start-Sleep -Seconds $pollSeconds
    } while ($true)
}

function Get-IbkrReconciliationStatus {
    param([object]$GatewayChecks)

    if ($null -eq $GatewayChecks) {
        if ($ReadOnly -or $ConfirmIbkrPaperOrder) {
            return "unknown"
        }
        return "not_run"
    }

    $reconcileCheck = @($GatewayChecks.checks | Where-Object { $_.name -eq "reconcile" } | Select-Object -First 1)
    if ($reconcileCheck.Count -gt 0 -and [string]$reconcileCheck[0].output -match "ibkr paper reconcile ok:") {
        return "ok"
    }
    return "unknown"
}

function Get-ReconciliationInt {
    param(
        [string]$Text,
        [string]$Name
    )

    $match = [regex]::Match($Text, "$Name=(-?\d+)")
    if ($match.Success) {
        return [int]$match.Groups[1].Value
    }
    return 0
}

function Get-ReconciliationDecimal {
    param(
        [string]$Text,
        [string]$Name
    )

    $match = [regex]::Match($Text, "$Name=([-+]?\d+(?:\.\d+)?)")
    if ($match.Success) {
        return [decimal]$match.Groups[1].Value
    }
    return [decimal]0
}

function Get-IbkrReconciliationCounters {
    param([object]$GatewayChecks)

    $output = if ($null -ne $GatewayChecks) { [string]$GatewayChecks.reconciliation } else { "" }
    $audits = if ($output -match "ibkr paper reconcile ok:") { 1 } else { 0 }
    $localOnlyOrders = Get-ReconciliationInt -Text $output -Name "local_only_orders"
    $remoteOpenUnmatched = Get-ReconciliationInt -Text $output -Name "remote_open_unmatched"
    $remoteExecutionUnmatched = Get-ReconciliationInt -Text $output -Name "remote_execution_unmatched"
    $executionFieldDrifts = Get-ReconciliationInt -Text $output -Name "remote_execution_field_drifts"
    $qtyDelta = Get-ReconciliationDecimal -Text $output -Name "qty_delta"
    $executionDrifts = $remoteExecutionUnmatched + $executionFieldDrifts
    if ($qtyDelta -ne [decimal]0) {
        $executionDrifts += 1
    }

    [pscustomobject]@{
        audits = $audits
        cash_drifts = 0
        position_drifts = 0
        open_order_drifts = $localOnlyOrders + $remoteOpenUnmatched
        execution_drifts = $executionDrifts
        execution_field_drifts = $executionFieldDrifts
        stale_inputs = 0
        total_drifts = $localOnlyOrders + $remoteOpenUnmatched + $executionDrifts
    }
}

try {
    $env:CARGO_BUILD_JOBS = "1"
    if (-not $env:TRADER_TEST_EXE) {
        Invoke-CheckedCargo @("build", "-p", "trader-cli")
    }

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
    if ($IbkrRouteExchange.Trim().Length -gt 0) {
        $routeLine = "ibkr_route_exchange = `"$($IbkrRouteExchange.Trim())`""
        if ($configText -match 'ibkr_route_exchange = "[^"]*"') {
            $configText = $configText -replace 'ibkr_route_exchange = "[^"]*"', $routeLine
        } else {
            $configText = $configText -replace '(order_submit_enabled = (?:true|false))', "`$1`r`n$routeLine"
        }
    }
    if ($IbkrOverridePercentageConstraints) {
        $overrideLine = "ibkr_override_percentage_constraints = true"
        if ($configText -match 'ibkr_override_percentage_constraints = (?:true|false)') {
            $configText = $configText -replace 'ibkr_override_percentage_constraints = (?:true|false)', $overrideLine
        } else {
            $configText = $configText -replace '(order_submit_enabled = (?:true|false))', "`$1`r`n$overrideLine"
        }
    }

    $effectiveAccountId = Get-IbkrAccountId $configText
    $usesGateway = $ReadOnly -or $ConfirmIbkrPaperOrder
    if ($usesGateway -and ($effectiveAccountId.Length -eq 0 -or $effectiveAccountId -eq "DU000000")) {
        throw "IBKR paper gateway checks require a real IBKR paper account id; pass -AccountId DU... or update the config"
    }

    $runConfigText = $configText `
        -replace 'run_id = "ibkr-aapl-1d-paper"', "run_id = `"$runId`"" `
        -replace 'url = "sqlite://data/ibkr-aapl-1d-paper.sqlite"', "url = `"$databaseUrl`""
    if ($ConfirmIbkrPaperOrder) {
        $runConfigText = $runConfigText -replace 'order_submit_enabled = false', 'order_submit_enabled = true'
    }
    Set-Content -Path $runConfigPath -Value $runConfigText -Encoding UTF8

    if ($usesGateway) {
        $effectiveGatewayHost = Get-IbkrGatewayValue -ConfigText $runConfigText -Name "host" -DefaultValue "127.0.0.1"
        $effectiveGatewayPort = [int](Get-IbkrGatewayValue -ConfigText $runConfigText -Name "port" -DefaultValue "7497")
        if (-not (Test-GatewayPort -HostName $effectiveGatewayHost -PortNumber $effectiveGatewayPort)) {
            Write-GatewayPreflightFailureSummary -HostName $effectiveGatewayHost -PortNumber $effectiveGatewayPort
            throw "IBKR paper run failed: gateway_preflight classified as gateway_unreachable; see $summaryPath"
        }
    }

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
    if ($ReadOnly) {
        Write-Host "Skipping paper-run for read-only IBKR Gateway reconciliation"
    } else {
        try {
            Invoke-CheckedTrader @("paper-run", "--config", $runConfigPath)
        } catch {
            if ($usesGateway) {
                Invoke-IbkrPaperGatewayChecks
            }
            throw
        }
    }
    if (-not $ReadOnly) {
        Invoke-CheckedTrader @("report", "--config", $runConfigPath, "--run-id", $runId)
        Invoke-CheckedTrader @("report", "--config", $runConfigPath, "--run-id", $runId, "--format", "text", "--output", $textReportPath)
        Invoke-CheckedTrader @("report", "--config", $runConfigPath, "--run-id", $runId, "--format", "csv", "--output", $csvReportPath)
        Invoke-CheckedTrader @("report", "--config", $runConfigPath, "--run-id", $runId, "--format", "html", "--output", $htmlReportPath)
    }
    $gatewayChecks = if ($usesGateway) { Invoke-IbkrPaperGatewayChecksUntilNoOpenOrders -Reason $(if ($ReadOnly) { "read-only reconciliation" } else { "paper-run" }) } else { $null }
    $riskEventsOutput = Invoke-CapturedTrader @("risk-events", "--config", $runConfigPath, "--run-id", $runId)
    $riskRejections = @(Get-RiskRejections $riskEventsOutput)
    $haltReason = Get-FirstHaltReason $riskRejections
    $openOrdersOutput = if ($null -ne $gatewayChecks) { [string]$gatewayChecks.open_orders } else { "" }
    $openOrdersRemaining = Get-OpenOrdersCount $openOrdersOutput
    $cancelAllAttempted = $false
    $cancelAllSucceeded = $true
    $cancelAllOutput = ""
    if ($ConfirmIbkrPaperOrder -and $openOrdersRemaining -gt 0) {
        $cancelAllAttempted = $true
        $cancelAllSucceeded = $false
        try {
            $cancelAllOutput = Invoke-CapturedTrader @(
                "risk-kill-switch",
                "--config", $runConfigPath,
                "--run-id", $runId,
                "--cancel-open-orders",
                "--confirm-kill-switch"
            )
        } catch {
            Write-Warning "risk-kill-switch cleanup failed: $_"
            $cancelAllOutput = "failed: $_"
        }
        $gatewayChecks = Invoke-IbkrPaperGatewayChecksUntilNoOpenOrders -Reason "risk-kill-switch"
        $openOrdersOutput = [string]$gatewayChecks.open_orders
        $openOrdersRemaining = Get-OpenOrdersCount $openOrdersOutput
        $cancelAllSucceeded = ($openOrdersRemaining -eq 0)
    }
    $reconciliationCounters = Get-IbkrReconciliationCounters $gatewayChecks
    $runFailureClass = if ($openOrdersRemaining -gt 0) {
        "open_orders_remaining"
    } elseif ($null -ne $haltReason) {
        $haltReason
    } elseif ($null -ne $gatewayChecks -and $gatewayChecks.failure_class -ne "ok") {
        $gatewayChecks.failure_class
    } elseif ($reconciliationCounters.total_drifts -gt 0) {
        "reconciliation_drift"
    } else {
        "ok"
    }
    $runStatus = if ($runFailureClass -eq "ok") { "completed" } else { "failed" }

    $summary = [pscustomobject]@{
        run_id = $runId
        status = $runStatus
        failure_class = $runFailureClass
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
        halt_reason = $haltReason
        risk_rejections = $riskRejections
        open_orders_remaining = $openOrdersRemaining
        reconciliation_audits = $reconciliationCounters.audits
        reconciliation_cash_drifts = $reconciliationCounters.cash_drifts
        reconciliation_position_drifts = $reconciliationCounters.position_drifts
        reconciliation_open_order_drifts = $reconciliationCounters.open_order_drifts
        reconciliation_execution_drifts = $reconciliationCounters.execution_drifts
        reconciliation_execution_field_drifts = $reconciliationCounters.execution_field_drifts
        reconciliation_stale_inputs = $reconciliationCounters.stale_inputs
        cancel_all_attempted = $cancelAllAttempted
        cancel_all_succeeded = $cancelAllSucceeded
        cancel_all = $cancelAllOutput
        reconciliation_status = Get-IbkrReconciliationStatus $gatewayChecks
        gateway_checks = $gatewayChecks
    }
    $summary | ConvertTo-Json -Depth 5 | Set-Content -Path $summaryPath -Encoding UTF8
    Write-Host "summary : $summaryPath"

    if ($runStatus -ne "completed") {
        throw "IBKR paper run failed post-run checks: $runFailureClass; see $summaryPath"
    }

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
