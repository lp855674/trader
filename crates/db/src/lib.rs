//! SQLite persistence. Only this crate uses `sqlx` directly.

mod bars;
mod bootstrap;
mod error;
mod instruments;
mod orders;
mod signals;

pub use bars::{insert_bar, last_bar_close, NewBar};
pub use bootstrap::{ensure_account, ensure_mvp_seed};
pub use error::DbError;
pub use instruments::{list_instruments, upsert_instrument, InstrumentRow};
pub use orders::{
    count_orders_for_account, insert_fill, insert_order, list_orders_for_account, NewFill,
    NewOrder, OrderListRow,
};
pub use signals::{insert_risk_decision, insert_signal};

use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};

#[derive(Clone)]
pub struct Db {
    pool: SqlitePool,
}

impl Db {
    pub async fn connect(database_url: &str) -> Result<Self, DbError> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

#[cfg(test)]
mod tests {
    use super::Db;

    #[tokio::test]
    async fn migrate_runs_clean() {
        let database = Db::connect("sqlite::memory:").await;
        assert!(database.is_ok(), "{:?}", database.err());
    }
}
