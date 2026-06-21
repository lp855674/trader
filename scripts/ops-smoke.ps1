$ErrorActionPreference = "Stop"

$repoRoot = Get-Location
$id = [guid]::NewGuid().ToString("N")
$databasePath = Join-Path $env:TEMP "trader-ops-$id.sqlite"
$configPath = Join-Path $env:TEMP "trader-ops-$id.toml"
$alertFilePath = Join-Path $env:TEMP "trader-ops-alerts-$id.jsonl"
$logExportPath = Join-Path $env:TEMP "trader-ops-system-logs-$id.jsonl"
$alertExportPath = Join-Path $env:TEMP "trader-ops-reconciliation-alerts-$id.jsonl"
$alertDeliveryExportPath = Join-Path $env:TEMP "trader-ops-reconciliation-alert-deliveries-$id.jsonl"
$stdoutPath = Join-Path $env:TEMP "trader-ops-server-$id.out.log"
$stderrPath = Join-Path $env:TEMP "trader-ops-server-$id.err.log"
$databaseUrl = "sqlite://$($databasePath.Replace('\', '/'))"
$runId = "ops-live-$id"
$approvalConfigName = "ops-approval-$id"
$targetDir = $env:TRADER_SMOKE_TARGET_DIR
$targetRoot = if ($targetDir) { $targetDir } else { Join-Path $repoRoot "target" }
$traderExe = Join-Path $targetRoot "debug/trader.exe"
$serverExe = Join-Path $targetRoot "debug/trader-server.exe"

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

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

function Wait-RunStatus {
    param([string]$BaseUrl, [string]$RunId, [string]$Expected)

    for ($i = 0; $i -lt 80; $i++) {
        Start-Sleep -Milliseconds 250
        $status = Invoke-RestMethod "$BaseUrl/api/v1/runs/$RunId/status"
        if ($status.status -eq $Expected) { return $status }
    }
    throw "run $RunId did not reach $Expected"
}

function Wait-ApiArray {
    param([string]$Url, [string]$Description)

    for ($i = 0; $i -lt 80; $i++) {
        Start-Sleep -Milliseconds 250
        $items = Invoke-RestMethod $Url
        if (@($items).Count -ge 1) { return $items }
    }
    throw "expected at least one $Description"
}

function Wait-ApiObject {
    param([string]$Url, [string]$Description)

    for ($i = 0; $i -lt 80; $i++) {
        Start-Sleep -Milliseconds 250
        try {
            return Invoke-RestMethod $Url
        } catch {}
    }
    throw "expected $Description"
}

$template = Get-Content "configs/backtest/ma_cross.toml" -Raw
$config = $template `
    -replace 'mode = "backtest"', 'mode = "live"' `
    -replace 'run_id = "sample-ma-cross"', "run_id = `"$runId`"" `
    -replace 'url = "sqlite://data/trader.sqlite"', "url = `"$databaseUrl`"" `
    -replace 'initial_cash = "100000"', 'initial_cash = "25000"' `
    -replace 'enabled = false', "enabled = true`nbroker_snapshot_interval_ms = 5"
$config = "$config`n`n[live.alerts]`nenabled = true`nsink = `"file`"`nfile_path = `"$($alertFilePath.Replace('\', '/'))`"`ncooldown_ms = 60000`n"
Set-Content -Path $configPath -Value $config -Encoding UTF8

$server = $null
try {
    $env:CARGO_BUILD_JOBS = if ($env:CARGO_BUILD_JOBS) { $env:CARGO_BUILD_JOBS } else { "1" }
    if ($targetDir) {
        $env:CARGO_TARGET_DIR = $targetDir
    }
    Write-Host "Ops smoke config: $configPath"
    Write-Host "Ops smoke database: $databaseUrl"

    Invoke-CheckedCargo @("build", "-p", "trader-cli", "-p", "trader-server")

    $env:TRADER_CONFIG = $configPath
    $env:TRADER_DATABASE_URL = $databaseUrl
    if (Test-Path $serverExe) {
        $server = Start-Process -FilePath $serverExe `
            -WorkingDirectory $repoRoot `
            -PassThru `
            -RedirectStandardOutput $stdoutPath `
            -RedirectStandardError $stderrPath `
            -WindowStyle Hidden
    } else {
        $server = Start-Process -FilePath "cargo" `
            -ArgumentList @("run", "-p", "trader-server") `
            -WorkingDirectory $repoRoot `
            -PassThru `
            -RedirectStandardOutput $stdoutPath `
            -RedirectStandardError $stderrPath `
            -WindowStyle Hidden
    }

    $baseUrl = "http://127.0.0.1:8080"
    $ready = $false
    for ($i = 0; $i -lt 80; $i++) {
        if ($server.HasExited) { break }
        Start-Sleep -Milliseconds 500
        try {
            Invoke-RestMethod "$baseUrl/api/v1/health" | Out-Null
            $ready = $true
            break
        } catch {}
    }
    if (-not $ready) {
        if (Test-Path $stdoutPath) { Get-Content $stdoutPath }
        if (Test-Path $stderrPath) { Get-Content $stderrPath }
        throw "trader-server did not become ready"
    }

    $live = Invoke-RestMethod -Method Post "$baseUrl/api/v1/live-runs"
    Assert-True ($live.run_id -eq $runId) "expected live run id $runId"
    Wait-RunStatus $baseUrl $runId "running" | Out-Null

    $apiCash = Wait-ApiArray "$baseUrl/api/v1/runs/$runId/cash-snapshots?currency=USD" "API cash snapshot"
    $apiPositions = Wait-ApiArray "$baseUrl/api/v1/runs/$runId/position-snapshots?symbol=CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP&position_side=long" "API position snapshot"
    $apiReconciliation = Wait-ApiObject "$baseUrl/api/v1/runs/$runId/reconciliation" "API reconciliation"
    $apiLogs = Wait-ApiArray "$baseUrl/api/v1/runs/$runId/system-logs?target=runtime.broker_snapshot" "API broker snapshot logs"
    $apiAlertSummary = Wait-ApiObject "$baseUrl/api/v1/runs/$runId/reconciliation-alerts/summary" "API reconciliation alert summary"
    $apiAlertDeliverySummary = Wait-ApiObject "$baseUrl/api/v1/runs/$runId/reconciliation-alert-deliveries/summary" "API reconciliation alert delivery summary"
    $apiConfigVersion = Wait-ApiObject "$baseUrl/api/v1/runs/$runId/config-version" "API config version binding"

    Assert-True (@($apiCash).Count -ge 1) "expected API cash snapshots"
    Assert-True (@($apiPositions).Count -ge 1) "expected API position snapshots"
    Assert-True ($apiReconciliation.status -eq "drift") "expected API reconciliation drift"
    Assert-True (@($apiLogs).Count -ge 1) "expected API system logs"
    Assert-True ($apiAlertSummary.alert_count -ge 1) "expected API reconciliation alert summary"
    Assert-True ($apiAlertDeliverySummary.delivery_count -ge 1) "expected API reconciliation alert delivery summary"
    Assert-True ($apiConfigVersion.run_id -eq $runId) "expected API config version run id"

    $cliCash = Invoke-CheckedTrader @("snapshots", "cash", "--config", $configPath, "--run-id", $runId, "--currency", "USD") 2>&1 | Out-String
    $cliPositions = Invoke-CheckedTrader @("snapshots", "positions", "--config", $configPath, "--run-id", $runId, "--symbol", "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP", "--position-side", "long") 2>&1 | Out-String
    $cliReconciliation = Invoke-CheckedTrader @("reconciliation", "--config", $configPath, "--run-id", $runId) 2>&1 | Out-String
    $cliAlertSummary = Invoke-CheckedTrader @("reconciliation-alerts-summary", "--config", $configPath, "--run-id", $runId) 2>&1 | Out-String
    $cliAlertDeliverySummary = Invoke-CheckedTrader @("reconciliation-alert-deliveries-summary", "--config", $configPath, "--run-id", $runId) 2>&1 | Out-String
    $cliLogs = Invoke-CheckedTrader @("logs", "list", "--config", $configPath, "--run-id", $runId, "--target", "runtime.broker_snapshot") 2>&1 | Out-String
    $cliConfigVersion = Invoke-CheckedTrader @("runs", "config-version", "--config", $configPath, "--run-id", $runId) 2>&1 | Out-String

    Assert-True ($cliCash.Contains("cash_snapshot: run_id=$runId")) "expected CLI cash snapshot"
    Assert-True ($cliPositions.Contains("position_snapshot: run_id=$runId")) "expected CLI position snapshot"
    Assert-True ($cliReconciliation.Contains("reconciliation: run_id=$runId status=drift")) "expected CLI reconciliation drift"
    Assert-True ($cliAlertSummary.Contains("reconciliation_alert_summary: run_id=$runId")) "expected CLI reconciliation alert summary"
    Assert-True ($cliAlertSummary.Contains("alert_count=")) "expected CLI alert count"
    Assert-True ($cliAlertDeliverySummary.Contains("reconciliation_alert_delivery_summary: run_id=$runId")) "expected CLI reconciliation alert delivery summary"
    Assert-True ($cliAlertDeliverySummary.Contains("delivery_count=")) "expected CLI alert delivery count"
    Assert-True ($cliLogs.Contains("system_log: run_id=$runId")) "expected CLI system log"
    Assert-True ($cliConfigVersion.Contains("run_config_version: run_id=$runId")) "expected CLI config version binding"
    Assert-True (-not $cliConfigVersion.Contains("status=missing")) "expected bound CLI config version"

    $createdApprovalConfig = Invoke-RestMethod `
        -Method Post `
        -Uri "$baseUrl/api/v1/configs" `
        -ContentType "application/json" `
        -Body (@{
            name = $approvalConfigName
            content = @{
                risk = @{
                    max_order_notional = "1000"
                }
            }
            created_by = "release"
            target_env = "staging"
            rollout = "ops-smoke"
            ts_ms = 800
        } | ConvertTo-Json -Depth 5)
    Assert-True ($createdApprovalConfig.name -eq $approvalConfigName) "expected staged approval config"
    $approvalConfigId = $createdApprovalConfig.id
    Assert-True (-not [string]::IsNullOrWhiteSpace($approvalConfigId)) "expected approval config id"

    $pendingApprovalConfig = Invoke-RestMethod `
        -Method Put `
        -Uri "$baseUrl/api/v1/configs/$approvalConfigName/1/state" `
        -ContentType "application/json" `
        -Body (@{
            new_state = "pending_review"
            changed_by = "release"
            actor_role = "release_manager"
            reason = "ops smoke approval queue"
            ts_ms = 900
        } | ConvertTo-Json)
    Assert-True ($pendingApprovalConfig.state -eq "pending_review") "expected pending approval config state"

    $apiPendingApprovals = Wait-ApiArray "$baseUrl/api/v1/config-approvals/pending?target_env=staging" "API pending approvals"
    $matchingApiApprovals = @($apiPendingApprovals | Where-Object { $_.name -eq $approvalConfigName })
    Assert-True (@($matchingApiApprovals).Count -eq 1) "expected API pending approval queue entry"

    $cliPendingApprovals = Invoke-CheckedTrader @("configs", "pending-approvals", "--config", $configPath, "--target-env", "staging") 2>&1 | Out-String
    Assert-True ($cliPendingApprovals.Contains("config_approval: name=$approvalConfigName version=1")) "expected CLI pending approval entry"
    Assert-True ($cliPendingApprovals.Contains("target_env=staging")) "expected CLI pending approval target env"

    $approvedApprovalConfig = Invoke-RestMethod `
        -Method Put `
        -Uri "$baseUrl/api/v1/configs/$approvalConfigName/1/state" `
        -ContentType "application/json" `
        -Body (@{
            new_state = "approved"
            changed_by = "qa-owner"
            actor_role = "approver"
            reason = "ops smoke approval"
            ts_ms = 1000
        } | ConvertTo-Json)
    Assert-True ($approvedApprovalConfig.state -eq "approved") "expected approved config state"

    $publishedApprovalConfig = Invoke-RestMethod `
        -Method Put `
        -Uri "$baseUrl/api/v1/configs/$approvalConfigName/1/state" `
        -ContentType "application/json" `
        -Body (@{
            new_state = "published"
            changed_by = "release"
            actor_role = "release_manager"
            reason = "ops smoke publish"
            ts_ms = 1100
        } | ConvertTo-Json)
    Assert-True ($publishedApprovalConfig.state -eq "published") "expected published config state"

    $apiConfigReleases = $null
    $apiConfigAudits = $null
    $remainingPendingApprovals = $null
    $approvalConfigIdPath = [uri]::EscapeDataString($approvalConfigId)
    for ($i = 0; $i -lt 40; $i++) {
        Start-Sleep -Milliseconds 250
        $apiConfigReleases = Invoke-RestMethod "$baseUrl/api/v1/configs/$approvalConfigIdPath/releases"
        $apiConfigAudits = Invoke-RestMethod "$baseUrl/api/v1/configs/$approvalConfigIdPath/audits"
        $remainingPendingApprovals = @(
            (Invoke-RestMethod "$baseUrl/api/v1/config-approvals/pending?target_env=staging") |
                Where-Object { $_.name -eq $approvalConfigName }
        )
        if (@($apiConfigReleases).Count -ge 1 -and @($apiConfigAudits).Count -ge 3 -and @($remainingPendingApprovals).Count -eq 0) {
            break
        }
    }

    Assert-True (@($apiConfigReleases).Count -ge 1) "expected API config releases"
    Assert-True (@($apiConfigAudits).Count -ge 3) "expected API config audits"
    Assert-True (@($remainingPendingApprovals).Count -eq 0) "expected pending approval queue to clear after publish"

    $releaseStatuses = @($apiConfigReleases | ForEach-Object { $_.status })
    $auditActions = @($apiConfigAudits | ForEach-Object { $_.action })
    Assert-True ($releaseStatuses -contains "published") "expected published release record"
    Assert-True ((@($auditActions | Where-Object { $_ -eq "state_changed" }).Count) -ge 3) "expected state_changed audits"

    $cliReleases = Invoke-CheckedTrader @("configs", "releases", "--config", $configPath, "--config-id", $approvalConfigId) 2>&1 | Out-String
    $cliAudits = Invoke-CheckedTrader @("configs", "audits", "--config", $configPath, "--config-id", $approvalConfigId) 2>&1 | Out-String
    Assert-True ($cliReleases.Contains("config_release: config_id=$approvalConfigId version=1 status=published")) "expected CLI published release"
    Assert-True ($cliAudits.Contains("config_audit: config_id=$approvalConfigId version=1 action=state_changed")) "expected CLI config audit"

    $stopped = Invoke-RestMethod -Method Post "$baseUrl/api/v1/live-runs/$runId/stop"
    Assert-True ($stopped.status -eq "stopped") "expected live stopped"
    Assert-True (Test-Path $alertFilePath) "expected alert sink file"
    $alertLines = @(Get-Content $alertFilePath | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    Assert-True (@($alertLines).Count -ge 1) "expected alert sink records"
    $alertRecords = @($alertLines | ForEach-Object { $_ | ConvertFrom-Json })
    Assert-True (@($alertRecords | Where-Object { $_.target -eq "runtime.alert" }).Count -ge 1) "expected runtime.alert sink record"
    Assert-True (@($alertRecords | Where-Object { $_.message -eq "reconciliation_drift.alert" }).Count -ge 1) "expected reconciliation drift sink record"
    Assert-True (@($alertRecords | Where-Object { -not [string]::IsNullOrWhiteSpace($_.dedup_key) }).Count -eq @($alertRecords).Count) "expected dedup keys on alert sink records"
    Assert-True (@($alertRecords).Count -le $apiAlertSummary.alert_count) "expected sink records to not exceed alert log summary"
    $cliLogExport = Invoke-CheckedTrader @("logs", "export", "--config", $configPath, "--output", $logExportPath, "--run-id", $runId, "--target", "runtime.alert") 2>&1 | Out-String
    Assert-True ($cliLogExport.Contains("system_logs_exported: count=")) "expected CLI log export summary"
    Assert-True (Test-Path $logExportPath) "expected exported system log file"
    $exportedLogLines = @(Get-Content $logExportPath | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    Assert-True (@($exportedLogLines).Count -ge 1) "expected exported system log records"
    $exportedLogRecords = @($exportedLogLines | ForEach-Object { $_ | ConvertFrom-Json })
    Assert-True (@($exportedLogRecords | Where-Object { $_.target -eq "runtime.alert" }).Count -ge 1) "expected exported runtime.alert records"
    Assert-True (@($exportedLogRecords | Where-Object { $_.message -eq "reconciliation_drift.alert" }).Count -ge 1) "expected exported reconciliation alert records"
    $cliAlertExport = Invoke-CheckedTrader @("reconciliation-alerts-export", "--config", $configPath, "--output", $alertExportPath, "--run-id", $runId) 2>&1 | Out-String
    Assert-True ($cliAlertExport.Contains("reconciliation_alerts_exported: count=")) "expected CLI reconciliation alert export summary"
    Assert-True (Test-Path $alertExportPath) "expected reconciliation alert export file"
    $exportedAlertLines = @(Get-Content $alertExportPath | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    Assert-True (@($exportedAlertLines).Count -ge 1) "expected reconciliation alert export records"
    $exportedAlertRecords = @($exportedAlertLines | ForEach-Object { $_ | ConvertFrom-Json })
    Assert-True (@($exportedAlertRecords | Where-Object { $_.message -eq "reconciliation_drift.alert" }).Count -ge 1) "expected exported reconciliation drift alerts"
    Assert-True (@($exportedAlertRecords | Where-Object { -not [string]::IsNullOrWhiteSpace($_.dedup_key) }).Count -eq @($exportedAlertRecords).Count) "expected dedup keys on exported reconciliation alerts"
    $cliAlertDeliveryExport = Invoke-CheckedTrader @("reconciliation-alert-deliveries-export", "--config", $configPath, "--output", $alertDeliveryExportPath, "--run-id", $runId) 2>&1 | Out-String
    Assert-True ($cliAlertDeliveryExport.Contains("reconciliation_alert_deliveries_exported: count=")) "expected CLI reconciliation alert delivery export summary"
    Assert-True (Test-Path $alertDeliveryExportPath) "expected reconciliation alert delivery export file"
    $exportedAlertDeliveryLines = @(Get-Content $alertDeliveryExportPath | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    Assert-True (@($exportedAlertDeliveryLines).Count -ge 1) "expected reconciliation alert delivery export records"
    $exportedAlertDeliveryRecords = @($exportedAlertDeliveryLines | ForEach-Object { $_ | ConvertFrom-Json })
    Assert-True (@($exportedAlertDeliveryRecords | Where-Object { $_.message -eq "alert.delivery" }).Count -ge 1) "expected exported alert delivery records"
    Assert-True (@($exportedAlertDeliveryRecords | Where-Object { $_.sink -eq "file" }).Count -ge 1) "expected exported file delivery record"
    Assert-True (@($exportedAlertDeliveryRecords | Where-Object { $_.status -eq "sent" }).Count -ge 1) "expected exported sent delivery record"

    [pscustomobject]@{
        run_id = $runId
        api_cash_snapshots = @($apiCash).Count
        api_position_snapshots = @($apiPositions).Count
        api_reconciliation = $apiReconciliation.status
        api_system_logs = @($apiLogs).Count
        api_reconciliation_alerts = $apiAlertSummary.alert_count
        api_reconciliation_alert_deliveries = $apiAlertDeliverySummary.delivery_count
        sink_alert_records = @($alertRecords).Count
        exported_log_records = @($exportedLogRecords).Count
        exported_alert_records = @($exportedAlertRecords).Count
        exported_alert_delivery_records = @($exportedAlertDeliveryRecords).Count
        config_version = $apiConfigVersion.version
        approval_config = $approvalConfigName
        api_pending_approvals = @($matchingApiApprovals).Count
        approval_state = $publishedApprovalConfig.state
        api_config_releases = @($apiConfigReleases).Count
        api_config_audits = @($apiConfigAudits).Count
    }
} finally {
    Set-Location $repoRoot
    if ($server -and -not $server.HasExited) {
        Stop-Process -Id $server.Id -Force
    }
    Remove-Item -LiteralPath $configPath -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $databasePath -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath "$databasePath-shm" -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath "$databasePath-wal" -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $alertFilePath -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $logExportPath -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $alertExportPath -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $alertDeliveryExportPath -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $stdoutPath -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $stderrPath -Force -ErrorAction SilentlyContinue
}
