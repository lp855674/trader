CREATE TABLE IF NOT EXISTS broker_account_balances (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id TEXT NOT NULL,
    account_id TEXT NOT NULL,
    broker_kind TEXT NOT NULL,
    ts INTEGER NOT NULL,
    currency TEXT NOT NULL,
    cash TEXT NOT NULL,
    available_cash TEXT NOT NULL,
    frozen_cash TEXT NOT NULL DEFAULT '0',
    equity TEXT,
    buying_power TEXT,
    margin_used TEXT,
    source_ts INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    FOREIGN KEY(run_id) REFERENCES strategy_runs(id)
);

CREATE INDEX IF NOT EXISTS idx_broker_account_balances_run_ts
ON broker_account_balances(run_id, ts);

CREATE INDEX IF NOT EXISTS idx_broker_account_balances_account_currency
ON broker_account_balances(account_id, currency, ts);

CREATE TABLE IF NOT EXISTS broker_reconciliation_audits (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL,
    account_id TEXT NOT NULL,
    broker_kind TEXT NOT NULL,
    ts INTEGER NOT NULL,
    severity TEXT NOT NULL,
    cash_drift_count INTEGER NOT NULL,
    position_drift_count INTEGER NOT NULL,
    open_order_drift_count INTEGER NOT NULL,
    execution_drift_count INTEGER NOT NULL,
    stale_input_count INTEGER NOT NULL,
    payload_json TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    FOREIGN KEY(run_id) REFERENCES strategy_runs(id)
);

CREATE INDEX IF NOT EXISTS idx_broker_reconciliation_audits_run_ts
ON broker_reconciliation_audits(run_id, ts);

CREATE INDEX IF NOT EXISTS idx_broker_reconciliation_audits_severity
ON broker_reconciliation_audits(severity);

ALTER TABLE position_snapshots ADD COLUMN contract_metadata_json TEXT;
ALTER TABLE position_snapshots ADD COLUMN liquidation_price TEXT;
ALTER TABLE position_snapshots ADD COLUMN open_interest TEXT;
