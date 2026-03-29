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
