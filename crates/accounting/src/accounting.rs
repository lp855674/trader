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
    #[error("quantity must be positive")]
    InvalidQuantity,
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
    fn entry_mut(&mut self, symbol: &str) -> &mut Position {
        self.positions
            .entry(symbol.to_string())
            .or_insert(Position {
                symbol: symbol.to_string(),
                qty: Decimal::ZERO,
                avg_price: Decimal::ZERO,
            })
    }

    pub fn buy(&mut self, symbol: &str, qty: Decimal, price: Decimal) {
        let entry = self.entry_mut(symbol);
        increase_long_position(entry, qty, price);
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
        let mut realized_pnl = Decimal::ZERO;
        {
            let position = self.positions.entry_mut(symbol);
            if position.qty < Decimal::ZERO {
                let closing_qty = qty.min(position.qty.abs());
                if closing_qty > Decimal::ZERO {
                    realized_pnl += closing_qty * (position.avg_price - price) - fee;
                    position.qty += closing_qty;
                    if position.qty == Decimal::ZERO {
                        position.avg_price = Decimal::ZERO;
                    }
                }

                let opening_qty = qty - closing_qty;
                if opening_qty > Decimal::ZERO {
                    position.qty = opening_qty;
                    position.avg_price = price;
                }
            } else {
                increase_long_position(position, qty, price);
            }
        }
        self.realized_pnl += realized_pnl;
    }

    pub fn sell(
        &mut self,
        symbol: &str,
        qty: Decimal,
        price: Decimal,
        fee: Decimal,
    ) -> Result<(), AccountingError> {
        if qty <= Decimal::ZERO {
            return Err(AccountingError::InvalidQuantity);
        }
        self.cash += qty * price - fee;
        let mut realized_pnl = Decimal::ZERO;
        {
            let position = self.positions.entry_mut(symbol);
            if position.qty > Decimal::ZERO {
                let closing_qty = qty.min(position.qty);
                if closing_qty > Decimal::ZERO {
                    realized_pnl += closing_qty * (price - position.avg_price) - fee;
                    position.qty -= closing_qty;
                    if position.qty == Decimal::ZERO {
                        position.avg_price = Decimal::ZERO;
                    }
                }

                let opening_qty = qty - closing_qty;
                if opening_qty > Decimal::ZERO {
                    position.qty = -opening_qty;
                    position.avg_price = price;
                }
            } else {
                increase_short_position(position, qty, price);
            }
        }
        self.realized_pnl += realized_pnl;
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

    pub fn gross_exposure(&self, symbol: &str, mark_price: Decimal) -> Decimal {
        self.position(symbol)
            .map_or(Decimal::ZERO, |position| (position.qty * mark_price).abs())
    }

    pub fn gross_exposure_with_prices(
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
                (position.qty * mark_price).abs()
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

fn increase_long_position(position: &mut Position, qty: Decimal, price: Decimal) {
    let new_qty = position.qty + qty;
    if new_qty == Decimal::ZERO {
        position.qty = Decimal::ZERO;
        position.avg_price = Decimal::ZERO;
        return;
    }

    let notional = position.qty * position.avg_price + qty * price;
    position.qty = new_qty;
    position.avg_price = notional / new_qty;
}

fn increase_short_position(position: &mut Position, qty: Decimal, price: Decimal) {
    let current_short_qty = position.qty.abs();
    let new_short_qty = current_short_qty + qty;
    if new_short_qty == Decimal::ZERO {
        position.qty = Decimal::ZERO;
        position.avg_price = Decimal::ZERO;
        return;
    }

    let notional = current_short_qty * position.avg_price + qty * price;
    position.qty = -new_short_qty;
    position.avg_price = notional / new_short_qty;
}
