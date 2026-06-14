use indicators::{
    ExponentialMovingAverage, RelativeStrengthIndex, SimpleMovingAverage, indicator_ema,
    indicator_rsi, indicator_sma,
};
use rust_decimal_macros::dec;

#[test]
fn simple_moving_average_returns_none_until_window_is_full() {
    let mut average = SimpleMovingAverage::new(3).unwrap();

    assert_eq!(average.update(dec!(10)), None);
    assert_eq!(average.update(dec!(20)), None);
    assert_eq!(average.update(dec!(30)), Some(dec!(20)));
    assert_eq!(
        average.update(dec!(60)),
        Some(dec!(36.666666666666666666666666667))
    );
}

#[test]
fn exponential_moving_average_uses_standard_smoothing_factor() {
    let mut average = ExponentialMovingAverage::new(3).unwrap();

    assert_eq!(average.update(dec!(10)), Some(dec!(10)));
    assert_eq!(average.update(dec!(20)), Some(dec!(15.0)));
    assert_eq!(average.update(dec!(30)), Some(dec!(22.5)));
}

#[test]
fn indicator_helpers_reject_zero_period() {
    assert!(SimpleMovingAverage::new(0).is_err());
    assert!(ExponentialMovingAverage::new(0).is_err());
    assert!(RelativeStrengthIndex::new(0).is_err());
}

#[test]
fn indicator_helpers_calculate_batch_values() {
    let values = [dec!(10), dec!(20), dec!(30), dec!(60)];

    assert_eq!(
        indicator_sma(&values, 3).unwrap(),
        Some(dec!(36.666666666666666666666666667))
    );
    assert_eq!(indicator_ema(&values, 3).unwrap(), Some(dec!(41.250000)));
}

#[test]
fn relative_strength_index_reports_oversold_and_overbought_levels() {
    let mut oversold = RelativeStrengthIndex::new(3).unwrap();
    assert_eq!(oversold.update(dec!(10)), None);
    assert_eq!(oversold.update(dec!(9)), None);
    assert_eq!(oversold.update(dec!(8)), None);
    assert_eq!(oversold.update(dec!(7)), Some(dec!(0)));

    let mut overbought = RelativeStrengthIndex::new(3).unwrap();
    assert_eq!(overbought.update(dec!(10)), None);
    assert_eq!(overbought.update(dec!(11)), None);
    assert_eq!(overbought.update(dec!(12)), None);
    assert_eq!(overbought.update(dec!(13)), Some(dec!(100)));
}

#[test]
fn indicator_rsi_calculates_batch_value() {
    let values = [dec!(10), dec!(9), dec!(8), dec!(7)];

    assert_eq!(indicator_rsi(&values, 3).unwrap(), Some(dec!(0)));
}
