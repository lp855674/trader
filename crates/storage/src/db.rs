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
        sqlx::raw_sql(include_str!(
            "../../../migrations/0002_audit_projections.sql"
        ))
        .execute(&self.pool)
        .await?;
        sqlx::raw_sql(include_str!("../../../migrations/0003_market_rules.sql"))
            .execute(&self.pool)
            .await?;
        sqlx::raw_sql(include_str!(
            "../../../migrations/0004_contract_accounting.sql"
        ))
        .execute(&self.pool)
        .await?;
        sqlx::raw_sql(include_str!(
            "../../../migrations/0005_reference_snapshots_and_ops.sql"
        ))
        .execute(&self.pool)
        .await?;
        sqlx::raw_sql(include_str!(
            "../../../migrations/0006_config_lifecycle.sql"
        ))
        .execute(&self.pool)
        .await?;
        let fee_rule_minimum_fee_migration = sqlx::raw_sql(include_str!(
            "../../../migrations/0007_fee_rule_minimum_fee.sql"
        ))
        .execute(&self.pool)
        .await;
        if let Err(error) = fee_rule_minimum_fee_migration
            && !is_duplicate_column_error(&error)
        {
            return Err(error.into());
        }
        let fee_rule_tax_exchange_fee_migration = sqlx::raw_sql(include_str!(
            "../../../migrations/0008_fee_rule_tax_exchange_fees.sql"
        ))
        .execute(&self.pool)
        .await;
        if let Err(error) = fee_rule_tax_exchange_fee_migration
            && !is_duplicate_column_error(&error)
        {
            return Err(error.into());
        }
        sqlx::raw_sql(include_str!("../../../migrations/0009_fee_rule_tiers.sql"))
            .execute(&self.pool)
            .await?;
        let fee_rule_symbol_migration =
            sqlx::raw_sql(include_str!("../../../migrations/0010_fee_rule_symbol.sql"))
                .execute(&self.pool)
                .await;
        if let Err(error) = fee_rule_symbol_migration
            && !is_duplicate_column_error(&error)
        {
            return Err(error.into());
        }
        let fee_rule_volume_window_migration = sqlx::raw_sql(include_str!(
            "../../../migrations/0011_fee_rule_volume_window.sql"
        ))
        .execute(&self.pool)
        .await;
        if let Err(error) = fee_rule_volume_window_migration
            && !is_duplicate_column_error(&error)
        {
            return Err(error.into());
        }
        let production_reconciliation_migration = sqlx::raw_sql(include_str!(
            "../../../migrations/0012_production_reconciliation_contract_metadata.sql"
        ))
        .execute(&self.pool)
        .await;
        if let Err(error) = production_reconciliation_migration
            && !is_duplicate_column_error(&error)
        {
            return Err(error.into());
        }
        self.ensure_config_lifecycle_columns().await?;
        self.ensure_fee_rule_columns().await?;
        self.ensure_strategy_runs_error_column().await?;
        self.ensure_production_reconciliation_columns().await?;
        self.ensure_config_approvals_table().await?;
        Ok(())
    }

    async fn ensure_config_lifecycle_columns(&self) -> StorageResult<()> {
        let columns = sqlx::query_as::<_, (i64, String, String, i64, Option<String>, i64)>(
            "PRAGMA table_info(configs)",
        )
        .fetch_all(&self.pool)
        .await?;

        let has_column = |column_name: &str| {
            columns
                .iter()
                .any(|(_, name, _, _, _, _)| name == column_name)
        };
        let required_columns = [
            ("lifecycle_version", "INTEGER"),
            ("state", "TEXT"),
            ("parent_version", "INTEGER"),
            ("created_by", "TEXT"),
            ("state_changed_at", "INTEGER"),
            ("state_changed_by", "TEXT"),
            ("state_change_reason", "TEXT"),
            ("target_env", "TEXT"),
            ("rollout", "TEXT"),
            ("approved_by", "TEXT"),
            ("approved_at", "INTEGER"),
            ("published_by", "TEXT"),
            ("published_at", "INTEGER"),
        ];

        for (column_name, column_type) in required_columns {
            if !has_column(column_name) {
                sqlx::query(&format!(
                    "ALTER TABLE configs ADD COLUMN {column_name} {column_type}"
                ))
                .execute(&self.pool)
                .await?;
            }
        }

        sqlx::query(
            r#"
            CREATE UNIQUE INDEX IF NOT EXISTS idx_configs_name_lifecycle_version
            ON configs(name, lifecycle_version)
            WHERE lifecycle_version IS NOT NULL
            "#,
        )
        .execute(&self.pool)
        .await?;
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

    async fn ensure_config_approvals_table(&self) -> StorageResult<()> {
        sqlx::raw_sql(include_str!(
            "../../../migrations/0013_config_approvals.sql"
        ))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn ensure_fee_rule_columns(&self) -> StorageResult<()> {
        let columns = sqlx::query_as::<_, (i64, String, String, i64, Option<String>, i64)>(
            "PRAGMA table_info(fee_rules)",
        )
        .fetch_all(&self.pool)
        .await?;

        let has_column = |column_name: &str| {
            columns
                .iter()
                .any(|(_, name, _, _, _, _)| name == column_name)
        };
        let required_columns = [
            ("minimum_fee", "TEXT"),
            ("tax_bps", "TEXT"),
            ("exchange_fee_bps", "TEXT"),
            ("symbol", "TEXT"),
            ("volume_window", "TEXT NOT NULL DEFAULT 'run'"),
        ];

        for (column_name, column_type) in required_columns {
            if has_column(column_name) {
                continue;
            }
            let result = sqlx::query(&format!(
                "ALTER TABLE fee_rules ADD COLUMN {column_name} {column_type}"
            ))
            .execute(&self.pool)
            .await;
            if let Err(error) = result
                && !is_duplicate_column_error(&error)
            {
                return Err(error.into());
            }
        }
        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_fee_rules_lookup
            ON fee_rules(market, exchange, asset_class, symbol, effective_from_ms)
            "#,
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_orders_account_symbol
            ON orders(account_id, symbol)
            "#,
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_fills_order_ts
            ON fills(order_id, ts_ms)
            "#,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn ensure_production_reconciliation_columns(&self) -> StorageResult<()> {
        let columns = sqlx::query_as::<_, (i64, String, String, i64, Option<String>, i64)>(
            "PRAGMA table_info(position_snapshots)",
        )
        .fetch_all(&self.pool)
        .await?;

        let has_column = |column_name: &str| {
            columns
                .iter()
                .any(|(_, name, _, _, _, _)| name == column_name)
        };
        for (column_name, column_type) in [
            ("contract_metadata_json", "TEXT"),
            ("liquidation_price", "TEXT"),
            ("open_interest", "TEXT"),
        ] {
            if has_column(column_name) {
                continue;
            }
            let result = sqlx::query(&format!(
                "ALTER TABLE position_snapshots ADD COLUMN {column_name} {column_type}"
            ))
            .execute(&self.pool)
            .await;
            if let Err(error) = result
                && !is_duplicate_column_error(&error)
            {
                return Err(error.into());
            }
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
