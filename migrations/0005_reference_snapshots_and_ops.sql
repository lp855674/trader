CREATE TABLE IF NOT EXISTS crypto_market_meta (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    exchange TEXT NOT NULL,
    symbol TEXT NOT NULL,
    base_asset TEXT NOT NULL,
    quote_asset TEXT NOT NULL,
    instrument_type TEXT NOT NULL,
    contract_type TEXT,
    contract_size TEXT,
    settlement_asset TEXT,
    min_notional TEXT,
    min_qty TEXT,
    max_qty TEXT,
    price_precision INTEGER,
    qty_precision INTEGER,
    price_tick TEXT,
    qty_step TEXT,
    maker_fee_rate TEXT,
    taker_fee_rate TEXT,
    funding_interval_hours INTEGER,
    max_leverage TEXT,
    margin_modes TEXT,
    is_inverse INTEGER NOT NULL DEFAULT 0,
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE(exchange, symbol)
);

CREATE INDEX IF NOT EXISTS idx_crypto_market_meta_exchange
ON crypto_market_meta(exchange);

CREATE INDEX IF NOT EXISTS idx_crypto_market_meta_symbol
ON crypto_market_meta(symbol);

CREATE INDEX IF NOT EXISTS idx_crypto_market_meta_type
ON crypto_market_meta(instrument_type);

CREATE TABLE IF NOT EXISTS corporate_actions_meta (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    market TEXT NOT NULL,
    exchange TEXT NOT NULL,
    symbol TEXT NOT NULL,
    action_type TEXT NOT NULL,
    ex_date INTEGER NOT NULL,
    record_date INTEGER,
    payable_date INTEGER,
    ratio TEXT,
    cash_amount TEXT,
    currency TEXT,
    source TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_corporate_actions_symbol_date
ON corporate_actions_meta(market, symbol, ex_date);

CREATE TABLE IF NOT EXISTS cash_snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id TEXT NOT NULL,
    ts INTEGER NOT NULL,
    currency TEXT NOT NULL,
    cash TEXT NOT NULL,
    available_cash TEXT NOT NULL,
    frozen_cash TEXT NOT NULL DEFAULT '0',
    created_at INTEGER NOT NULL,
    FOREIGN KEY(run_id) REFERENCES strategy_runs(id)
);

CREATE INDEX IF NOT EXISTS idx_cash_snapshots_run_ts
ON cash_snapshots(run_id, ts);

CREATE TABLE IF NOT EXISTS position_snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id TEXT NOT NULL,
    ts INTEGER NOT NULL,
    market TEXT NOT NULL,
    exchange TEXT NOT NULL,
    symbol TEXT NOT NULL,
    asset_class TEXT NOT NULL,
    position_side TEXT,
    qty TEXT NOT NULL,
    available_qty TEXT NOT NULL,
    avg_price TEXT,
    entry_price TEXT,
    market_price TEXT,
    mark_price TEXT,
    market_value TEXT,
    unrealized_pnl TEXT,
    realized_pnl TEXT,
    currency TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    FOREIGN KEY(run_id) REFERENCES strategy_runs(id)
);

CREATE INDEX IF NOT EXISTS idx_position_snapshots_run_ts
ON position_snapshots(run_id, ts);

CREATE INDEX IF NOT EXISTS idx_position_snapshots_symbol_ts
ON position_snapshots(market, symbol, ts);

CREATE INDEX IF NOT EXISTS idx_position_snapshots_asset_class
ON position_snapshots(asset_class);

CREATE TABLE IF NOT EXISTS configs (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    config_type TEXT NOT NULL,
    content TEXT NOT NULL,
    format TEXT NOT NULL,
    checksum TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_configs_name
ON configs(name);

CREATE INDEX IF NOT EXISTS idx_configs_type
ON configs(config_type);

CREATE TABLE IF NOT EXISTS system_logs (
    id TEXT PRIMARY KEY,
    run_id TEXT,
    ts INTEGER NOT NULL,
    level TEXT NOT NULL,
    target TEXT NOT NULL,
    message TEXT NOT NULL,
    fields_json TEXT,
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_system_logs_run_id
ON system_logs(run_id);

CREATE INDEX IF NOT EXISTS idx_system_logs_ts
ON system_logs(ts);

CREATE INDEX IF NOT EXISTS idx_system_logs_level
ON system_logs(level);
