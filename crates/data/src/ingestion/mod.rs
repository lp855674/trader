pub mod binance_meta;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CryptoMarketMeta {
    pub exchange: String,
    pub symbol: String,
    pub base_asset: String,
    pub quote_asset: String,
    pub instrument_type: String,
    pub contract_type: Option<String>,
    pub contract_size: Option<String>,
    pub settlement_asset: Option<String>,
    pub min_notional: Option<String>,
    pub min_qty: Option<String>,
    pub max_qty: Option<String>,
    pub price_precision: Option<i64>,
    pub qty_precision: Option<i64>,
    pub price_tick: Option<String>,
    pub qty_step: Option<String>,
    pub maker_fee_rate: Option<String>,
    pub taker_fee_rate: Option<String>,
    pub funding_interval_hours: Option<i64>,
    pub max_leverage: Option<String>,
    pub margin_modes: Option<Vec<String>>,
    pub is_inverse: bool,
    pub is_active: bool,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum IngestionError {
    #[error("failed to fetch reference data: {0}")]
    Http(#[from] reqwest::Error),
    #[error("failed to parse reference data: {0}")]
    Json(#[from] serde_json::Error),
}
