//! Seed rows for paper trading (`paper` profile, accounts, synthetic bar source).

use crate::error::DbError;
use sqlx::SqlitePool;

/// `bars.data_source_id` FK for [`ingest::MockBarsAdapter`] synthetic bars.
pub const PAPER_BARS_DATA_SOURCE_ID: &str = "paper_bars";

pub async fn ensure_mvp_seed(pool: &SqlitePool) -> Result<(), DbError> {
    sqlx::query("INSERT OR IGNORE INTO data_sources (id, kind, config_json) VALUES (?, ?, NULL)")
        .bind(PAPER_BARS_DATA_SOURCE_ID)
        .bind("synthetic_bars")
        .execute(pool)
        .await?;

    sqlx::query(
        "INSERT OR IGNORE INTO execution_profiles (id, kind, config_json) VALUES ('paper', 'paper_sim', NULL)",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "INSERT OR IGNORE INTO accounts (id, mode, execution_profile_id, venue) VALUES ('acc_mvp_paper', 'paper', 'paper', NULL)",
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Longbridge 实盘账户与数据源元数据（与 `longbridge_adapters` 配合使用）。
pub async fn ensure_longbridge_live_account(pool: &SqlitePool) -> Result<(), DbError> {
    sqlx::query(
        "INSERT OR IGNORE INTO data_sources (id, kind, config_json) VALUES ('longbridge', 'longbridge_quote', NULL)",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "INSERT OR IGNORE INTO execution_profiles (id, kind, config_json) VALUES ('longbridge', 'longbridge_live', NULL)",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "INSERT OR IGNORE INTO accounts (id, mode, execution_profile_id, venue) VALUES ('acc_lb_live', 'live', 'longbridge', NULL)",
    )
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn ensure_account(
    pool: &SqlitePool,
    id: &str,
    mode: &str,
    execution_profile_id: &str,
) -> Result<(), DbError> {
    sqlx::query(
        "INSERT OR IGNORE INTO accounts (id, mode, execution_profile_id, venue) VALUES (?, ?, ?, NULL)",
    )
    .bind(id)
    .bind(mode)
    .bind(execution_profile_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// 写入长桥 paper 账号凭证（凭证存 execution_profiles.config_json）。
pub async fn ensure_longbridge_paper_account(
    pool: &SqlitePool,
    app_key: &str,
    app_secret: &str,
    access_token: &str,
) -> Result<(), DbError> {
    let config_json = format!(
        r#"{{"app_key":"{}","app_secret":"{}","access_token":"{}"}}"#,
        app_key, app_secret, access_token
    );

    sqlx::query(
        "INSERT INTO execution_profiles (id, kind, config_json) VALUES ('longbridge_paper', 'longbridge_paper', ?)
         ON CONFLICT(id) DO UPDATE SET config_json = excluded.config_json",
    )
    .bind(&config_json)
    .execute(pool)
    .await?;

    sqlx::query(
        "INSERT OR IGNORE INTO accounts (id, mode, execution_profile_id, venue)
         VALUES ('acc_lb_paper', 'paper', 'longbridge_paper', NULL)",
    )
    .execute(pool)
    .await?;

    Ok(())
}
