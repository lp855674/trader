//! SQLite persistence. Only this crate uses `sqlx` directly.

mod bars;
mod bootstrap;
mod error;
mod execution_profiles;
mod instruments;
mod orders;
mod reconciliation;
mod runtime_controls;
mod runtime_cycle_history;
mod signals;
mod system_config;

pub use bars::{
    BarRow, NewBar, count_bars_for_source, get_recent_bars, insert_bar, last_bar_close,
};
pub use bootstrap::{
    PAPER_BARS_DATA_SOURCE_ID, ensure_account, ensure_longbridge_live_account,
    ensure_longbridge_paper_account, ensure_mvp_seed,
};
pub use error::DbError;
pub use execution_profiles::{
    AccountRow, ExecutionProfileRow, load_accounts, load_execution_profiles_by_kind,
};
pub use instruments::{InstrumentRow, list_instruments, upsert_instrument};
pub use orders::{
    LocalPositionSummary, LocalPositionViewRow, NewFill, NewOrder, OpenOrderViewRow, OrderListRow,
    RawOrderListRow,
    amend_order, cancel_order, count_orders_for_account, has_open_order_for_instrument,
    insert_fill, insert_order, latest_order_ts_for_instrument_side,
    list_local_positions_for_account, list_open_orders_for_account, list_orders_for_account,
    list_raw_orders_for_account,
    local_position_summary_for_instrument, order_exists_by_idempotency_key,
};
pub use reconciliation::{
    ReconciliationSnapshot, ReconciliationSnapshotRow, insert_reconciliation_snapshot,
    load_latest_reconciliation_snapshot,
};
pub use runtime_controls::{
    get_runtime_control, list_symbol_allowlist, replace_symbol_allowlist, set_runtime_control,
};
pub use runtime_cycle_history::{
    NewRuntimeCycleRun, NewRuntimeCycleSymbol, RuntimeCycleRunRow, RuntimeCycleSymbolRow,
    insert_runtime_cycle_run, insert_runtime_cycle_symbols, list_runtime_cycle_runs,
    list_runtime_cycle_symbols_for_run,
};
pub use signals::{insert_risk_decision, insert_signal};
pub use system_config::{get_system_config, list_system_config_by_prefix, set_system_config};

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

pub fn sqlite_max_connections(url: &str) -> u32 {
    if url.trim().starts_with("sqlite:") && url.contains("memory") {
        1
    } else {
        5
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
            .max_connections(sqlite_max_connections(&database_url))
            .connect(&database_url)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}
