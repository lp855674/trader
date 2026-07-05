CREATE TABLE IF NOT EXISTS fee_rule_tiers (
    id TEXT PRIMARY KEY,
    fee_rule_id TEXT NOT NULL,
    volume_from TEXT NOT NULL,
    volume_to TEXT,
    maker_bps TEXT NOT NULL,
    taker_bps TEXT NOT NULL,
    FOREIGN KEY(fee_rule_id) REFERENCES fee_rules(id)
);

CREATE INDEX IF NOT EXISTS idx_fee_rule_tiers_lookup
ON fee_rule_tiers(fee_rule_id, volume_from);
