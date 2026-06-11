#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AccountingError {
    #[error("position not found")]
    PositionNotFound,
    #[error("sell quantity exceeds position")]
    InsufficientPosition,
}

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

    pub fn position_mut(&mut self, symbol: &str) -> Option<&mut Position> {
        self.positions.get_mut(symbol)
    }

    pub fn positions(&self) -> Vec<&Position> {
        let mut positions = self.positions.values().collect::<Vec<_>>();
        positions.sort_by(|left, right| left.symbol.cmp(&right.symbol));
        positions
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

    pub fn sell(
        &mut self,
        symbol: &str,
        qty: Decimal,
        price: Decimal,
        fee: Decimal,
    ) -> Result<(), AccountingError> {
        let position = self
            .positions
            .position_mut(symbol)
            .ok_or(AccountingError::PositionNotFound)?;
        if qty > position.qty {
            return Err(AccountingError::InsufficientPosition);
        }
        self.cash += qty * price - fee;
        self.realized_pnl += qty * (price - position.avg_price) - fee;
        position.qty -= qty;
        Ok(())
    }

    pub fn position(&self, symbol: &str) -> Option<&Position> {
        self.positions.position(symbol)
    }

    pub fn positions(&self) -> Vec<&Position> {
        self.positions.positions()
    }

    pub fn market_value(&self, symbol: &str, mark_price: Decimal) -> Decimal {
        self.position(symbol)
            .map_or(Decimal::ZERO, |position| position.qty * mark_price)
    }

    pub fn market_value_with_prices(
        &self,
        mark_prices: &std::collections::BTreeMap<String, Decimal>,
    ) -> Decimal {
        self.positions()
            .into_iter()
            .map(|position| {
                position.qty
                    * mark_prices
                        .get(&position.symbol)
                        .copied()
                        .unwrap_or(position.avg_price)
            })
            .fold(Decimal::ZERO, |sum, value| sum + value)
    }

    pub fn equity(&self, symbol: &str, mark_price: Decimal) -> Decimal {
        self.cash + self.market_value(symbol, mark_price)
    }

    pub fn equity_with_prices(
        &self,
        mark_prices: &std::collections::BTreeMap<String, Decimal>,
    ) -> Decimal {
        self.cash + self.market_value_with_prices(mark_prices)
    }

    pub fn cash(&self) -> Decimal {
        self.cash
    }

    pub fn realized_pnl(&self) -> Decimal {
        self.realized_pnl
    }

    pub fn unrealized_pnl(&self, symbol: &str, mark_price: Decimal) -> Decimal {
        self.position(symbol).map_or(Decimal::ZERO, |position| {
            position.qty * (mark_price - position.avg_price)
        })
    }

    pub fn unrealized_pnl_with_prices(
        &self,
        mark_prices: &std::collections::BTreeMap<String, Decimal>,
    ) -> Decimal {
        self.positions()
            .into_iter()
            .map(|position| {
                let mark_price = mark_prices
                    .get(&position.symbol)
                    .copied()
                    .unwrap_or(position.avg_price);
                position.qty * (mark_price - position.avg_price)
            })
            .fold(Decimal::ZERO, |sum, value| sum + value)
    }
}
