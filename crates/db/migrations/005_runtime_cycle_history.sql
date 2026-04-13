CREATE TABLE IF NOT EXISTS runtime_cycle_runs (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL,
    venue TEXT NOT NULL,
    mode TEXT NOT NULL,
    triggered_at_ms INTEGER NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

CREATE TABLE IF NOT EXISTS runtime_cycle_symbols (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id TEXT NOT NULL REFERENCES runtime_cycle_runs(id) ON DELETE CASCADE,
    symbol TEXT NOT NULL,
    score REAL,
    confidence REAL,
    decision TEXT NOT NULL,
    reason TEXT,
    order_id TEXT
);

CREATE INDEX IF NOT EXISTS idx_runtime_cycle_runs_created_at
ON runtime_cycle_runs (triggered_at_ms DESC);

CREATE INDEX IF NOT EXISTS idx_runtime_cycle_symbols_run_id
ON runtime_cycle_symbols (run_id);
