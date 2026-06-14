CREATE TABLE IF NOT EXISTS market_calendars (
    id TEXT PRIMARY KEY,
    market TEXT NOT NULL,
    trading_day TEXT NOT NULL,
    is_open INTEGER NOT NULL,
    session_template TEXT,
    UNIQUE(market, trading_day)
);

CREATE TABLE IF NOT EXISTS trading_sessions (
    id TEXT PRIMARY KEY,
    market TEXT NOT NULL,
    trading_day TEXT NOT NULL,
    session_name TEXT NOT NULL,
    open_time TEXT NOT NULL,
    close_time TEXT NOT NULL,
    timezone TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS fee_rules (
    id TEXT PRIMARY KEY,
    market TEXT NOT NULL,
    exchange TEXT NOT NULL,
    asset_class TEXT NOT NULL,
    maker_bps TEXT NOT NULL,
    taker_bps TEXT NOT NULL,
    effective_from_ms INTEGER NOT NULL,
    effective_to_ms INTEGER
);

CREATE TABLE IF NOT EXISTS lot_size_rules (
    id TEXT PRIMARY KEY,
    market TEXT NOT NULL,
    exchange TEXT NOT NULL,
    asset_class TEXT NOT NULL,
    symbol TEXT,
    lot_size TEXT NOT NULL,
    min_qty TEXT NOT NULL,
    min_notional TEXT NOT NULL,
    effective_from_ms INTEGER NOT NULL,
    effective_to_ms INTEGER
);

CREATE INDEX IF NOT EXISTS idx_lot_size_rules_lookup
ON lot_size_rules(market, exchange, asset_class, symbol, effective_from_ms);

CREATE TABLE IF NOT EXISTS price_limit_rules (
    id TEXT PRIMARY KEY,
    market TEXT NOT NULL,
    exchange TEXT NOT NULL,
    asset_class TEXT NOT NULL,
    symbol TEXT,
    tick_size TEXT NOT NULL,
    limit_up_bps TEXT,
    limit_down_bps TEXT,
    effective_from_ms INTEGER NOT NULL,
    effective_to_ms INTEGER
);

CREATE INDEX IF NOT EXISTS idx_price_limit_rules_lookup
ON price_limit_rules(market, exchange, asset_class, symbol, effective_from_ms);
