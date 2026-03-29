use crate::error::DbError;
use sqlx::SqlitePool;

pub async fn insert_order(
    pool: &SqlitePool,
    order_id: &str,
    account_id: &str,
    instrument_id: i64,
    side: &str,
    qty: f64,
    status: &str,
    idempotency_key: Option<&str>,
    created_at_ms: i64,
) -> Result<(), DbError> {
    sqlx::query(
        "INSERT INTO orders (id, account_id, instrument_id, side, qty, status, idempotency_key, created_at_ms)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(order_id)
    .bind(account_id)
    .bind(instrument_id)
    .bind(side)
    .bind(qty)
    .bind(status)
    .bind(idempotency_key)
    .bind(created_at_ms)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn insert_fill(
    pool: &SqlitePool,
    fill_id: &str,
    order_id: &str,
    qty: f64,
    price: f64,
    created_at_ms: i64,
) -> Result<(), DbError> {
    sqlx::query(
        "INSERT INTO fills (id, order_id, qty, price, created_at_ms) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(fill_id)
    .bind(order_id)
    .bind(qty)
    .bind(price)
    .bind(created_at_ms)
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
