use crate::{AssetClass, Market};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Symbol {
    pub market: Market,
    pub exchange: String,
    pub code: String,
    pub asset_class: AssetClass,
}

impl Symbol {
    pub fn new(
        market: Market,
        exchange: impl Into<String>,
        code: impl Into<String>,
        asset_class: AssetClass,
    ) -> Self {
        Self {
            market,
            exchange: exchange.into(),
            code: code.into(),
            asset_class,
        }
    }
}

impl fmt::Display for Symbol {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}:{}:{}:{}",
            self.market.code(),
            self.exchange,
            self.code,
            self.asset_class.code()
        )
    }
}
