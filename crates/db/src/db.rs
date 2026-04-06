//! SQLite persistence. Only this crate uses `sqlx` directly.

mod bars;
mod bootstrap;
mod error;
mod instruments;
mod orders;
mod signals;

pub use bars::{count_bars_for_source, insert_bar, last_bar_close, NewBar};
pub use bootstrap::{
    ensure_account, ensure_longbridge_live_account, ensure_mvp_seed, PAPER_BARS_DATA_SOURCE_ID,
};
pub use error::DbError;
pub use instruments::{list_instruments, upsert_instrument, InstrumentRow};
pub use orders::{
    count_orders_for_account, insert_fill, insert_order, list_orders_for_account, NewFill,
    NewOrder, OrderListRow,
};
pub use signals::{insert_risk_decision, insert_signal};

use std::path::{Path, PathBuf};

use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};

/// Ensure parent directories exist for file-backed SQLite URLs (e.g. `sqlite://./data/quantd.db`).
fn ensure_sqlite_parent_dir(url: &str) -> Result<(), DbError> {
    let Some(path) = sqlite_file_path(url) else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(DbError::from)?;
        }
    }
    Ok(())
}

fn sqlite_file_path(url: &str) -> Option<PathBuf> {
    let mut u = url.trim();
    if let Some(rest) = u.strip_prefix("sqlite://") {
        u = rest;
    } else if let Some(rest) = u.strip_prefix("sqlite:") {
        u = rest;
    } else {
        return None;
    }
    let path_part = u.split('?').next().filter(|p| !p.is_empty())?;
    if path_part.contains("memory") {
        return None;
    }
    Some(Path::new(path_part).to_path_buf())
}

/// sqlx SQLite defaults omit `create_if_missing`; append `mode=rwc` when no `mode=` is set so a new file can be created.
fn normalize_sqlite_url(url: &str) -> String {
    let t = url.trim();
    if !t.starts_with("sqlite:") {
        return url.to_string();
    }
    if t.contains("memory") {
        return url.to_string();
    }
    if t.contains("mode=") {
        return url.to_string();
    }
    if t.contains('?') {
        format!("{url}&mode=rwc")
    } else {
        format!("{url}?mode=rwc")
    }
}

#[derive(Clone)]
pub struct Db {
    pool: SqlitePool,
}

impl Db {
    pub async fn connect(database_url: &str) -> Result<Self, DbError> {
        ensure_sqlite_parent_dir(database_url)?;
        let database_url = normalize_sqlite_url(database_url);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}
