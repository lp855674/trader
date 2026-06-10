use crate::{Db, StorageError, StorageResult};
use chrono::{TimeZone, Utc};
use events::{AnyEventEnvelope, EventBus, EventCategory, EventEnvelope, RuntimeEvent, TraderEvent};
use rust_decimal::Decimal;
use serde::Serialize;
use trader_core::OrderRequest;
use uuid::Uuid;

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
pub struct RecoveredOrderState {
    pub id: String,
    pub run_id: String,
    pub order_qty: String,
    pub filled_qty: String,
    pub status: String,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredRuntimeEvent {
    pub ts_ms: i64,
    pub category: String,
    pub payload_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BacktestExecutionRecord {
    pub run_id: String,
    pub order_id: String,
    pub fill_id: String,
    pub broker_order_id: String,
    pub account_id: String,
    pub symbol: String,
    pub side: String,
    pub order_type: String,
    pub price: Option<String>,
    pub qty: String,
    pub fill_price: String,
    pub fee: String,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BacktestPositionRecord {
    pub run_id: String,
    pub account_id: String,
    pub symbol: String,
    pub qty: String,
    pub avg_price: String,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BacktestFilledExecutionCommand {
    pub run_id: String,
    pub order_id: String,
    pub fill_id: String,
    pub broker_order_id: String,
    pub order: OrderRequest,
    pub fill_price: Decimal,
    pub fee: Decimal,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BacktestPositionCommand {
    pub run_id: String,
    pub account_id: String,
    pub symbol: String,
    pub qty: Decimal,
    pub avg_price: Decimal,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeEventCommand {
    pub source: String,
    pub ts_ms: i64,
    pub category: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveRunCommand {
    pub run_id: String,
    pub started_at_ms: i64,
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrategyRunStartCommand {
    pub run_id: String,
    pub name: String,
    pub mode: String,
    pub started_at_ms: i64,
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalOrderCommand {
    pub run_id: String,
    pub order_id: String,
    pub client_order_id: String,
    pub broker_order_id: Option<String>,
    pub account_id: String,
    pub symbol: String,
    pub side: String,
    pub order_type: String,
    pub price: Option<Decimal>,
    pub qty: Decimal,
    pub filled_qty: Decimal,
    pub status: String,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalFillCommand {
    pub id: String,
    pub order_id: String,
    pub run_id: String,
    pub symbol: String,
    pub side: String,
    pub price: Decimal,
    pub qty: Decimal,
    pub fee: Decimal,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountBalanceCommand {
    pub run_id: String,
    pub account_id: String,
    pub asset: String,
    pub total: Decimal,
    pub available: Decimal,
    pub frozen: Decimal,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PositionCommand {
    pub run_id: String,
    pub account_id: String,
    pub symbol: String,
    pub qty: Decimal,
    pub avg_price: Decimal,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortfolioSnapshotCommand {
    pub id: String,
    pub run_id: String,
    pub account_id: String,
    pub ts_ms: i64,
    pub cash: Decimal,
    pub market_value: Decimal,
    pub equity: Decimal,
    pub realized_pnl: Decimal,
    pub unrealized_pnl: Decimal,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PaperOrderCommand {
    pub run_id: String,
    pub order_id: String,
    pub client_order_id: String,
    pub order: OrderRequest,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PaperFailedOrderCommand {
    pub run_id: String,
    pub order_id: String,
    pub client_order_id: String,
    pub order: OrderRequest,
    pub error: String,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PaperExecutionCommand {
    pub run_id: String,
    pub order_id: String,
    pub fill_id: String,
    pub client_order_id: String,
    pub order: OrderRequest,
    pub broker_order_id: String,
    pub status: String,
    pub price: Decimal,
    pub qty: Decimal,
    pub fee: Decimal,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaperPortfolioSnapshotCommand {
    pub run_id: String,
    pub account_id: String,
    pub ts_ms: i64,
    pub cash: Decimal,
    pub market_value: Decimal,
    pub equity: Decimal,
    pub realized_pnl: Decimal,
    pub unrealized_pnl: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaperFinalStateCommand {
    pub run_id: String,
    pub strategy_name: String,
    pub account_id: String,
    pub symbol: String,
    pub base_currency: String,
    pub started_at_ms: i64,
    pub ended_at_ms: i64,
    pub config_json: String,
    pub cash: Decimal,
    pub market_value: Decimal,
    pub equity: Decimal,
    pub realized_pnl: Decimal,
    pub unrealized_pnl: Decimal,
    pub position_qty: Decimal,
    pub position_avg_price: Decimal,
}

struct PaperOrderRecordInput<'a> {
    run_id: &'a str,
    order_id: &'a str,
    client_order_id: &'a str,
    broker_order_id: Option<String>,
    order: &'a OrderRequest,
    filled_qty: Decimal,
    status: &'a str,
    ts_ms: i64,
}

fn paper_order_record(input: PaperOrderRecordInput<'_>) -> NewOrder {
    NewOrder {
        id: input.order_id.to_string(),
        run_id: input.run_id.to_string(),
        client_order_id: input.client_order_id.to_string(),
        broker_order_id: input.broker_order_id,
        account_id: input.order.account_id.clone(),
        symbol: input.order.symbol.clone(),
        side: order_side(input.order),
        order_type: order_type(input.order),
        price: input.order.price.map(|price| price.to_string()),
        qty: input.order.qty.to_string(),
        filled_qty: input.filled_qty.to_string(),
        status: input.status.to_string(),
        created_at_ms: input.ts_ms,
        updated_at_ms: input.ts_ms,
    }
}

#[allow(clippy::too_many_arguments)]
fn paper_order_event_payload(
    run_id: &str,
    order_id: &str,
    client_order_id: &str,
    broker_order_id: Option<&str>,
    order: &OrderRequest,
    filled_qty: Decimal,
    status: &str,
    error: Option<&str>,
) -> serde_json::Value {
    serde_json::json!({
        "run_id": run_id,
        "order_id": order_id,
        "client_order_id": client_order_id,
        "broker_order_id": broker_order_id,
        "account_id": &order.account_id,
        "symbol": &order.symbol,
        "side": order_side(order),
        "order_type": order_type(order),
        "qty": order.qty.to_string(),
        "filled_qty": filled_qty.to_string(),
        "status": status,
        "error": error
    })
}

fn order_side(order: &OrderRequest) -> String {
    format!("{:?}", order.side).to_uppercase()
}

fn order_type(order: &OrderRequest) -> String {
    format!("{:?}", order.order_type).to_uppercase()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BacktestCompletedRun {
    pub run_id: String,
    pub strategy_name: String,
    pub started_at_ms: i64,
    pub ended_at_ms: i64,
    pub config_json: String,
}

impl Db {
    pub async fn insert_instrument(&self, instrument: NewInstrument) -> StorageResult<()> {
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

    pub async fn get_instrument(&self, symbol: &str) -> StorageResult<Option<InstrumentRecord>> {
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

    pub async fn insert_strategy_run(&self, run: NewStrategyRun) -> StorageResult<()> {
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
    ) -> StorageResult<()> {
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

    pub async fn get_strategy_run(&self, run_id: &str) -> StorageResult<Option<StrategyRunRecord>> {
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

    pub async fn list_strategy_runs(&self) -> StorageResult<Vec<StrategyRunRecord>> {
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

    pub async fn insert_order(&self, order: NewOrder) -> StorageResult<()> {
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

    pub async fn update_order_status_by_broker_id(
        &self,
        run_id: &str,
        broker_order_id: &str,
        status: &str,
        updated_at_ms: i64,
    ) -> StorageResult<()> {
        sqlx::query(
            r#"
            UPDATE orders
            SET status = ?, updated_at_ms = ?
            WHERE run_id = ? AND broker_order_id = ?
            "#,
        )
        .bind(status)
        .bind(updated_at_ms)
        .bind(run_id)
        .bind(broker_order_id)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn update_order_status_by_client_order_id(
        &self,
        run_id: &str,
        client_order_id: &str,
        broker_order_id: &str,
        status: &str,
        updated_at_ms: i64,
    ) -> StorageResult<u64> {
        let result = sqlx::query(
            r#"
            UPDATE orders
            SET broker_order_id = ?, status = ?, updated_at_ms = ?
            WHERE run_id = ? AND client_order_id = ?
            "#,
        )
        .bind(broker_order_id)
        .bind(status)
        .bind(updated_at_ms)
        .bind(run_id)
        .bind(client_order_id)
        .execute(self.pool())
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn update_order_execution_by_broker_id(
        &self,
        run_id: &str,
        broker_order_id: &str,
        status: &str,
        filled_qty: &str,
        updated_at_ms: i64,
    ) -> StorageResult<()> {
        sqlx::query(
            r#"
            UPDATE orders
            SET status = ?, filled_qty = ?, updated_at_ms = ?
            WHERE run_id = ? AND broker_order_id = ?
            "#,
        )
        .bind(status)
        .bind(filled_qty)
        .bind(updated_at_ms)
        .bind(run_id)
        .bind(broker_order_id)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn update_order_execution_by_client_order_id(
        &self,
        client_order_id: &str,
        broker_order_id: &str,
        status: &str,
        filled_qty: &str,
        updated_at_ms: i64,
    ) -> StorageResult<()> {
        sqlx::query(
            r#"
            UPDATE orders
            SET broker_order_id = ?, status = ?, filled_qty = ?, updated_at_ms = ?
            WHERE client_order_id = ?
            "#,
        )
        .bind(broker_order_id)
        .bind(status)
        .bind(filled_qty)
        .bind(updated_at_ms)
        .bind(client_order_id)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn insert_fill(&self, fill: NewFill) -> StorageResult<()> {
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

    pub async fn upsert_position(&self, position: NewPosition) -> StorageResult<()> {
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

    pub async fn upsert_account_balance(&self, balance: NewAccountBalance) -> StorageResult<()> {
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
    ) -> StorageResult<()> {
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

    pub async fn insert_event(&self, event: NewEventRecord) -> StorageResult<()> {
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

    pub async fn record_runtime_event(&self, command: RuntimeEventCommand) -> StorageResult<()> {
        self.insert_event(NewEventRecord {
            event_id: Uuid::new_v4().to_string(),
            ts_ms: command.ts_ms,
            source: command.source,
            category: command.category,
            payload_json: command.payload.to_string(),
        })
        .await
    }

    pub async fn start_strategy_run(&self, command: StrategyRunStartCommand) -> StorageResult<()> {
        self.insert_strategy_run(NewStrategyRun {
            id: command.run_id,
            name: command.name,
            mode: command.mode,
            status: "running".to_string(),
            started_at_ms: command.started_at_ms,
            ended_at_ms: None,
            error: None,
            config_json: command.config.to_string(),
        })
        .await
    }

    pub async fn start_live_run(&self, command: LiveRunCommand) -> StorageResult<()> {
        self.insert_strategy_run(NewStrategyRun {
            id: command.run_id,
            name: "live".to_string(),
            mode: "live".to_string(),
            status: "running".to_string(),
            started_at_ms: command.started_at_ms,
            ended_at_ms: None,
            error: None,
            config_json: command.config.to_string(),
        })
        .await
    }

    pub async fn record_external_order(&self, command: ExternalOrderCommand) -> StorageResult<()> {
        self.insert_order(NewOrder {
            id: command.order_id,
            run_id: command.run_id,
            client_order_id: command.client_order_id,
            broker_order_id: command.broker_order_id,
            account_id: command.account_id,
            symbol: command.symbol,
            side: command.side,
            order_type: command.order_type,
            price: command.price.map(|price| price.to_string()),
            qty: command.qty.to_string(),
            filled_qty: command.filled_qty.to_string(),
            status: command.status,
            created_at_ms: command.ts_ms,
            updated_at_ms: command.ts_ms,
        })
        .await
    }

    pub async fn record_external_fill(&self, command: ExternalFillCommand) -> StorageResult<()> {
        self.insert_fill(NewFill {
            id: command.id,
            order_id: command.order_id,
            run_id: command.run_id,
            symbol: command.symbol,
            side: command.side,
            price: command.price.to_string(),
            qty: command.qty.to_string(),
            fee: command.fee.to_string(),
            ts_ms: command.ts_ms,
        })
        .await
    }

    pub async fn record_account_balance(
        &self,
        command: AccountBalanceCommand,
    ) -> StorageResult<()> {
        self.upsert_account_balance(NewAccountBalance {
            run_id: command.run_id,
            account_id: command.account_id,
            asset: command.asset,
            total: command.total.to_string(),
            available: command.available.to_string(),
            frozen: command.frozen.to_string(),
            updated_at_ms: command.updated_at_ms,
        })
        .await
    }

    pub async fn record_position(&self, command: PositionCommand) -> StorageResult<()> {
        self.upsert_position(NewPosition {
            run_id: command.run_id,
            account_id: command.account_id,
            symbol: command.symbol,
            qty: command.qty.to_string(),
            avg_price: command.avg_price.to_string(),
            updated_at_ms: command.updated_at_ms,
        })
        .await
    }

    pub async fn record_portfolio_snapshot(
        &self,
        command: PortfolioSnapshotCommand,
    ) -> StorageResult<()> {
        self.insert_portfolio_snapshot(NewPortfolioSnapshot {
            id: command.id,
            run_id: command.run_id,
            account_id: command.account_id,
            ts_ms: command.ts_ms,
            cash: command.cash.to_string(),
            market_value: command.market_value.to_string(),
            equity: command.equity.to_string(),
            realized_pnl: command.realized_pnl.to_string(),
            unrealized_pnl: command.unrealized_pnl.to_string(),
        })
        .await
    }

    pub async fn record_paper_order_submitted(
        &self,
        command: PaperOrderCommand,
    ) -> StorageResult<()> {
        self.insert_order(paper_order_record(PaperOrderRecordInput {
            run_id: &command.run_id,
            order_id: &command.order_id,
            client_order_id: &command.client_order_id,
            broker_order_id: None,
            order: &command.order,
            filled_qty: Decimal::ZERO,
            status: "SUBMITTED",
            ts_ms: command.ts_ms,
        }))
        .await?;
        self.insert_event(NewEventRecord {
            event_id: Uuid::new_v4().to_string(),
            ts_ms: command.ts_ms,
            source: command.run_id.clone(),
            category: "broker.order.submitted".to_string(),
            payload_json: paper_order_event_payload(
                &command.run_id,
                &command.order_id,
                &command.client_order_id,
                None,
                &command.order,
                Decimal::ZERO,
                "SUBMITTED",
                None,
            )
            .to_string(),
        })
        .await
    }

    pub async fn record_paper_order_failed(
        &self,
        command: PaperFailedOrderCommand,
    ) -> StorageResult<()> {
        self.insert_order(paper_order_record(PaperOrderRecordInput {
            run_id: &command.run_id,
            order_id: &command.order_id,
            client_order_id: &command.client_order_id,
            broker_order_id: None,
            order: &command.order,
            filled_qty: Decimal::ZERO,
            status: "FAILED",
            ts_ms: command.ts_ms,
        }))
        .await?;
        self.insert_event(NewEventRecord {
            event_id: Uuid::new_v4().to_string(),
            ts_ms: command.ts_ms,
            source: command.run_id.clone(),
            category: "broker.order.failed".to_string(),
            payload_json: paper_order_event_payload(
                &command.run_id,
                &command.order_id,
                &command.client_order_id,
                None,
                &command.order,
                Decimal::ZERO,
                "FAILED",
                Some(&command.error),
            )
            .to_string(),
        })
        .await
    }

    pub async fn record_paper_execution_result(
        &self,
        command: PaperExecutionCommand,
    ) -> StorageResult<()> {
        self.insert_order(paper_order_record(PaperOrderRecordInput {
            run_id: &command.run_id,
            order_id: &command.order_id,
            client_order_id: &command.client_order_id,
            broker_order_id: Some(command.broker_order_id.clone()),
            order: &command.order,
            filled_qty: command.qty,
            status: &command.status,
            ts_ms: command.ts_ms,
        }))
        .await?;

        if command.qty > Decimal::ZERO {
            let symbol = command.order.symbol.clone();
            let side = order_side(&command.order);
            self.insert_fill(NewFill {
                id: command.fill_id,
                order_id: command.order_id,
                run_id: command.run_id,
                symbol,
                side,
                price: command.price.to_string(),
                qty: command.qty.to_string(),
                fee: command.fee.to_string(),
                ts_ms: command.ts_ms,
            })
            .await?;
        }
        Ok(())
    }

    pub async fn record_paper_portfolio_snapshot(
        &self,
        command: PaperPortfolioSnapshotCommand,
    ) -> StorageResult<()> {
        self.insert_portfolio_snapshot(NewPortfolioSnapshot {
            id: format!("{}-snapshot-{}", command.run_id, command.ts_ms),
            run_id: command.run_id,
            account_id: command.account_id,
            ts_ms: command.ts_ms,
            cash: command.cash.to_string(),
            market_value: command.market_value.to_string(),
            equity: command.equity.to_string(),
            realized_pnl: command.realized_pnl.to_string(),
            unrealized_pnl: command.unrealized_pnl.to_string(),
        })
        .await
    }

    pub async fn complete_paper_run(&self, command: PaperFinalStateCommand) -> StorageResult<()> {
        self.insert_strategy_run(NewStrategyRun {
            id: command.run_id.clone(),
            name: command.strategy_name,
            mode: "paper".to_string(),
            status: "completed".to_string(),
            started_at_ms: command.started_at_ms,
            ended_at_ms: Some(command.ended_at_ms),
            error: None,
            config_json: command.config_json,
        })
        .await?;

        self.upsert_account_balance(NewAccountBalance {
            run_id: command.run_id.clone(),
            account_id: command.account_id.clone(),
            asset: command.base_currency,
            total: command.cash.to_string(),
            available: command.cash.to_string(),
            frozen: Decimal::ZERO.to_string(),
            updated_at_ms: command.ended_at_ms,
        })
        .await?;

        self.upsert_position(NewPosition {
            run_id: command.run_id.clone(),
            account_id: command.account_id.clone(),
            symbol: command.symbol,
            qty: command.position_qty.to_string(),
            avg_price: command.position_avg_price.to_string(),
            updated_at_ms: command.ended_at_ms,
        })
        .await?;

        self.insert_portfolio_snapshot(NewPortfolioSnapshot {
            id: format!("{}-snapshot-final", command.run_id),
            run_id: command.run_id,
            account_id: command.account_id,
            ts_ms: command.ended_at_ms,
            cash: command.cash.to_string(),
            market_value: command.market_value.to_string(),
            equity: command.equity.to_string(),
            realized_pnl: command.realized_pnl.to_string(),
            unrealized_pnl: command.unrealized_pnl.to_string(),
        })
        .await
    }

    pub async fn insert_runtime_events(
        &self,
        source: &str,
        events: &[StoredRuntimeEvent],
    ) -> StorageResult<()> {
        for event in events {
            self.insert_event(NewEventRecord {
                event_id: Uuid::new_v4().to_string(),
                ts_ms: event.ts_ms,
                source: source.to_string(),
                category: event.category.clone(),
                payload_json: event.payload_json.clone(),
            })
            .await?;
        }
        Ok(())
    }

    pub async fn insert_filled_backtest_execution(
        &self,
        execution: BacktestExecutionRecord,
    ) -> StorageResult<()> {
        self.insert_order(NewOrder {
            id: execution.order_id.clone(),
            run_id: execution.run_id.clone(),
            client_order_id: execution.order_id.clone(),
            broker_order_id: Some(execution.broker_order_id),
            account_id: execution.account_id,
            symbol: execution.symbol.clone(),
            side: execution.side.clone(),
            order_type: execution.order_type,
            price: execution.price,
            qty: execution.qty.clone(),
            filled_qty: execution.qty.clone(),
            status: "FILLED".to_string(),
            created_at_ms: execution.ts_ms,
            updated_at_ms: execution.ts_ms,
        })
        .await?;

        self.insert_fill(NewFill {
            id: execution.fill_id,
            order_id: execution.order_id,
            run_id: execution.run_id,
            symbol: execution.symbol,
            side: execution.side,
            price: execution.fill_price,
            qty: execution.qty,
            fee: execution.fee,
            ts_ms: execution.ts_ms,
        })
        .await
    }

    pub async fn record_backtest_filled_execution(
        &self,
        command: BacktestFilledExecutionCommand,
    ) -> StorageResult<()> {
        self.insert_order(paper_order_record(PaperOrderRecordInput {
            run_id: &command.run_id,
            order_id: &command.order_id,
            client_order_id: &command.order_id,
            broker_order_id: Some(command.broker_order_id),
            order: &command.order,
            filled_qty: command.order.qty,
            status: "FILLED",
            ts_ms: command.ts_ms,
        }))
        .await?;

        self.insert_fill(NewFill {
            id: command.fill_id,
            order_id: command.order_id,
            run_id: command.run_id,
            symbol: command.order.symbol.clone(),
            side: order_side(&command.order),
            price: command.fill_price.to_string(),
            qty: command.order.qty.to_string(),
            fee: command.fee.to_string(),
            ts_ms: command.ts_ms,
        })
        .await
    }

    pub async fn upsert_backtest_position(
        &self,
        position: BacktestPositionRecord,
    ) -> StorageResult<()> {
        self.upsert_position(NewPosition {
            run_id: position.run_id,
            account_id: position.account_id,
            symbol: position.symbol,
            qty: position.qty,
            avg_price: position.avg_price,
            updated_at_ms: position.updated_at_ms,
        })
        .await
    }

    pub async fn record_backtest_position(
        &self,
        command: BacktestPositionCommand,
    ) -> StorageResult<()> {
        self.upsert_position(NewPosition {
            run_id: command.run_id,
            account_id: command.account_id,
            symbol: command.symbol,
            qty: command.qty.to_string(),
            avg_price: command.avg_price.to_string(),
            updated_at_ms: command.updated_at_ms,
        })
        .await
    }

    pub async fn complete_backtest_run(&self, run: BacktestCompletedRun) -> StorageResult<()> {
        self.insert_strategy_run(NewStrategyRun {
            id: run.run_id,
            name: run.strategy_name,
            mode: "backtest".to_string(),
            status: "completed".to_string(),
            started_at_ms: run.started_at_ms,
            ended_at_ms: Some(run.ended_at_ms),
            error: None,
            config_json: run.config_json,
        })
        .await
    }

    pub async fn list_orders(&self, run_id: &str) -> StorageResult<Vec<NewOrder>> {
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

    pub async fn get_order_by_client_order_id(
        &self,
        client_order_id: &str,
    ) -> StorageResult<Option<NewOrder>> {
        let row = sqlx::query_as::<
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
            WHERE client_order_id = ?
            "#,
        )
        .bind(client_order_id)
        .fetch_optional(self.pool())
        .await?;

        Ok(row.map(
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
        ))
    }

    pub async fn list_recoverable_orders(&self, run_id: &str) -> StorageResult<Vec<NewOrder>> {
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
            WHERE run_id = ? AND status IN ('SUBMITTED', 'NEW', 'PARTIALLY_FILLED')
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

    pub async fn recover_order_state(
        &self,
        run_id: &str,
        order_id: &str,
    ) -> StorageResult<Option<RecoveredOrderState>> {
        let row = sqlx::query_as::<_, (String, String, String, String, String, i64)>(
            r#"
            SELECT id, run_id, qty, filled_qty, status, updated_at_ms
            FROM orders
            WHERE run_id = ? AND id = ?
            "#,
        )
        .bind(run_id)
        .bind(order_id)
        .fetch_optional(self.pool())
        .await?;

        Ok(row.map(
            |(id, run_id, order_qty, filled_qty, status, updated_at_ms)| RecoveredOrderState {
                id,
                run_id,
                order_qty,
                filled_qty,
                status,
                updated_at_ms,
            },
        ))
    }

    pub async fn list_fills(&self, run_id: &str) -> StorageResult<Vec<NewFill>> {
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

    pub async fn list_positions(&self, run_id: &str) -> StorageResult<Vec<NewPosition>> {
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
    ) -> StorageResult<Vec<NewAccountBalance>> {
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
    ) -> StorageResult<Vec<NewPortfolioSnapshot>> {
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

    pub async fn list_events(&self) -> StorageResult<Vec<EventRecord>> {
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

    pub async fn list_events_by_source(&self, source: &str) -> StorageResult<Vec<EventRecord>> {
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

    pub async fn replay_events_to_bus(&self, source: &str, bus: &EventBus) -> StorageResult<usize> {
        let events = self.list_events_by_source(source).await?;
        let envelopes = events
            .into_iter()
            .map(event_record_to_envelope)
            .collect::<Vec<_>>();
        let replayed = envelopes.len();
        bus.replay(envelopes)
            .map_err(|error| StorageError::Protocol(error.to_string()))?;
        Ok(replayed)
    }
}

fn event_record_to_envelope(record: EventRecord) -> AnyEventEnvelope {
    EventEnvelope {
        event_id: Uuid::parse_str(&record.event_id).unwrap_or_else(|_| Uuid::new_v4()),
        ts: Utc
            .timestamp_millis_opt(record.ts_ms)
            .single()
            .unwrap_or_else(Utc::now),
        source: record.source,
        category: EventCategory::System,
        payload: TraderEvent::Runtime(RuntimeEvent {
            category: record.category,
            payload_json: record.payload_json,
        }),
    }
}
