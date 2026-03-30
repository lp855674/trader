use crate::ids::InstrumentId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccountMode {
    Paper,
    Live,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Signal {
    pub strategy_id: String,
    pub instrument: InstrumentId,
    pub instrument_db_id: i64,
    pub side: Side,
    pub qty: f64,
    /// 限价单价格（与 `OrderIntent.limit_price` 一致）。
    pub limit_price: f64,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrderIntent {
    pub strategy_id: String,
    pub instrument: InstrumentId,
    pub instrument_db_id: i64,
    pub side: Side,
    pub qty: f64,
    pub limit_price: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NormalizedBar {
    pub ts_ms: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}
