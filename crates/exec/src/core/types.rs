use domain::{InstrumentId, Side};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OrderKind {
    Market,
    Limit { price: f64 },
    Stop { stop: f64 },
    StopLimit { stop: f64, limit: f64 },
    Twap { duration_ms: u64, slices: u32 },
    Vwap { duration_ms: u64 },
    Iceberg { display_qty: f64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TimeInForce {
    GTC,
    IOC,
    FOK,
    GTD { expires_ms: i64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OrderFlag {
    PostOnly,
    ReduceOnly,
    AllowPartialFill,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderRequest {
    pub client_order_id: String,
    pub instrument: InstrumentId,
    pub side: Side,
    pub quantity: f64,
    pub kind: OrderKind,
    pub tif: TimeInForce,
    pub flags: Vec<OrderFlag>,
    pub strategy_id: String,
    pub submitted_ts_ms: i64,
}
