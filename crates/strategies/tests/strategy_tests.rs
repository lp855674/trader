use data::Bar;
use events::SignalSide;
use rust_decimal_macros::dec;
use strategies::{MovingAverageCrossStrategy, Strategy};

#[test]
fn moving_average_cross_emits_buy_signal() {
    let mut strategy = MovingAverageCrossStrategy::new("ma", "AAPL", 2, 3);
    strategy.on_bar(&Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)));
    strategy.on_bar(&Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)));
    let signal = strategy
        .on_bar(&Bar::new(3, dec!(1), dec!(1), dec!(1), dec!(20), dec!(1)))
        .unwrap();

    assert_eq!(signal.side, SignalSide::Buy);
}
