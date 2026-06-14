use accounting::{AccountBook, PositionBook};
use rust_decimal_macros::dec;
use std::collections::BTreeMap;

#[test]
fn buy_updates_average_price() {
    let mut book = PositionBook::default();
    book.buy("AAPL", dec!(10), dec!(100));
    book.buy("AAPL", dec!(10), dec!(120));
    let position = book.position("AAPL").unwrap();

    assert_eq!(position.qty, dec!(20));
    assert_eq!(position.avg_price, dec!(110));
}

#[test]
fn account_buy_decreases_cash_and_increases_position() {
    let mut book = AccountBook::new("paper", dec!(10000));

    book.buy("AAPL", dec!(2), dec!(100), dec!(1));
    let position = book.position("AAPL").unwrap();

    assert_eq!(book.cash(), dec!(9799));
    assert_eq!(position.qty, dec!(2));
    assert_eq!(position.avg_price, dec!(100));
}

#[test]
fn account_equity_equals_cash_plus_market_value() {
    let mut book = AccountBook::new("paper", dec!(10000));
    book.buy("AAPL", dec!(2), dec!(100), dec!(1));

    assert_eq!(book.market_value("AAPL", dec!(110)), dec!(220));
    assert_eq!(book.equity("AAPL", dec!(110)), dec!(10019));
}

#[test]
fn account_average_price_remains_decimal_precise() {
    let mut book = AccountBook::new("paper", dec!(10000));

    book.buy("AAPL", dec!(0.1), dec!(100.10), dec!(0));
    book.buy("AAPL", dec!(0.2), dec!(100.20), dec!(0));
    let position = book.position("AAPL").unwrap();

    assert_eq!(position.qty, dec!(0.3));
    assert_eq!(position.avg_price, dec!(100.16666666666666666666666667));
}

#[test]
fn account_sell_decreases_position_and_increases_cash() {
    let mut book = AccountBook::new("paper", dec!(10000));
    book.buy("AAPL", dec!(2), dec!(100), dec!(1));

    book.sell("AAPL", dec!(1), dec!(110), dec!(0.5)).unwrap();
    let position = book.position("AAPL").unwrap();

    assert_eq!(book.cash(), dec!(9908.5));
    assert_eq!(position.qty, dec!(1));
    assert_eq!(position.avg_price, dec!(100));
    assert_eq!(book.realized_pnl(), dec!(9.5));
}

#[test]
fn account_sell_opens_short_position_and_increases_cash() {
    let mut book = AccountBook::new("paper", dec!(10000));

    book.sell("AAPL", dec!(2), dec!(100), dec!(1)).unwrap();
    let position = book.position("AAPL").unwrap();

    assert_eq!(book.cash(), dec!(10199));
    assert_eq!(position.qty, dec!(-2));
    assert_eq!(position.avg_price, dec!(100));
    assert_eq!(book.market_value("AAPL", dec!(90)), dec!(-180));
    assert_eq!(book.equity("AAPL", dec!(90)), dec!(10019));
    assert_eq!(book.unrealized_pnl("AAPL", dec!(90)), dec!(20));
}

#[test]
fn account_buy_closes_short_position_and_realizes_pnl() {
    let mut book = AccountBook::new("paper", dec!(10000));
    book.sell("AAPL", dec!(2), dec!(100), dec!(0)).unwrap();

    book.buy("AAPL", dec!(1), dec!(90), dec!(0.5));
    let position = book.position("AAPL").unwrap();

    assert_eq!(book.cash(), dec!(10109.5));
    assert_eq!(position.qty, dec!(-1));
    assert_eq!(position.avg_price, dec!(100));
    assert_eq!(book.realized_pnl(), dec!(9.5));
}

#[test]
fn account_unrealized_pnl_uses_mark_price() {
    let mut book = AccountBook::new("paper", dec!(10000));
    book.buy("AAPL", dec!(2), dec!(100), dec!(1));

    assert_eq!(book.unrealized_pnl("AAPL", dec!(110)), dec!(20));
}

#[test]
fn account_values_multiple_positions_with_symbol_prices() {
    let mut book = AccountBook::new("paper", dec!(10000));
    book.buy("AAPL", dec!(2), dec!(100), dec!(0));
    book.buy("MSFT", dec!(3), dec!(50), dec!(0));
    let prices = BTreeMap::from([
        ("AAPL".to_string(), dec!(110)),
        ("MSFT".to_string(), dec!(40)),
    ]);

    assert_eq!(book.market_value_with_prices(&prices), dec!(340));
    assert_eq!(book.equity_with_prices(&prices), dec!(9990));
    assert_eq!(book.unrealized_pnl_with_prices(&prices), dec!(-10));
    assert_eq!(
        book.positions()
            .into_iter()
            .map(|position| position.symbol.clone())
            .collect::<Vec<_>>(),
        vec!["AAPL".to_string(), "MSFT".to_string()]
    );
}

#[test]
fn account_gross_exposure_uses_absolute_short_market_value() {
    let mut book = AccountBook::new("paper", dec!(10000));
    book.sell("AAPL", dec!(2), dec!(100), dec!(0)).unwrap();
    let prices = BTreeMap::from([("AAPL".to_string(), dec!(90))]);

    assert_eq!(book.market_value_with_prices(&prices), dec!(-180));
    assert_eq!(book.gross_exposure_with_prices(&prices), dec!(180));
}
