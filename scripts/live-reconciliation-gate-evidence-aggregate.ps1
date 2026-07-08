param(
    [string]$IbkrSummary,
    [string]$IbkrAccount,
    [string]$BinanceSummary,
    [string]$BinanceAccount = "binance-testnet",
    [string]$TemplateDatabase = "data/ibkr-aapl-1d-paper.sqlite",
    [string]$OutputDir = "data/live-reconciliation-gate-replay",
    [string]$EvidenceId = "",
    [int]$MinSuccessfulAudits = 3,
    [int64]$MaxAuditAgeMs = 300000
)

$ErrorActionPreference = "Stop"

function Require-Path {
    param(
        [string]$Path,
        [string]$Label
    )

    if ([string]::IsNullOrWhiteSpace($Path) -or -not (Test-Path $Path)) {
        throw "missing $Label path: $Path"
    }
}

function Read-JsonFile {
    param([string]$Path)

    return Get-Content -Path $Path -Raw | ConvertFrom-Json
}

function Escape-Sql {
    param([string]$Value)

    return $Value.Replace("'", "''")
}

function Require-Zero {
    param(
        [object]$Value,
        [string]$Label
    )

    if ([int64]$Value -ne 0) {
        throw "$Label must be 0, observed $Value"
    }
}

function Validate-IbkrSummary {
    param(
        [object]$Summary,
        [string]$Account,
        [int]$RequiredAudits
    )

    if ([string]$Summary.status -ne "completed") {
        throw "IBKR summary status must be completed, observed $($Summary.status)"
    }
    if ([string]$Summary.failure_class -ne "ok") {
        throw "IBKR failure_class must be ok, observed $($Summary.failure_class)"
    }
    if (-not [bool]$Summary.read_only) {
        throw "IBKR summary must be read_only"
    }
    if ([string]$Summary.account_id -ne $Account) {
        throw "IBKR account mismatch: expected $Account observed $($Summary.account_id)"
    }
    if ([int]$Summary.reconciliation_audits -lt $RequiredAudits) {
        throw "IBKR reconciliation_audits must be >= $RequiredAudits, observed $($Summary.reconciliation_audits)"
    }

    Require-Zero $Summary.reconciliation_cash_drifts "IBKR cash drifts"
    Require-Zero $Summary.reconciliation_position_drifts "IBKR position drifts"
    Require-Zero $Summary.reconciliation_open_order_drifts "IBKR open-order drifts"
    Require-Zero $Summary.reconciliation_execution_drifts "IBKR execution drifts"
    Require-Zero $Summary.reconciliation_stale_inputs "IBKR stale inputs"
}

function Validate-BinanceSummary {
    param(
        [object]$Summary,
        [int]$RequiredAudits
    )

    if ([string]$Summary.status -ne "completed") {
        throw "Binance summary status must be completed, observed $($Summary.status)"
    }
    if ([string]$Summary.failure_class -ne "ok") {
        throw "Binance failure_class must be ok, observed $($Summary.failure_class)"
    }
    if ([string]$Summary.order_submit -ne "disabled") {
        throw "Binance order_submit must be disabled, observed $($Summary.order_submit)"
    }
    if ([int]$Summary.iterations_completed -lt $RequiredAudits) {
        throw "Binance iterations_completed must be >= $RequiredAudits, observed $($Summary.iterations_completed)"
    }

    $cleanIterations = @($Summary.iterations | Where-Object {
        [int]$_.exit_code -eq 0 -and
        [string]$_.status -eq "completed" -and
        [string]$_.failure_class -eq "ok" -and
        [string]$_.reconciliation_status -eq "ok" -and
        [int]$_.open_orders_remaining -eq 0
    })

    if ($cleanIterations.Count -lt $RequiredAudits) {
        throw "Binance clean iterations must be >= $RequiredAudits, observed $($cleanIterations.Count)"
    }
}

Require-Path $IbkrSummary "IBKR summary"
Require-Path $BinanceSummary "Binance summary"
Require-Path $TemplateDatabase "template database"

if ([string]::IsNullOrWhiteSpace($IbkrAccount)) {
    throw "IbkrAccount is required"
}
if ([string]::IsNullOrWhiteSpace($BinanceAccount)) {
    throw "BinanceAccount is required"
}
if ($MinSuccessfulAudits -lt 1) {
    throw "MinSuccessfulAudits must be at least 1"
}
if ($MaxAuditAgeMs -lt 1) {
    throw "MaxAuditAgeMs must be at least 1"
}

$ibkr = Read-JsonFile $IbkrSummary
$binance = Read-JsonFile $BinanceSummary
Validate-IbkrSummary $ibkr $IbkrAccount $MinSuccessfulAudits
Validate-BinanceSummary $binance $MinSuccessfulAudits

if ([string]::IsNullOrWhiteSpace($EvidenceId)) {
    $EvidenceId = "gate-evidence-$([guid]::NewGuid().ToString('N').Substring(0, 12))"
}

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
$databasePath = Join-Path $OutputDir "$EvidenceId.sqlite"
$configPath = Join-Path $OutputDir "$EvidenceId.toml"
Copy-Item -Path $TemplateDatabase -Destination $databasePath -Force

$now = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
$runId = Escape-Sql $EvidenceId
$ibkrAccountSql = Escape-Sql $IbkrAccount
$binanceAccountSql = Escape-Sql $BinanceAccount
$configJson = Escape-Sql "{}"

$values = New-Object System.Collections.Generic.List[string]
for ($i = 1; $i -le $MinSuccessfulAudits; $i++) {
    $values.Add("('$runId-ibkr-$i','$runId','$ibkrAccountSql','ibkr',$now,'info',0,0,0,0,0,'{}',$now)")
    $values.Add("('$runId-binance-$i','$runId','$binanceAccountSql','binance',$now,'info',0,0,0,0,0,'{}',$now)")
}

$sql = @"
INSERT OR REPLACE INTO strategy_runs (id, name, mode, status, started_at_ms, ended_at_ms, error, config_json)
VALUES ('$runId', '$runId', 'paper', 'completed', $now, $now, NULL, '$configJson');
DELETE FROM broker_reconciliation_audits;
INSERT INTO broker_reconciliation_audits (
    id, run_id, account_id, broker_kind, ts, severity,
    cash_drift_count, position_drift_count, open_order_drift_count,
    execution_drift_count, stale_input_count, payload_json, created_at
) VALUES $($values -join ", ");
"@

sqlite3 $databasePath $sql

$config = @"
[runtime]
mode = "paper"
run_id = "$EvidenceId"

[database]
url = "sqlite://$($databasePath.Replace('\', '/'))"

[data]
source = "parquet"
path = "datasets/ibkr/aapl_1d.parquet"

[strategy]
name = "moving_average_cross"
symbols = ["US:NASDAQ:AAPL:EQUITY"]
fast_window = 2
slow_window = 3

[portfolio]
initial_cash = "100000"
base_currency = "USD"
order_qty = "1"
max_abs_qty = "100"

[risk]
max_order_notional = "1000"
min_cash_after_order = "1000"
max_exposure = "10000"
max_drawdown = "0.2"
max_leverage = "1"
max_margin_used = "0"
trading_halted = false

[broker]
kind = "ibkr"
mode = "paper"
host = "127.0.0.1"
port = 7497
client_id = 1
order_submit_enabled = false

[paper]
account_id = "DU000000"
slippage_bps = "5"
fee_bps = "2"

[live]
enabled = false

[live.reconciliation_gate]
enabled = true
min_successful_audits = $MinSuccessfulAudits
max_audit_age_ms = $MaxAuditAgeMs
required_accounts = ["ibkr:$IbkrAccount", "binance:$BinanceAccount"]

[live.startup_recovery]
unmatched_open_orders = "fail"
"@

$config | Set-Content -Path $configPath -Encoding UTF8

[pscustomobject]@{
    evidence_id = $EvidenceId
    config = $configPath
    database = $databasePath
    ibkr_summary = $IbkrSummary
    binance_summary = $BinanceSummary
    min_successful_audits = $MinSuccessfulAudits
    max_audit_age_ms = $MaxAuditAgeMs
    accounts = @("ibkr:$IbkrAccount", "binance:$BinanceAccount")
}
