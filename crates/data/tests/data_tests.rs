use data::Bar;
use rust_decimal_macros::dec;

#[test]
fn bar_return_uses_close_to_close() {
    let previous = Bar::new(1, dec!(100), dec!(110), dec!(90), dec!(100), dec!(1000));
    let current = Bar::new(2, dec!(100), dec!(115), dec!(95), dec!(110), dec!(1200));

    assert_eq!(current.close_return(&previous), dec!(0.1));
}
