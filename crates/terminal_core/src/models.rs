use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubmitOrderRequest {
    pub account_id: String,
    pub symbol: String,
    pub side: String,
    pub qty: f64,
    pub order_type: String,
    pub limit_price: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CancelOrderRequest {
    pub account_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AmendOrderRequest {
    pub account_id: String,
    pub order_id: String,
    pub qty: f64,
    pub limit_price: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OrderActionResult {
    pub order_id: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TerminalWatchRow {
    pub symbol: String,
    pub venue: String,
    pub last_price: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TerminalOverview {
    pub account_id: String,
    pub runtime_mode: String,
    pub watchlist: Vec<TerminalWatchRow>,
    pub positions: Vec<LocalPositionRow>,
    pub open_orders: Vec<OpenOrderRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalPositionRow {
    pub venue: String,
    pub symbol: String,
    pub net_qty: f64,
    pub last_fill_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OpenOrderRow {
    pub order_id: String,
    pub venue: String,
    pub symbol: String,
    pub side: String,
    pub qty: f64,
    pub status: String,
    pub order_type: String,
    pub limit_price: Option<f64>,
    pub exchange_ref: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OrderRow {
    pub order_id: String,
    pub venue: String,
    pub symbol: String,
    pub side: String,
    pub qty: f64,
    pub status: String,
    pub order_type: String,
    pub limit_price: Option<f64>,
    pub exchange_ref: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QuoteBar {
    pub ts_ms: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QuoteView {
    pub symbol: String,
    pub venue: String,
    pub last_price: Option<f64>,
    pub day_high: Option<f64>,
    pub day_low: Option<f64>,
    pub bars: Vec<QuoteBar>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StreamMessage {
    Hello {
        schema_version: u32,
    },
    OrderCreated {
        payload: serde_json::Value,
    },
    OrderUpdated {
        payload: serde_json::Value,
    },
    OrderCancelled {
        payload: serde_json::Value,
    },
    OrderReplaced {
        payload: serde_json::Value,
    },
    QuoteUpdated {
        payload: serde_json::Value,
    },
    Error {
        error_code: String,
        message: String,
    },
}
