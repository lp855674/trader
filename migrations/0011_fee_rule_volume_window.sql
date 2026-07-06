ALTER TABLE fee_rules ADD COLUMN volume_window TEXT NOT NULL DEFAULT 'run';

CREATE INDEX IF NOT EXISTS idx_orders_account_symbol
ON orders(account_id, symbol);

CREATE INDEX IF NOT EXISTS idx_fills_order_ts
ON fills(order_id, ts_ms);
