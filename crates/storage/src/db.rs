use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};

#[derive(Clone)]
pub struct Db {
    pool: SqlitePool,
}

impl Db {
    pub async fn connect(database_url: &str) -> Result<Self, sqlx::Error> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await?;
        Ok(Self { pool })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn migrate(&self) -> Result<(), sqlx::Error> {
        sqlx::raw_sql(include_str!("../../../migrations/0001_init.sql"))
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
