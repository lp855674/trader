use std::sync::Arc;

use async_trait::async_trait;
use db::Db;
use domain::Venue;
use ingest::{IngestAdapter, IngestError};
use longbridge::quote::{AdjustType, Period, QuoteContext, TradeSessions};
use rust_decimal::prelude::ToPrimitive;

/// 使用 Longbridge 拉取最近 1 根分钟 K 并写入 `bars`（`data_source_id = longbridge`）。  
/// `lb_symbol` 需为 Longbridge 标的代码，如 `AAPL.US`、`700.HK`。
pub struct LongbridgeCandleIngest {
    quote: Arc<QuoteContext>,
    venue: Venue,
    lb_symbol: String,
}

impl LongbridgeCandleIngest {
    pub fn new(quote: Arc<QuoteContext>, venue: Venue, lb_symbol: impl Into<String>) -> Self {
        Self {
            quote,
            venue,
            lb_symbol: lb_symbol.into(),
        }
    }
}

fn decimal_to_f64(d: rust_decimal::Decimal) -> Result<f64, IngestError> {
    d.to_f64()
        .ok_or_else(|| IngestError::Longbridge("decimal to f64".to_string()))
}

#[async_trait]
impl IngestAdapter for LongbridgeCandleIngest {
    fn data_source_id(&self) -> &'static str {
        "longbridge"
    }

    fn venue(&self) -> Venue {
        self.venue
    }

    async fn ingest_once(&self, db: &Db, instrument_db_id: i64) -> Result<(), IngestError> {
        let rows = self
            .quote
            .candlesticks(
                self.lb_symbol.as_str(),
                Period::OneMinute,
                1,
                AdjustType::NoAdjust,
                TradeSessions::Intraday,
            )
            .await
            .map_err(|e| IngestError::Longbridge(e.to_string()))?;

        let c = rows
            .into_iter()
            .next_back()
            .ok_or_else(|| IngestError::Longbridge("empty candlesticks".to_string()))?;

        let ts_ms = c.timestamp.unix_timestamp() * 1000;
        let open = decimal_to_f64(c.open)?;
        let high = decimal_to_f64(c.high)?;
        let low = decimal_to_f64(c.low)?;
        let close = decimal_to_f64(c.close)?;
        let volume = c.volume as f64;

        let bar = db::NewBar {
            instrument_id: instrument_db_id,
            data_source_id: self.data_source_id(),
            ts_ms,
            open,
            high,
            low,
            close,
            volume,
        };
        db::insert_bar(db.pool(), &bar).await?;
        Ok(())
    }
}
