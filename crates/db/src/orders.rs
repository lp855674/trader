use crate::error::DbError;
use sqlx::SqlitePool;

pub struct NewOrder<'a> {
    pub order_id: &'a str,
    pub account_id: &'a str,
    pub instrument_id: i64,
    pub side: &'a str,
    pub qty: f64,
    pub status: &'a str,
    pub order_type: &'a str,
    pub limit_price: Option<f64>,
    pub exchange_ref: Option<&'a str>,
    pub idempotency_key: Option<&'a str>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
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
        "INSERT INTO orders (
             id,
             account_id,
             instrument_id,
             side,
             qty,
             status,
             order_type,
             limit_price,
             exchange_ref,
             idempotency_key,
             created_at_ms,
             updated_at_ms
         )
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(order.order_id)
    .bind(order.account_id)
    .bind(order.instrument_id)
    .bind(order.side)
    .bind(order.qty)
    .bind(order.status)
    .bind(order.order_type)
    .bind(order.limit_price)
    .bind(order.exchange_ref)
    .bind(order.idempotency_key)
    .bind(order.created_at_ms)
    .bind(order.updated_at_ms)
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

pub async fn amend_order(
    pool: &SqlitePool,
    order_id: &str,
    qty: f64,
    limit_price: Option<f64>,
    updated_at_ms: i64,
) -> Result<(), DbError> {
    sqlx::query(
        "UPDATE orders
         SET qty = ?, limit_price = ?, updated_at_ms = ?
         WHERE id = ?
           AND UPPER(status) IN ('PENDING', 'SUBMITTED', 'PARTIALLY_FILLED')",
    )
    .bind(qty)
    .bind(limit_price)
    .bind(updated_at_ms)
    .bind(order_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn cancel_order(
    pool: &SqlitePool,
    order_id: &str,
    updated_at_ms: i64,
) -> Result<(), DbError> {
    sqlx::query(
        "UPDATE orders
         SET status = 'CANCELLED', updated_at_ms = ?
         WHERE id = ?
           AND UPPER(status) IN ('PENDING', 'SUBMITTED', 'PARTIALLY_FILLED')",
    )
    .bind(updated_at_ms)
    .bind(order_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn update_order_status(
    pool: &SqlitePool,
    order_id: &str,
    status: &str,
    updated_at_ms: i64,
) -> Result<(), DbError> {
    sqlx::query(
        "UPDATE orders
         SET status = ?, updated_at_ms = ?
         WHERE id = ?",
    )
    .bind(status)
    .bind(updated_at_ms)
    .bind(order_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn filled_qty_for_order(pool: &SqlitePool, order_id: &str) -> Result<f64, DbError> {
    let qty = sqlx::query_scalar::<_, Option<f64>>(
        "SELECT SUM(qty) FROM fills WHERE order_id = ?",
    )
    .bind(order_id)
    .fetch_one(pool)
    .await?;
    Ok(qty.unwrap_or(0.0))
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
    pub order_type: String,
    pub limit_price: Option<f64>,
    pub exchange_ref: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
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
             orders.order_type AS order_type,
             orders.limit_price AS limit_price,
             orders.exchange_ref AS exchange_ref,
             orders.created_at_ms AS created_at_ms,
             orders.updated_at_ms AS updated_at_ms
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
    pub order_id: String,
    pub venue: String,
    pub symbol: String,
    pub side: String,
    pub qty: f64,
    pub status: String,
    pub order_type: String,
    pub limit_price: Option<f64>,
    pub exchange_ref: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

pub async fn list_orders_for_account(
    pool: &SqlitePool,
    account_id: &str,
) -> Result<Vec<OrderListRow>, DbError> {
    sqlx::query_as::<_, OrderListRow>(
        "SELECT
             orders.id AS order_id,
             instruments.venue AS venue,
             instruments.symbol AS symbol,
             orders.side AS side,
             orders.qty AS qty,
             orders.status AS status,
             orders.order_type AS order_type,
             orders.limit_price AS limit_price,
             orders.exchange_ref AS exchange_ref,
             orders.created_at_ms AS created_at_ms,
             orders.updated_at_ms AS updated_at_ms
         FROM orders
         INNER JOIN instruments ON instruments.id = orders.instrument_id
         WHERE orders.account_id = ?
         ORDER BY orders.created_at_ms DESC",
    )
    .bind(account_id)
    .fetch_all(pool)
    .await
    .map_err(Into::into)
}

#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct RawOrderListRow {
    pub id: String,
    pub account_id: String,
    pub instrument_id: i64,
    pub side: String,
    pub qty: f64,
    pub status: String,
    pub order_type: String,
    pub limit_price: Option<f64>,
    pub exchange_ref: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

pub async fn list_raw_orders_for_account(
    pool: &SqlitePool,
    account_id: &str,
) -> Result<Vec<RawOrderListRow>, DbError> {
    sqlx::query_as::<_, RawOrderListRow>(
        "SELECT
             id,
             account_id,
             instrument_id,
             side,
             qty,
             status,
             order_type,
             limit_price,
             exchange_ref,
             created_at_ms,
             updated_at_ms
         FROM orders WHERE account_id = ? ORDER BY created_at_ms DESC",
    )
    .bind(account_id)
    .fetch_all(pool)
    .await
    .map_err(Into::into)
}
