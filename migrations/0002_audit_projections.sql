CREATE TABLE IF NOT EXISTS order_events (
    id TEXT PRIMARY KEY,
    event_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    order_id TEXT,
    client_order_id TEXT,
    broker_order_id TEXT,
    account_id TEXT,
    symbol TEXT,
    status TEXT NOT NULL,
    event_type TEXT NOT NULL,
    message TEXT,
    ts_ms INTEGER NOT NULL,
    payload_json TEXT NOT NULL,
    FOREIGN KEY(event_id) REFERENCES event_store(event_id)
);

CREATE INDEX IF NOT EXISTS idx_order_events_run_id
ON order_events(run_id);

CREATE INDEX IF NOT EXISTS idx_order_events_order_id
ON order_events(order_id);

CREATE INDEX IF NOT EXISTS idx_order_events_ts
ON order_events(ts_ms);

CREATE TABLE IF NOT EXISTS risk_events (
    id TEXT PRIMARY KEY,
    event_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    account_id TEXT,
    symbol TEXT,
    risk_type TEXT NOT NULL,
    decision TEXT NOT NULL,
    reason TEXT,
    threshold TEXT,
    observed_value TEXT,
    ts_ms INTEGER NOT NULL,
    payload_json TEXT NOT NULL,
    FOREIGN KEY(event_id) REFERENCES event_store(event_id)
);

CREATE INDEX IF NOT EXISTS idx_risk_events_run_id
ON risk_events(run_id);

CREATE INDEX IF NOT EXISTS idx_risk_events_symbol
ON risk_events(symbol);

CREATE INDEX IF NOT EXISTS idx_risk_events_ts
ON risk_events(ts_ms);

CREATE TABLE IF NOT EXISTS insights (
    id TEXT PRIMARY KEY,
    event_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    strategy TEXT NOT NULL,
    symbol TEXT NOT NULL,
    side TEXT NOT NULL,
    confidence TEXT NOT NULL,
    ts_ms INTEGER NOT NULL,
    payload_json TEXT NOT NULL,
    FOREIGN KEY(event_id) REFERENCES event_store(event_id)
);

CREATE INDEX IF NOT EXISTS idx_insights_run_symbol_ts
ON insights(run_id, symbol, ts_ms);

CREATE TABLE IF NOT EXISTS portfolio_targets (
    id TEXT PRIMARY KEY,
    event_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    account_id TEXT NOT NULL,
    symbol TEXT NOT NULL,
    target_qty TEXT NOT NULL,
    ts_ms INTEGER NOT NULL,
    payload_json TEXT NOT NULL,
    FOREIGN KEY(event_id) REFERENCES event_store(event_id)
);

CREATE INDEX IF NOT EXISTS idx_portfolio_targets_run_symbol_ts
ON portfolio_targets(run_id, symbol, ts_ms);
