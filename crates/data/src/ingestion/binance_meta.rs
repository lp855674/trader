use serde::Deserialize;

use crate::ingestion::{CryptoMarketMeta, IngestionError};

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
    let payload = client
        .get(BINANCE_SPOT_EXCHANGE_INFO_URL)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    parse_binance_market_meta(&payload, chrono::Utc::now().timestamp_millis())
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
