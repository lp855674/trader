CREATE TABLE IF NOT EXISTS crypto_positions (
    run_id TEXT NOT NULL,
    account_id TEXT NOT NULL,
    exchange TEXT NOT NULL,
    symbol TEXT NOT NULL,
    asset_class TEXT NOT NULL,
    margin_mode TEXT NOT NULL,
    position_side TEXT NOT NULL,
    leverage TEXT NOT NULL,
    qty TEXT NOT NULL,
    avg_price TEXT NOT NULL,
    margin_used TEXT NOT NULL,
    funding_fee TEXT NOT NULL DEFAULT '0',
    realized_pnl TEXT NOT NULL DEFAULT '0',
    unrealized_pnl TEXT NOT NULL DEFAULT '0',
    updated_at_ms INTEGER NOT NULL,
    PRIMARY KEY (run_id, account_id, exchange, symbol, position_side)
);

CREATE TABLE IF NOT EXISTS funding_rates (
    id TEXT PRIMARY KEY,
    exchange TEXT NOT NULL,
    symbol TEXT NOT NULL,
    funding_time_ms INTEGER NOT NULL,
    funding_rate TEXT NOT NULL,
    mark_price TEXT,
    source TEXT NOT NULL,
    UNIQUE(exchange, symbol, funding_time_ms)
);

CREATE INDEX IF NOT EXISTS idx_funding_rates_symbol_time
ON funding_rates(exchange, symbol, funding_time_ms);
