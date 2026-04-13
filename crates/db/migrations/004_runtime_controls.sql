-- Runtime control tables

CREATE TABLE IF NOT EXISTS runtime_controls (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

CREATE TABLE IF NOT EXISTS symbol_allowlist (
    symbol TEXT PRIMARY KEY,
    enabled INTEGER NOT NULL CHECK(enabled IN (0,1)),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

CREATE TABLE IF NOT EXISTS reconciliation_snapshots (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL,
    broker_cash REAL NOT NULL,
    local_cash REAL NOT NULL,
    broker_positions_json TEXT NOT NULL,
    local_positions_json TEXT NOT NULL,
    mismatch_count INTEGER NOT NULL,
    status TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_reconciliation_account_created ON reconciliation_snapshots (account_id, created_at);
