use accounting::PositionBook;
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
