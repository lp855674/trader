use crate::error::DbError;
use sqlx::SqlitePool;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ExecutionProfileRow {
    pub id: String,
    pub kind: String,
    pub config_json: Option<String>,
}

pub async fn load_execution_profiles_by_kind(
    pool: &SqlitePool,
    kinds: &[&str],
) -> Result<Vec<ExecutionProfileRow>, DbError> {
    let mut results = Vec::new();
    for kind in kinds {
        let rows = sqlx::query_as::<_, ExecutionProfileRow>(
            "SELECT id, kind, config_json FROM execution_profiles WHERE kind = ?",
        )
        .bind(*kind)
        .fetch_all(pool)
        .await?;
        results.extend(rows);
    }
    Ok(results)
}

pub async fn load_execution_profiles(pool: &SqlitePool) -> Result<Vec<ExecutionProfileRow>, DbError> {
    sqlx::query_as::<_, ExecutionProfileRow>(
        "SELECT id, kind, config_json FROM execution_profiles ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .map_err(Into::into)
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AccountRow {
    pub id: String,
    pub mode: String,
    pub execution_profile_id: String,
}

pub async fn load_accounts(pool: &SqlitePool) -> Result<Vec<AccountRow>, DbError> {
    let rows =
        sqlx::query_as::<_, AccountRow>("SELECT id, mode, execution_profile_id FROM accounts")
            .fetch_all(pool)
            .await?;
    Ok(rows)
}
