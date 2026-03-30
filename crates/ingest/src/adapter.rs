use async_trait::async_trait;
use db::Db;
use domain::Venue;

#[derive(Debug, thiserror::Error)]
pub enum IngestError {
    #[error(transparent)]
    Db(#[from] db::DbError),
    #[error("longbridge: {0}")]
    Longbridge(String),
}

#[async_trait]
pub trait IngestAdapter: Send + Sync {
    fn data_source_id(&self) -> &'static str;
    fn venue(&self) -> Venue;
    async fn ingest_once(&self, db: &Db, instrument_db_id: i64) -> Result<(), IngestError>;
}
