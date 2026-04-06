-- Execution subsystem schema
-- exec_orders tracks order lifecycle with full state machine fields
-- exec_fills tracks individual fill events

CREATE TABLE IF NOT EXISTS exec_orders (
    id TEXT PRIMARY KEY,
    client_order_id TEXT NOT NULL,
    instrument TEXT NOT NULL,
    side TEXT NOT NULL,
    quantity REAL NOT NULL,
    kind TEXT NOT NULL,
    venue TEXT NOT NULL DEFAULT 'paper',
    state TEXT NOT NULL,
    filled_qty REAL NOT NULL DEFAULT 0,
    avg_fill_price REAL NOT NULL DEFAULT 0,
    strategy_id TEXT NOT NULL,
    created_ts_ms INTEGER NOT NULL,
    updated_ts_ms INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_exec_orders_instrument ON exec_orders(instrument);
CREATE INDEX IF NOT EXISTS idx_exec_orders_strategy ON exec_orders(strategy_id);
CREATE INDEX IF NOT EXISTS idx_exec_orders_state ON exec_orders(state);

CREATE TABLE IF NOT EXISTS exec_fills (
    order_id TEXT NOT NULL REFERENCES exec_orders(id),
    instrument TEXT NOT NULL,
    side TEXT NOT NULL,
    qty REAL NOT NULL,
    price REAL NOT NULL,
    commission REAL NOT NULL,
    ts_ms INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_exec_fills_order_id ON exec_fills(order_id);
CREATE INDEX IF NOT EXISTS idx_exec_fills_instrument ON exec_fills(instrument);
