use serde::Deserialize;
use std::str::FromStr;

use crate::ingestion::{CryptoMarketMeta, IngestionError, IngestionResult, http_retry};

const BINANCE_SPOT_EXCHANGE_INFO_URL: &str = "https://api.binance.com/api/v3/exchangeInfo";

#[derive(Debug, Deserialize)]
struct BinanceExchangeInfo {
    symbols: Vec<BinanceSymbolInfo>,
}

#[derive(Debug, Deserialize)]
struct BinanceSymbolInfo {
    symbol: String,
    status: String,
    #[serde(rename = "baseAsset")]
    base_asset: String,
    #[serde(rename = "quoteAsset")]
    quote_asset: String,
    #[serde(rename = "baseAssetPrecision")]
    base_asset_precision: Option<i64>,
    #[serde(rename = "quoteAssetPrecision")]
    quote_asset_precision: Option<i64>,
    #[serde(rename = "contractType")]
    contract_type: Option<String>,
    #[serde(rename = "contractSize")]
    contract_size: Option<String>,
    #[serde(rename = "marginAsset")]
    margin_asset: Option<String>,
    #[serde(rename = "pricePrecision")]
    price_precision: Option<i64>,
    #[serde(rename = "quantityPrecision")]
    quantity_precision: Option<i64>,
    filters: Vec<BinanceFilter>,
}

#[derive(Debug, Deserialize)]
struct BinanceFilter {
    #[serde(rename = "filterType")]
    filter_type: String,
    #[serde(rename = "tickSize")]
    tick_size: Option<String>,
    #[serde(rename = "minQty")]
    min_qty: Option<String>,
    #[serde(rename = "maxQty")]
    max_qty: Option<String>,
    #[serde(rename = "stepSize")]
    step_size: Option<String>,
    #[serde(rename = "minNotional")]
    min_notional: Option<String>,
    #[serde(rename = "notional")]
    notional: Option<String>,
}

pub async fn fetch_binance_market_meta(
    client: &reqwest::Client,
) -> Result<Vec<CryptoMarketMeta>, IngestionError> {
    let payload =
        http_retry::get_text_with_retry(client, BINANCE_SPOT_EXCHANGE_INFO_URL, &[]).await?;
    parse_binance_market_meta(&payload, chrono::Utc::now().timestamp_millis())
}

pub async fn ingest_binance_market_meta(
    db: &storage::Db,
    client: &reqwest::Client,
) -> Result<IngestionResult, IngestionError> {
    let records = fetch_binance_market_meta(client).await?;
    let rows_fetched = records.len();

    for record in records {
        db.record_crypto_market_meta(storage::CryptoMarketMetaCommand {
            exchange: record.exchange,
            symbol: record.symbol,
            base_asset: record.base_asset,
            quote_asset: record.quote_asset,
            instrument_type: record.instrument_type,
            contract_type: record.contract_type,
            contract_size: record
                .contract_size
                .as_deref()
                .map(parse_decimal)
                .transpose()?,
            settlement_asset: record.settlement_asset,
            min_notional: record
                .min_notional
                .as_deref()
                .map(parse_decimal)
                .transpose()?,
            min_qty: record.min_qty.as_deref().map(parse_decimal).transpose()?,
            max_qty: record.max_qty.as_deref().map(parse_decimal).transpose()?,
            price_precision: record.price_precision,
            qty_precision: record.qty_precision,
            price_tick: record
                .price_tick
                .as_deref()
                .map(parse_decimal)
                .transpose()?,
            qty_step: record.qty_step.as_deref().map(parse_decimal).transpose()?,
            maker_fee_rate: record
                .maker_fee_rate
                .as_deref()
                .map(parse_decimal)
                .transpose()?,
            taker_fee_rate: record
                .taker_fee_rate
                .as_deref()
                .map(parse_decimal)
                .transpose()?,
            funding_interval_hours: record.funding_interval_hours,
            max_leverage: record
                .max_leverage
                .as_deref()
                .map(parse_decimal)
                .transpose()?,
            margin_modes: record.margin_modes,
            is_inverse: record.is_inverse,
            is_active: record.is_active,
            created_at_ms: record.created_at_ms,
            updated_at_ms: record.updated_at_ms,
        })
        .await?;
    }

    Ok(IngestionResult {
        source: "binance".to_string(),
        table: "crypto_market_meta".to_string(),
        rows_fetched,
        rows_upserted: rows_fetched,
    })
}

pub fn parse_binance_market_meta(
    payload: &str,
    fetched_at_ms: i64,
) -> Result<Vec<CryptoMarketMeta>, IngestionError> {
    let exchange_info = serde_json::from_str::<BinanceExchangeInfo>(payload)?;
    Ok(exchange_info
        .symbols
        .into_iter()
        .map(|symbol| symbol.into_market_meta(fetched_at_ms))
        .collect())
}

fn parse_decimal(value: &str) -> Result<rust_decimal::Decimal, IngestionError> {
    rust_decimal::Decimal::from_str(value).map_err(|source| IngestionError::Decimal {
        value: value.to_string(),
        source,
    })
}

impl BinanceSymbolInfo {
    fn into_market_meta(self, fetched_at_ms: i64) -> CryptoMarketMeta {
        let price_tick = self
            .filter("PRICE_FILTER")
            .and_then(|filter| filter.tick_size.clone());
        let min_qty = self
            .filter("LOT_SIZE")
            .and_then(|filter| filter.min_qty.clone());
        let max_qty = self
            .filter("LOT_SIZE")
            .and_then(|filter| filter.max_qty.clone());
        let qty_step = self
            .filter("LOT_SIZE")
            .and_then(|filter| filter.step_size.clone());
        let min_notional = self
            .filter("MIN_NOTIONAL")
            .and_then(|filter| filter.min_notional.clone())
            .or_else(|| {
                self.filter("NOTIONAL").and_then(|filter| {
                    filter
                        .min_notional
                        .clone()
                        .or_else(|| filter.notional.clone())
                })
            });
        let is_contract = self.contract_type.is_some();

        CryptoMarketMeta {
            exchange: "BINANCE".to_string(),
            symbol: self.symbol,
            base_asset: self.base_asset,
            quote_asset: self.quote_asset,
            instrument_type: if self.contract_type.is_some() {
                "PERP".to_string()
            } else {
                "SPOT".to_string()
            },
            contract_type: self.contract_type,
            contract_size: self.contract_size,
            settlement_asset: self.margin_asset,
            min_notional,
            min_qty,
            max_qty,
            price_precision: self.price_precision.or(self.quote_asset_precision),
            qty_precision: self.quantity_precision.or(self.base_asset_precision),
            price_tick,
            qty_step,
            maker_fee_rate: None,
            taker_fee_rate: None,
            funding_interval_hours: is_contract.then_some(8),
            max_leverage: None,
            margin_modes: is_contract.then(|| vec!["CROSS".to_string(), "ISOLATED".to_string()]),
            is_inverse: false,
            is_active: self.status == "TRADING",
            created_at_ms: fetched_at_ms,
            updated_at_ms: fetched_at_ms,
        }
    }

    fn filter(&self, filter_type: &str) -> Option<&BinanceFilter> {
        self.filters
            .iter()
            .find(|filter| filter.filter_type == filter_type)
    }
}
