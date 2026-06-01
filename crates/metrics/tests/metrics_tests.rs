use metrics::total_return;
use rust_decimal_macros::dec;

#[test]
fn total_return_uses_start_and_end_equity() {
    assert_eq!(total_return(dec!(100), dec!(125)), dec!(0.25));
}
