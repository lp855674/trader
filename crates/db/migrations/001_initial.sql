CREATE TABLE IF NOT EXISTS instruments (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    venue TEXT NOT NULL,
    symbol TEXT NOT NULL,
    meta_json TEXT,
    UNIQUE(venue, symbol)
);

CREATE TABLE IF NOT EXISTS data_sources (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    config_json TEXT
);

CREATE TABLE IF NOT EXISTS execution_profiles (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    config_json TEXT
);

CREATE TABLE IF NOT EXISTS accounts (
    id TEXT PRIMARY KEY,
    mode TEXT NOT NULL,
    execution_profile_id TEXT NOT NULL REFERENCES execution_profiles(id),
    venue TEXT
);

CREATE TABLE IF NOT EXISTS bars (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    instrument_id INTEGER NOT NULL REFERENCES instruments(id),
    data_source_id TEXT NOT NULL REFERENCES data_sources(id),
    ts_ms INTEGER NOT NULL,
    o REAL NOT NULL,
    h REAL NOT NULL,
    l REAL NOT NULL,
    c REAL NOT NULL,
    volume REAL NOT NULL DEFAULT 0,
    UNIQUE(instrument_id, data_source_id, ts_ms)
);

CREATE TABLE IF NOT EXISTS signals (
    id TEXT PRIMARY KEY,
    instrument_id INTEGER NOT NULL REFERENCES instruments(id),
    strategy_id TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS risk_decisions (
    id TEXT PRIMARY KEY,
    signal_id TEXT NOT NULL REFERENCES signals(id),
    allow INTEGER NOT NULL,
    reason TEXT,
    created_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS orders (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts(id),
    instrument_id INTEGER NOT NULL REFERENCES instruments(id),
    side TEXT NOT NULL,
    qty REAL NOT NULL,
    status TEXT NOT NULL,
    idempotency_key TEXT,
    created_at_ms INTEGER NOT NULL,
    UNIQUE(account_id, idempotency_key)
);

CREATE TABLE IF NOT EXISTS fills (
    id TEXT PRIMARY KEY,
    order_id TEXT NOT NULL REFERENCES orders(id),
    qty REAL NOT NULL,
    price REAL NOT NULL,
    created_at_ms INTEGER NOT NULL
);
