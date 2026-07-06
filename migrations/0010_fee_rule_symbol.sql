ALTER TABLE fee_rules ADD COLUMN symbol TEXT;

CREATE INDEX IF NOT EXISTS idx_fee_rules_lookup
ON fee_rules(market, exchange, asset_class, symbol, effective_from_ms);
