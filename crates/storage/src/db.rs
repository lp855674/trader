use std::str::FromStr;

use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};

pub type StorageResult<T> = Result<T, StorageError>;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error(transparent)]
    Sql(#[from] sqlx::Error),
    #[error("{0}")]
    Protocol(String),
}

#[derive(Clone)]
pub struct Db {
    pool: SqlitePool,
}

impl Db {
    pub async fn connect(database_url: &str) -> StorageResult<Self> {
        let options = SqliteConnectOptions::from_str(database_url)?.create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;
        Ok(Self { pool })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn migrate(&self) -> StorageResult<()> {
        sqlx::raw_sql(include_str!("../../../migrations/0001_init.sql"))
            .execute(&self.pool)
            .await?;
        self.ensure_strategy_runs_error_column().await?;
        Ok(())
    }

    async fn ensure_strategy_runs_error_column(&self) -> StorageResult<()> {
        let columns = sqlx::query_as::<_, (i64, String, String, i64, Option<String>, i64)>(
            "PRAGMA table_info(strategy_runs)",
        )
        .fetch_all(&self.pool)
        .await?;

        if columns.iter().any(|(_, name, _, _, _, _)| name == "error") {
            return Ok(());
        }

        let result = sqlx::query("ALTER TABLE strategy_runs ADD COLUMN error TEXT")
            .execute(&self.pool)
            .await;
        if let Err(error) = result
            && !is_duplicate_column_error(&error)
        {
            return Err(error.into());
        }
        Ok(())
    }
}

fn is_duplicate_column_error(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .map(|database_error| database_error.message().contains("duplicate column name"))
        .unwrap_or(false)
}
