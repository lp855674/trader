param(
    [int]$Iterations = 3,
    [int]$DelaySeconds = 0,
    [switch]$IncludeBinanceReadOnly,
    [switch]$IncludeBinanceNetwork,
    [switch]$IncludeIbkrReadOnly,
    [string]$IbkrAccountId = "",
    [string]$IbkrGatewayHost = "127.0.0.1",
    [int]$IbkrPort = 7497,
    [int]$IbkrClientId = 1
)

$ErrorActionPreference = "Stop"

if ($Iterations -lt 1) {
    throw "Iterations must be at least 1"
}

if ($DelaySeconds -lt 0) {
    throw "DelaySeconds must be non-negative"
}

if ($IncludeBinanceNetwork -and -not $IncludeBinanceReadOnly) {
    throw "IncludeBinanceNetwork requires IncludeBinanceReadOnly"
}

if ($IncludeIbkrReadOnly -and ($IbkrAccountId.Trim().Length -eq 0 -or $IbkrAccountId -eq "DU000000")) {
    throw "IncludeIbkrReadOnly requires a real IBKR paper account id; pass -IbkrAccountId DU..."
}

$repoRoot = Get-Location
$id = [guid]::NewGuid().ToString("N")
$verificationId = "live-recovery-$($id.Substring(0, 12))"
$verificationDir = Join-Path $repoRoot "data/live-recovery-verification/$verificationId"
$summaryPath = Join-Path $verificationDir "summary.json"
$failed = $false
$iterationSummaries = @()
$adapterSummaries = @()

$localGroups = @(
    @{
        name = "startup_recovery"
        tests = @(
            "live_runtime_recovers_open_orders_and_executions_on_startup"
        )
    },
    @{
        name = "unmatched_open_order_fail"
        tests = @(
            "live_runtime_fails_startup_when_remote_open_order_is_unmatched"
        )
    },
    @{
        name = "unmatched_open_order_warn_only"
        tests = @(
            "live_runtime_can_warn_only_for_unmatched_remote_open_orders_when_configured"
        )
    },
    @{
        name = "recovered_execution_dedup"
        tests = @(
            "live_runtime_adds_new_recovered_executions_to_existing_fills",
            "live_runtime_does_not_decrease_local_filled_qty_when_recovery_lacks_executions"
        )
    },
    @{
        name = "broker_snapshot_drift"
        tests = @(
            "live_runtime_periodically_records_broker_reported_cash_snapshot",
            "live_runtime_periodically_records_broker_reported_position_snapshot",
            "live_runtime_emits_reconciliation_drift_when_broker_cash_differs_from_runtime_cash",
            "live_runtime_emits_reconciliation_drift_when_broker_position_is_missing_from_runtime",
            "live_runtime_emits_reconciliation_drift_when_runtime_position_qty_differs_from_broker"
        )
    },
    @{
        name = "alert_retry_cooldown"
        tests = @(
            "live_runtime_writes_reconciliation_alert_to_file_sink_when_configured",
            "live_runtime_posts_reconciliation_alert_to_webhook_sink_when_configured",
            "live_runtime_sends_reconciliation_alert_to_all_configured_sinks",
            "live_runtime_retries_webhook_alert_with_auth_header",
            "live_runtime_does_not_retry_webhook_alert_on_client_error_and_logs_failure",
            "live_runtime_suppresses_duplicate_file_sink_alerts_within_cooldown"
        )
    },
    @{
        name = "process_supervisor"
        package = "runtime"
        tests = @(
            "supervisor_records_heartbeat_and_health",
            "supervisor_marks_non_terminal_run_failed_on_crash",
            "supervisor_automatically_kills_stale_heartbeat_worker",
            "supervisor_fails_run_on_handshake_timeout"
        )
    },
    @{
        name = "launch_file_secret_guard"
        package = "runtime"
        tests = @(
            "launch_spec_redaction_rejects_secret_fields",
            "launch_spec_redaction_rejects_webhook_auth_token",
            "launch_spec_redaction_rejects_credentialed_database_url"
        )
    },
    @{
        name = "live_worker_ipc"
        package = "trader-cli"
        tests = @(
            "live_worker_starts_and_stops_over_jsonl"
        )
    },
    @{
        name = "api_live_process_routes"
        package = "api"
        tests = @(
            "live_runtime_routes_start_report_status_and_stop",
            "live_runtime_route_fails_by_default_for_fake_unmatched_startup_open_orders",
            "live_runtime_route_warn_only_continues_for_fake_unmatched_startup_open_orders"
        )
    }
)

function Invoke-CapturedCommand {
    param(
        [string]$Name,
        [string[]]$CommandArgs,
        [string]$LogPath
    )

    Write-Host "Running $Name"
    $previousErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        $output = & $CommandArgs[0] $CommandArgs[1..($CommandArgs.Count - 1)] 2>&1
        $exitCode = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }

    $text = $output -join [Environment]::NewLine
    $text | Set-Content -Path $LogPath -Encoding UTF8
    $output | ForEach-Object { Write-Host $_ }

    return [pscustomobject]@{
        name = $Name
        command = $CommandArgs -join " "
        exit_code = $exitCode
        log = $LogPath
    }
}

try {
    New-Item -ItemType Directory -Force -Path $verificationDir | Out-Null
    $env:CARGO_BUILD_JOBS = "1"

    for ($iteration = 1; $iteration -le $Iterations; $iteration++) {
        Write-Host "Live recovery verification iteration $iteration/$Iterations"
        $groupSummaries = @()

        foreach ($group in $localGroups) {
            $package = if ($group.ContainsKey("package")) { $group.package } else { "runtime" }
            $testSummaries = @()
            foreach ($testName in $group.tests) {
                $logPath = Join-Path $verificationDir "iteration-$iteration-$($group.name)-$testName.log"
                $commandArgs = @(
                    "cargo",
                    "test",
                    "-p",
                    $package,
                    $testName
                )
                $result = Invoke-CapturedCommand -Name "$($group.name) $testName iteration $iteration" -CommandArgs $commandArgs -LogPath $logPath
                $testSummaries += $result

                if ($result.exit_code -ne 0) {
                    $failed = $true
                    break
                }
            }

            $groupSummaries += [pscustomobject]@{
                name = $group.name
                status = if ($failed) { "failed" } else { "completed" }
                tests = $testSummaries
            }

            if ($failed) {
                break
            }
        }

        $iterationSummaries += [pscustomobject]@{
            iteration = $iteration
            status = if ($failed) { "failed" } else { "completed" }
            groups = $groupSummaries
        }

        if ($failed) {
            break
        }

        if ($iteration -lt $Iterations -and $DelaySeconds -gt 0) {
            Start-Sleep -Seconds $DelaySeconds
        }
    }

    if (-not $failed -and $IncludeBinanceReadOnly) {
        $logPath = Join-Path $verificationDir "adapter-binance-readonly.log"
        $commandArgs = @(
            "powershell",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            ".\scripts\binance-paper-recover-smoke.ps1"
        )
        if (-not $IncludeBinanceNetwork) {
            $commandArgs += "-SkipNetwork"
        }
        $result = Invoke-CapturedCommand -Name "binance read-only recovery" -CommandArgs $commandArgs -LogPath $logPath
        $adapterSummaries += $result
        if ($result.exit_code -ne 0) {
            $failed = $true
        }
    }

    if (-not $failed -and $IncludeIbkrReadOnly) {
        $logPath = Join-Path $verificationDir "adapter-ibkr-readonly.log"
        $commandArgs = @(
            "powershell",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            ".\scripts\ibkr-paper-test-guide.ps1",
            "-Stage",
            "ReadOnly",
            "-AccountId",
            $IbkrAccountId,
            "-GatewayHost",
            $IbkrGatewayHost,
            "-Port",
            "$IbkrPort",
            "-ClientId",
            "$IbkrClientId"
        )
        $result = Invoke-CapturedCommand -Name "ibkr read-only recovery" -CommandArgs $commandArgs -LogPath $logPath
        $adapterSummaries += $result
        if ($result.exit_code -ne 0) {
            $failed = $true
        }
    }

    $summary = [pscustomobject]@{
        verification_id = $verificationId
        iterations_requested = $Iterations
        iterations_completed = $iterationSummaries.Count
        delay_seconds = $DelaySeconds
        status = if ($failed) { "failed" } else { "completed" }
        local_groups = $localGroups | ForEach-Object { $_.name }
        adapter_checks = [pscustomobject]@{
            binance_readonly = if ($IncludeBinanceReadOnly) { "run" } else { "skipped" }
            binance_network = if ($IncludeBinanceNetwork) { "run" } else { "skipped" }
            ibkr_readonly = if ($IncludeIbkrReadOnly) { "run" } else { "skipped" }
        }
        iterations = $iterationSummaries
        adapters = $adapterSummaries
    }
    $summary | ConvertTo-Json -Depth 8 | Set-Content -Path $summaryPath -Encoding UTF8

    Write-Host "Live recovery verification summary: $summaryPath"

    if ($failed) {
        throw "Live recovery verification failed; see $summaryPath"
    }

    $summary
} finally {
    Set-Location $repoRoot
}
