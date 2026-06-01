use crate::Db;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewInstrument {
    pub symbol: String,
    pub market: String,
    pub exchange: String,
    pub asset_class: String,
    pub currency: String,
    pub lot_size: String,
    pub tick_size: String,
    pub tradable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstrumentRecord {
    pub symbol: String,
    pub market: String,
    pub exchange: String,
    pub asset_class: String,
    pub currency: String,
    pub lot_size: String,
    pub tick_size: String,
    pub tradable: bool,
}

impl Db {
    pub async fn insert_instrument(&self, instrument: NewInstrument) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO instruments (
                symbol, market, exchange, asset_class, currency, lot_size, tick_size, tradable
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(instrument.symbol)
        .bind(instrument.market)
        .bind(instrument.exchange)
        .bind(instrument.asset_class)
        .bind(instrument.currency)
        .bind(instrument.lot_size)
        .bind(instrument.tick_size)
        .bind(instrument.tradable)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn get_instrument(
        &self,
        symbol: &str,
    ) -> Result<Option<InstrumentRecord>, sqlx::Error> {
        let row =
            sqlx::query_as::<_, (String, String, String, String, String, String, String, i64)>(
                r#"
            SELECT symbol, market, exchange, asset_class, currency, lot_size, tick_size, tradable
            FROM instruments
            WHERE symbol = ?
            "#,
            )
            .bind(symbol)
            .fetch_optional(self.pool())
            .await?;

        Ok(row.map(
            |(symbol, market, exchange, asset_class, currency, lot_size, tick_size, tradable)| {
                InstrumentRecord {
                    symbol,
                    market,
                    exchange,
                    asset_class,
                    currency,
                    lot_size,
                    tick_size,
                    tradable: tradable != 0,
                }
            },
        ))
    }
}
