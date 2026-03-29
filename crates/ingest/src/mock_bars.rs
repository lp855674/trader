use async_trait::async_trait;
use db::Db;
use domain::Venue;

use crate::adapter::{IngestAdapter, IngestError};

pub struct MockBarsAdapter {
    venue: Venue,
    data_source_id: &'static str,
}

impl MockBarsAdapter {
    pub const fn new(venue: Venue, data_source_id: &'static str) -> Self {
        Self {
            venue,
            data_source_id,
        }
    }
}

#[async_trait]
impl IngestAdapter for MockBarsAdapter {
    fn data_source_id(&self) -> &'static str {
        self.data_source_id
    }

    fn venue(&self) -> Venue {
        self.venue
    }

    async fn ingest_once(&self, db: &Db, instrument_db_id: i64) -> Result<(), IngestError> {
        db::insert_bar(
            db.pool(),
            instrument_db_id,
            self.data_source_id,
            1,
            100.0,
            100.0,
            100.0,
            100.0,
            0.0,
        )
        .await?;
        Ok(())
    }
}
