#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct Position {
    pub symbol: String,
    pub qty: Decimal,
    pub avg_price: Decimal,
}

#[derive(Debug, Clone, Default, PartialEq)]
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

#[derive(Debug, Clone, PartialEq)]
pub struct AccountBook {
    pub account_id: String,
    pub cash: Decimal,
    pub realized_pnl: Decimal,
    positions: PositionBook,
}

impl AccountBook {
    pub fn new(account_id: impl Into<String>, initial_cash: Decimal) -> Self {
        Self {
            account_id: account_id.into(),
            cash: initial_cash,
            realized_pnl: Decimal::ZERO,
            positions: PositionBook::default(),
        }
    }

    pub fn buy(&mut self, symbol: &str, qty: Decimal, price: Decimal, fee: Decimal) {
        self.cash -= qty * price + fee;
        self.positions.buy(symbol, qty, price);
    }

    pub fn position(&self, symbol: &str) -> Option<&Position> {
        self.positions.position(symbol)
    }

    pub fn market_value(&self, symbol: &str, mark_price: Decimal) -> Decimal {
        self.position(symbol)
            .map_or(Decimal::ZERO, |position| position.qty * mark_price)
    }

    pub fn equity(&self, symbol: &str, mark_price: Decimal) -> Decimal {
        self.cash + self.market_value(symbol, mark_price)
    }

    pub fn cash(&self) -> Decimal {
        self.cash
    }
}
