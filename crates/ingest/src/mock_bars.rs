use async_trait::async_trait;
use db::Db;
use domain::Venue;

use crate::adapter::{IngestAdapter, IngestError};

/// Matches [`db::bootstrap::PAPER_BARS_DATA_SOURCE_ID`] for FK on `bars`.
pub const PAPER_BARS_DATA_SOURCE_ID: &str = db::PAPER_BARS_DATA_SOURCE_ID;

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

    pub const fn paper_bars(venue: Venue) -> Self {
        Self::new(venue, PAPER_BARS_DATA_SOURCE_ID)
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
        let next_ts_ms =
            match db::get_recent_bars(db.pool(), instrument_db_id, self.data_source_id, 1)
                .await?
                .last()
            {
                Some(last_bar) => last_bar.ts_ms + 60_000,
                None => std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|duration| duration.as_millis() as i64)
                    .unwrap_or(1),
            };
        let bar = db::NewBar {
            instrument_id: instrument_db_id,
            data_source_id: self.data_source_id,
            ts_ms: next_ts_ms,
            open: 100.0,
            high: 100.0,
            low: 100.0,
            close: 100.0,
            volume: 0.0,
        };
        db::insert_bar(db.pool(), &bar).await?;
        Ok(())
    }
}
