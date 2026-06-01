#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct Position {
    pub symbol: String,
    pub qty: Decimal,
    pub avg_price: Decimal,
}

#[derive(Default)]
pub struct PositionBook {
    positions: HashMap<String, Position>,
}

impl PositionBook {
    pub fn buy(&mut self, symbol: &str, qty: Decimal, price: Decimal) {
        let entry = self
            .positions
            .entry(symbol.to_string())
            .or_insert(Position {
                symbol: symbol.to_string(),
                qty: Decimal::ZERO,
                avg_price: Decimal::ZERO,
            });
        let notional = entry.qty * entry.avg_price + qty * price;
        entry.qty += qty;
        entry.avg_price = notional / entry.qty;
    }

    pub fn position(&self, symbol: &str) -> Option<&Position> {
        self.positions.get(symbol)
    }
}
