use crate::Db;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewInstrument {
    pub symbol: String,
    pub market: String,
    pub exchange: String,
    pub asset_class: String,
    pub currency: String,
    pub lot_size: String,
    pub tick_size: String,
    pub tradable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstrumentRecord {
    pub symbol: String,
    pub market: String,
    pub exchange: String,
    pub asset_class: String,
    pub currency: String,
    pub lot_size: String,
    pub tick_size: String,
    pub tradable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewStrategyRun {
    pub id: String,
    pub name: String,
    pub mode: String,
    pub status: String,
    pub started_at_ms: i64,
    pub ended_at_ms: Option<i64>,
    pub error: Option<String>,
    pub config_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StrategyRunRecord {
    pub id: String,
    pub name: String,
    pub mode: String,
    pub status: String,
    pub started_at_ms: i64,
    pub ended_at_ms: Option<i64>,
    pub error: Option<String>,
    pub config_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewOrder {
    pub id: String,
    pub run_id: String,
    pub client_order_id: String,
    pub broker_order_id: Option<String>,
    pub account_id: String,
    pub symbol: String,
    pub side: String,
    pub order_type: String,
    pub price: Option<String>,
    pub qty: String,
    pub filled_qty: String,
    pub status: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewFill {
    pub id: String,
    pub order_id: String,
    pub run_id: String,
    pub symbol: String,
    pub side: String,
    pub price: String,
    pub qty: String,
    pub fee: String,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewPosition {
    pub run_id: String,
    pub account_id: String,
    pub symbol: String,
    pub qty: String,
    pub avg_price: String,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewAccountBalance {
    pub run_id: String,
    pub account_id: String,
    pub asset: String,
    pub total: String,
    pub available: String,
    pub frozen: String,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewPortfolioSnapshot {
    pub id: String,
    pub run_id: String,
    pub account_id: String,
    pub ts_ms: i64,
    pub cash: String,
    pub market_value: String,
    pub equity: String,
    pub realized_pnl: String,
    pub unrealized_pnl: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewEventRecord {
    pub event_id: String,
    pub ts_ms: i64,
    pub source: String,
    pub category: String,
    pub payload_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EventRecord {
    pub event_id: String,
    pub ts_ms: i64,
    pub source: String,
    pub category: String,
    pub payload_json: String,
}

impl Db {
    pub async fn insert_instrument(&self, instrument: NewInstrument) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO instruments (
                symbol, market, exchange, asset_class, currency, lot_size, tick_size, tradable
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(instrument.symbol)
        .bind(instrument.market)
        .bind(instrument.exchange)
        .bind(instrument.asset_class)
        .bind(instrument.currency)
        .bind(instrument.lot_size)
        .bind(instrument.tick_size)
        .bind(instrument.tradable)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn get_instrument(
        &self,
        symbol: &str,
    ) -> Result<Option<InstrumentRecord>, sqlx::Error> {
        let row =
            sqlx::query_as::<_, (String, String, String, String, String, String, String, i64)>(
                r#"
            SELECT symbol, market, exchange, asset_class, currency, lot_size, tick_size, tradable
            FROM instruments
            WHERE symbol = ?
            "#,
            )
            .bind(symbol)
            .fetch_optional(self.pool())
            .await?;

        Ok(row.map(
            |(symbol, market, exchange, asset_class, currency, lot_size, tick_size, tradable)| {
                InstrumentRecord {
                    symbol,
                    market,
                    exchange,
                    asset_class,
                    currency,
                    lot_size,
                    tick_size,
                    tradable: tradable != 0,
                }
            },
        ))
    }

    pub async fn insert_strategy_run(&self, run: NewStrategyRun) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO strategy_runs (
                id, name, mode, status, started_at_ms, ended_at_ms, error, config_json
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(run.id)
        .bind(run.name)
        .bind(run.mode)
        .bind(run.status)
        .bind(run.started_at_ms)
        .bind(run.ended_at_ms)
        .bind(run.error)
        .bind(run.config_json)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn update_strategy_run_status(
        &self,
        run_id: &str,
        status: &str,
        ended_at_ms: Option<i64>,
        error: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE strategy_runs
            SET status = ?, ended_at_ms = ?, error = ?
            WHERE id = ?
            "#,
        )
        .bind(status)
        .bind(ended_at_ms)
        .bind(error)
        .bind(run_id)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn get_strategy_run(
        &self,
        run_id: &str,
    ) -> Result<Option<StrategyRunRecord>, sqlx::Error> {
        let row = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                String,
                i64,
                Option<i64>,
                Option<String>,
                String,
            ),
        >(
            r#"
            SELECT id, name, mode, status, started_at_ms, ended_at_ms, error, config_json
            FROM strategy_runs
            WHERE id = ?
            "#,
        )
        .bind(run_id)
        .fetch_optional(self.pool())
        .await?;

        Ok(row.map(
            |(id, name, mode, status, started_at_ms, ended_at_ms, error, config_json)| {
                StrategyRunRecord {
                    id,
                    name,
                    mode,
                    status,
                    started_at_ms,
                    ended_at_ms,
                    error,
                    config_json,
                }
            },
        ))
    }

    pub async fn list_strategy_runs(&self) -> Result<Vec<StrategyRunRecord>, sqlx::Error> {
        let rows = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                String,
                i64,
                Option<i64>,
                Option<String>,
                String,
            ),
        >(
            r#"
            SELECT id, name, mode, status, started_at_ms, ended_at_ms, error, config_json
            FROM strategy_runs
            ORDER BY started_at_ms DESC, id
            "#,
        )
        .fetch_all(self.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(id, name, mode, status, started_at_ms, ended_at_ms, error, config_json)| {
                    StrategyRunRecord {
                        id,
                        name,
                        mode,
                        status,
                        started_at_ms,
                        ended_at_ms,
                        error,
                        config_json,
                    }
                },
            )
            .collect())
    }

    pub async fn insert_order(&self, order: NewOrder) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO orders (
                id, run_id, client_order_id, broker_order_id, account_id, symbol, side,
                order_type, price, qty, filled_qty, status, created_at_ms, updated_at_ms
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(order.id)
        .bind(order.run_id)
        .bind(order.client_order_id)
        .bind(order.broker_order_id)
        .bind(order.account_id)
        .bind(order.symbol)
        .bind(order.side)
        .bind(order.order_type)
        .bind(order.price)
        .bind(order.qty)
        .bind(order.filled_qty)
        .bind(order.status)
        .bind(order.created_at_ms)
        .bind(order.updated_at_ms)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn insert_fill(&self, fill: NewFill) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO fills (
                id, order_id, run_id, symbol, side, price, qty, fee, ts_ms
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(fill.id)
        .bind(fill.order_id)
        .bind(fill.run_id)
        .bind(fill.symbol)
        .bind(fill.side)
        .bind(fill.price)
        .bind(fill.qty)
        .bind(fill.fee)
        .bind(fill.ts_ms)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn upsert_position(&self, position: NewPosition) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO positions (
                run_id, account_id, symbol, qty, avg_price, updated_at_ms
            ) VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT(run_id, account_id, symbol) DO UPDATE SET
                qty = excluded.qty,
                avg_price = excluded.avg_price,
                updated_at_ms = excluded.updated_at_ms
            "#,
        )
        .bind(position.run_id)
        .bind(position.account_id)
        .bind(position.symbol)
        .bind(position.qty)
        .bind(position.avg_price)
        .bind(position.updated_at_ms)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn upsert_account_balance(
        &self,
        balance: NewAccountBalance,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO account_balances (
                run_id, account_id, asset, total, available, frozen, updated_at_ms
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(run_id, account_id, asset) DO UPDATE SET
                total = excluded.total,
                available = excluded.available,
                frozen = excluded.frozen,
                updated_at_ms = excluded.updated_at_ms
            "#,
        )
        .bind(balance.run_id)
        .bind(balance.account_id)
        .bind(balance.asset)
        .bind(balance.total)
        .bind(balance.available)
        .bind(balance.frozen)
        .bind(balance.updated_at_ms)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn insert_portfolio_snapshot(
        &self,
        snapshot: NewPortfolioSnapshot,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO portfolio_snapshots (
                id, run_id, account_id, ts_ms, cash, market_value, equity,
                realized_pnl, unrealized_pnl
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(snapshot.id)
        .bind(snapshot.run_id)
        .bind(snapshot.account_id)
        .bind(snapshot.ts_ms)
        .bind(snapshot.cash)
        .bind(snapshot.market_value)
        .bind(snapshot.equity)
        .bind(snapshot.realized_pnl)
        .bind(snapshot.unrealized_pnl)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn insert_event(&self, event: NewEventRecord) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO event_store (
                event_id, ts_ms, source, category, payload_json
            ) VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(event.event_id)
        .bind(event.ts_ms)
        .bind(event.source)
        .bind(event.category)
        .bind(event.payload_json)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn list_orders(&self, run_id: &str) -> Result<Vec<NewOrder>, sqlx::Error> {
        let rows = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                Option<String>,
                String,
                String,
                String,
                String,
                Option<String>,
                String,
                String,
                String,
                i64,
                i64,
            ),
        >(
            r#"
            SELECT id, run_id, client_order_id, broker_order_id, account_id, symbol, side,
                   order_type, price, qty, filled_qty, status, created_at_ms, updated_at_ms
            FROM orders
            WHERE run_id = ?
            ORDER BY created_at_ms, id
            "#,
        )
        .bind(run_id)
        .fetch_all(self.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(
                    id,
                    run_id,
                    client_order_id,
                    broker_order_id,
                    account_id,
                    symbol,
                    side,
                    order_type,
                    price,
                    qty,
                    filled_qty,
                    status,
                    created_at_ms,
                    updated_at_ms,
                )| NewOrder {
                    id,
                    run_id,
                    client_order_id,
                    broker_order_id,
                    account_id,
                    symbol,
                    side,
                    order_type,
                    price,
                    qty,
                    filled_qty,
                    status,
                    created_at_ms,
                    updated_at_ms,
                },
            )
            .collect())
    }

    pub async fn list_fills(&self, run_id: &str) -> Result<Vec<NewFill>, sqlx::Error> {
        let rows = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                String,
                String,
                String,
                String,
                String,
                i64,
            ),
        >(
            r#"
            SELECT id, order_id, run_id, symbol, side, price, qty, fee, ts_ms
            FROM fills
            WHERE run_id = ?
            ORDER BY ts_ms, id
            "#,
        )
        .bind(run_id)
        .fetch_all(self.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(id, order_id, run_id, symbol, side, price, qty, fee, ts_ms)| NewFill {
                    id,
                    order_id,
                    run_id,
                    symbol,
                    side,
                    price,
                    qty,
                    fee,
                    ts_ms,
                },
            )
            .collect())
    }

    pub async fn list_positions(&self, run_id: &str) -> Result<Vec<NewPosition>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String, String, String, String, String, i64)>(
            r#"
            SELECT run_id, account_id, symbol, qty, avg_price, updated_at_ms
            FROM positions
            WHERE run_id = ?
            ORDER BY account_id, symbol
            "#,
        )
        .bind(run_id)
        .fetch_all(self.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(run_id, account_id, symbol, qty, avg_price, updated_at_ms)| NewPosition {
                    run_id,
                    account_id,
                    symbol,
                    qty,
                    avg_price,
                    updated_at_ms,
                },
            )
            .collect())
    }

    pub async fn list_account_balances(
        &self,
        run_id: &str,
    ) -> Result<Vec<NewAccountBalance>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String, String, String, String, String, String, i64)>(
            r#"
            SELECT run_id, account_id, asset, total, available, frozen, updated_at_ms
            FROM account_balances
            WHERE run_id = ?
            ORDER BY account_id, asset
            "#,
        )
        .bind(run_id)
        .fetch_all(self.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(run_id, account_id, asset, total, available, frozen, updated_at_ms)| {
                    NewAccountBalance {
                        run_id,
                        account_id,
                        asset,
                        total,
                        available,
                        frozen,
                        updated_at_ms,
                    }
                },
            )
            .collect())
    }

    pub async fn list_portfolio_snapshots(
        &self,
        run_id: &str,
    ) -> Result<Vec<NewPortfolioSnapshot>, sqlx::Error> {
        let rows = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                i64,
                String,
                String,
                String,
                String,
                String,
            ),
        >(
            r#"
            SELECT id, run_id, account_id, ts_ms, cash, market_value, equity,
                   realized_pnl, unrealized_pnl
            FROM portfolio_snapshots
            WHERE run_id = ?
            ORDER BY ts_ms, id
            "#,
        )
        .bind(run_id)
        .fetch_all(self.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(
                    id,
                    run_id,
                    account_id,
                    ts_ms,
                    cash,
                    market_value,
                    equity,
                    realized_pnl,
                    unrealized_pnl,
                )| NewPortfolioSnapshot {
                    id,
                    run_id,
                    account_id,
                    ts_ms,
                    cash,
                    market_value,
                    equity,
                    realized_pnl,
                    unrealized_pnl,
                },
            )
            .collect())
    }

    pub async fn list_events(&self) -> Result<Vec<EventRecord>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String, i64, String, String, String)>(
            r#"
            SELECT event_id, ts_ms, source, category, payload_json
            FROM event_store
            ORDER BY ts_ms, event_id
            "#,
        )
        .fetch_all(self.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(event_id, ts_ms, source, category, payload_json)| EventRecord {
                    event_id,
                    ts_ms,
                    source,
                    category,
                    payload_json,
                },
            )
            .collect())
    }

    pub async fn list_events_by_source(
        &self,
        source: &str,
    ) -> Result<Vec<EventRecord>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String, i64, String, String, String)>(
            r#"
            SELECT event_id, ts_ms, source, category, payload_json
            FROM event_store
            WHERE source = ?
            ORDER BY ts_ms, event_id
            "#,
        )
        .bind(source)
        .fetch_all(self.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(event_id, ts_ms, source, category, payload_json)| EventRecord {
                    event_id,
                    ts_ms,
                    source,
                    category,
                    payload_json,
                },
            )
            .collect())
    }
}
