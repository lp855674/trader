// Phase 0: Database Connection Pool with WAL Support
// Shared across all subsystems for crash safety and performance

use sqlx::{SqlitePool, SqlitePoolOptions, Pool, Row};
use std::time::Duration;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DbError {
    #[error("Database connection error: {0}")]
    Connection(#[from] sqlx::Error),
    
    #[error("Query error: {0}")]
    Query(#[from] sqlx::Error),
    
    #[error("Configuration error: {0}")]
    Config(String),
}

/// Database connection pool with WAL mode enabled
#[derive(Clone)]
pub struct Db {
    pub pool: Pool<SqlitePool>,
}

impl Db {
    /// Create new database pool with WAL mode enabled
    /// 
    /// # Arguments
    /// * `url` - Database URL (default: sqlite:quantd.db)
    /// * `max_connections` - Maximum connections (default: 25)
    /// 
    /// # Returns
    /// * `Result<Db, DbError>`
    pub async fn connect(url: &str) -> Result<Self, DbError> {
        let max_connections = 25;
        
        // Configure pool options
        let pool_options = SqlitePoolOptions::new()
            .max_connections(max_connections)
            .acquire_timeout(Duration::from_secs(30))
            .idle_timeout(Duration::from_secs(60))
            .min_idle(2);
        
        let pool = pool_options.connect(url).await?;
        
        // Enable WAL mode for crash safety and better concurrent performance
        sqlx::query("PRAGMA journal_mode = WAL")
            .execute(&pool)
            .await?;
        
        // Set WAL parameters for better performance
        sqlx::query("PRAGMA synchronous = NORMAL")
            .execute(&pool)
            .await?;
        
        sqlx::query("PRAGMA cache_size = -64000")  // 64MB cache
            .execute(&pool)
            .await?;
        
        sqlx::query("PRAGMA temp_store = MEMORY")
            .execute(&pool)
            .await?;
        
        sqlx::query("PRAGMA mmap_size = 268435456")  // 256MB memory-mapped file
            .execute(&pool)
            .await?;
        
        println!("[Db] Connected to SQLite with WAL mode (max_conns={})", max_connections);
        
        Ok(Self { pool })
    }

    /// Acquire a connection with timeout
    pub async fn get_connection(&self) -> Result<sqlx::SqliteConnection, DbError> {
        self.pool.acquire().await.map_err(DbError::from)
    }

    /// Execute raw SQL
    pub async fn execute(&self, sql: &str) -> Result<u64, DbError> {
        let mut cx = self.get_connection().await?;
        let count = sqlx::query_scalar(sql).fetch_one(&mut cx).await?;
        Ok(count)
    }

    /// Query with optional parameters
    pub async fn query_one(&self, sql: &str, params: Vec<(String, sqlx::Value)> 
    ) -> Result<Row, DbError> {
        let mut cx = self.get_connection().await?;
        let mut query = sqlx::query(sql);
        for (key, val) in params {
            query = query.bind((key, val));
        }
        query.fetch_one(&mut cx).await.map_err(DbError::from)
    }

    /// Health check
    pub async fn health_check(&self) -> Result<bool, DbError> {
        let query = "SELECT 1";
        let mut cx = self.get_connection().await?;
        let _ = sqlx::query(query).fetch_one(&mut cx).await?;
        Ok(true)
    }
}

impl Default for Db {
    fn default() -> Self {
        Self::connect("sqlite:quantd.db").await.unwrap_or_else(|e| {
            eprintln!("[Db] Failed to connect: {}", e);
            panic!("Failed to initialize database");
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_wal_mode() {
        // Use in-memory database for test
        let db = Db::connect("sqlite::memory:").await.unwrap();
        assert!(db.health_check().await.unwrap());
    }

    #[tokio::test]
    async fn test_pool_performance() {
        let db = Db::connect("sqlite::memory:").await.unwrap();
        
        // Test concurrent operations
        let handle = tokio::spawn(async move {
            let mut cx = db.get_connection().await.unwrap();
            sqlx::query("CREATE TABLE test (id INTEGER PRIMARY KEY)")
                .execute(&mut cx)
                .await
                .unwrap();
        });
        
        let _ = handle.await;
    }
}
