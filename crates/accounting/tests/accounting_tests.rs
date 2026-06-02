use accounting::{AccountBook, PositionBook};
use rust_decimal_macros::dec;

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
