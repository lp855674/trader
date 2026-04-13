use crate::error::DbError;
use sqlx::SqlitePool;

pub struct NewOrder<'a> {
    pub order_id: &'a str,
    pub account_id: &'a str,
    pub instrument_id: i64,
    pub side: &'a str,
    pub qty: f64,
    pub status: &'a str,
    pub idempotency_key: Option<&'a str>,
    pub created_at_ms: i64,
}

pub struct NewFill<'a> {
    pub fill_id: &'a str,
    pub order_id: &'a str,
    pub qty: f64,
    pub price: f64,
    pub created_at_ms: i64,
}

pub async fn insert_order(pool: &SqlitePool, order: &NewOrder<'_>) -> Result<(), DbError> {
    sqlx::query(
        "INSERT INTO orders (id, account_id, instrument_id, side, qty, status, idempotency_key, created_at_ms)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(order.order_id)
    .bind(order.account_id)
    .bind(order.instrument_id)
    .bind(order.side)
    .bind(order.qty)
    .bind(order.status)
    .bind(order.idempotency_key)
    .bind(order.created_at_ms)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn insert_fill(pool: &SqlitePool, fill: &NewFill<'_>) -> Result<(), DbError> {
    sqlx::query(
        "INSERT INTO fills (id, order_id, qty, price, created_at_ms) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(fill.fill_id)
    .bind(fill.order_id)
    .bind(fill.qty)
    .bind(fill.price)
    .bind(fill.created_at_ms)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn count_orders_for_account(pool: &SqlitePool, account_id: &str) -> Result<i64, DbError> {
    let n = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM orders WHERE account_id = ?")
        .bind(account_id)
        .fetch_one(pool)
        .await?;
    Ok(n)
}

pub async fn order_exists_by_idempotency_key(
    pool: &SqlitePool,
    account_id: &str,
    idempotency_key: &str,
) -> Result<bool, DbError> {
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM orders WHERE account_id = ? AND idempotency_key = ?",
    )
    .bind(account_id)
    .bind(idempotency_key)
    .fetch_one(pool)
    .await?;
    Ok(count > 0)
}

pub async fn latest_order_ts_for_instrument_side(
    pool: &SqlitePool,
    account_id: &str,
    instrument_id: i64,
    side: &str,
) -> Result<Option<i64>, DbError> {
    sqlx::query_scalar::<_, i64>(
        "SELECT created_at_ms
         FROM orders
         WHERE account_id = ? AND instrument_id = ? AND side = ?
         ORDER BY created_at_ms DESC
         LIMIT 1",
    )
    .bind(account_id)
    .bind(instrument_id)
    .bind(side)
    .fetch_optional(pool)
    .await
    .map_err(Into::into)
}

#[derive(Debug, Clone, PartialEq)]
pub struct LocalPositionSummary {
    pub net_qty: f64,
    pub last_fill_at_ms: i64,
}

pub async fn local_position_summary_for_instrument(
    pool: &SqlitePool,
    account_id: &str,
    instrument_id: i64,
) -> Result<Option<LocalPositionSummary>, DbError> {
    sqlx::query_as::<_, (f64, Option<i64>)>(
        "SELECT
             COALESCE(SUM(CASE WHEN orders.side = 'buy' THEN fills.qty ELSE -fills.qty END), 0.0) AS net_qty,
             MAX(fills.created_at_ms) AS last_fill_at_ms
         FROM fills
         INNER JOIN orders ON orders.id = fills.order_id
         WHERE orders.account_id = ? AND orders.instrument_id = ?",
    )
    .bind(account_id)
    .bind(instrument_id)
    .fetch_one(pool)
    .await
    .map(|(net_qty, last_fill_at_ms)| match last_fill_at_ms {
        Some(last_fill_at_ms) if net_qty.abs() > f64::EPSILON => Some(LocalPositionSummary {
            net_qty,
            last_fill_at_ms,
        }),
        _ => None,
    })
    .map_err(Into::into)
}

pub async fn has_open_order_for_instrument(
    pool: &SqlitePool,
    account_id: &str,
    instrument_id: i64,
) -> Result<bool, DbError> {
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)
         FROM orders
         WHERE account_id = ?
           AND instrument_id = ?
           AND UPPER(status) NOT IN ('FILLED', 'CANCELLED', 'REJECTED', 'EXPIRED')",
    )
    .bind(account_id)
    .bind(instrument_id)
    .fetch_one(pool)
    .await?;
    Ok(count > 0)
}

#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct OpenOrderViewRow {
    pub order_id: String,
    pub venue: String,
    pub symbol: String,
    pub side: String,
    pub qty: f64,
    pub status: String,
    pub created_at_ms: i64,
}

pub async fn list_open_orders_for_account(
    pool: &SqlitePool,
    account_id: &str,
) -> Result<Vec<OpenOrderViewRow>, DbError> {
    sqlx::query_as::<_, OpenOrderViewRow>(
        "SELECT
             orders.id AS order_id,
             instruments.venue AS venue,
             instruments.symbol AS symbol,
             orders.side AS side,
             orders.qty AS qty,
             orders.status AS status,
             orders.created_at_ms AS created_at_ms
         FROM orders
         INNER JOIN instruments ON instruments.id = orders.instrument_id
         WHERE orders.account_id = ?
           AND UPPER(orders.status) NOT IN ('FILLED', 'CANCELLED', 'REJECTED', 'EXPIRED')
         ORDER BY orders.created_at_ms DESC",
    )
    .bind(account_id)
    .fetch_all(pool)
    .await
    .map_err(Into::into)
}

#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct LocalPositionViewRow {
    pub venue: String,
    pub symbol: String,
    pub net_qty: f64,
    pub last_fill_at_ms: i64,
}

pub async fn list_local_positions_for_account(
    pool: &SqlitePool,
    account_id: &str,
) -> Result<Vec<LocalPositionViewRow>, DbError> {
    sqlx::query_as::<_, LocalPositionViewRow>(
        "SELECT
             instruments.venue AS venue,
             instruments.symbol AS symbol,
             SUM(CASE WHEN orders.side = 'buy' THEN fills.qty ELSE -fills.qty END) AS net_qty,
             MAX(fills.created_at_ms) AS last_fill_at_ms
         FROM fills
         INNER JOIN orders ON orders.id = fills.order_id
         INNER JOIN instruments ON instruments.id = orders.instrument_id
         WHERE orders.account_id = ?
         GROUP BY instruments.venue, instruments.symbol
         HAVING ABS(SUM(CASE WHEN orders.side = 'buy' THEN fills.qty ELSE -fills.qty END)) > 0.000000001
         ORDER BY instruments.venue, instruments.symbol",
    )
    .bind(account_id)
    .fetch_all(pool)
    .await
    .map_err(Into::into)
}

#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct OrderListRow {
    pub id: String,
    pub account_id: String,
    pub instrument_id: i64,
    pub side: String,
    pub qty: f64,
    pub status: String,
    pub created_at_ms: i64,
}

pub async fn list_orders_for_account(
    pool: &SqlitePool,
    account_id: &str,
) -> Result<Vec<OrderListRow>, DbError> {
    sqlx::query_as::<_, OrderListRow>(
        "SELECT id, account_id, instrument_id, side, qty, status, created_at_ms
         FROM orders WHERE account_id = ? ORDER BY created_at_ms DESC",
    )
    .bind(account_id)
    .fetch_all(pool)
    .await
    .map_err(Into::into)
}
