CREATE TABLE IF NOT EXISTS strategy_runs (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    mode TEXT NOT NULL,
    status TEXT NOT NULL,
    started_at_ms INTEGER NOT NULL,
    ended_at_ms INTEGER,
    config_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS instruments (
    symbol TEXT PRIMARY KEY,
    market TEXT NOT NULL,
    exchange TEXT NOT NULL,
    asset_class TEXT NOT NULL,
    currency TEXT NOT NULL,
    lot_size TEXT NOT NULL,
    tick_size TEXT NOT NULL,
    tradable INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS orders (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL,
    client_order_id TEXT NOT NULL UNIQUE,
    broker_order_id TEXT,
    account_id TEXT NOT NULL,
    symbol TEXT NOT NULL,
    side TEXT NOT NULL,
    order_type TEXT NOT NULL,
    price TEXT,
    qty TEXT NOT NULL,
    filled_qty TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS fills (
    id TEXT PRIMARY KEY,
    order_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    symbol TEXT NOT NULL,
    side TEXT NOT NULL,
    price TEXT NOT NULL,
    qty TEXT NOT NULL,
    fee TEXT NOT NULL,
    ts_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS positions (
    run_id TEXT NOT NULL,
    account_id TEXT NOT NULL,
    symbol TEXT NOT NULL,
    qty TEXT NOT NULL,
    avg_price TEXT NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    PRIMARY KEY (run_id, account_id, symbol)
);

CREATE TABLE IF NOT EXISTS event_store (
    event_id TEXT PRIMARY KEY,
    ts_ms INTEGER NOT NULL,
    source TEXT NOT NULL,
    category TEXT NOT NULL,
    payload_json TEXT NOT NULL
);
