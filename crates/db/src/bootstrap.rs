//! Seed rows required for MVP integration tests and local runs.

use crate::error::DbError;
use sqlx::SqlitePool;

pub async fn ensure_mvp_seed(pool: &SqlitePool) -> Result<(), DbError> {
    for (id, kind) in [
        ("mock_us", "mock_bars"),
        ("mock_hk", "mock_bars"),
        ("mock_crypto", "mock_bars"),
        ("mock_poly", "mock_bars"),
    ] {
        sqlx::query(
            "INSERT OR IGNORE INTO data_sources (id, kind, config_json) VALUES (?, ?, NULL)",
        )
        .bind(id)
        .bind(kind)
        .execute(pool)
        .await?;
    }

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
