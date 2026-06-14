use events::{SignalEvent, SignalSide};
use portfolio::equal_weight_target;
use rust_decimal_macros::dec;

#[test]
fn buy_signal_targets_configured_long_quantity() {
    let target = equal_weight_target(&signal(SignalSide::Buy), dec!(2));

    assert_eq!(target.target_qty, dec!(2));
}

#[test]
fn sell_signal_targets_configured_short_quantity() {
    let target = equal_weight_target(&signal(SignalSide::Sell), dec!(2));

    assert_eq!(target.target_qty, dec!(-2));
}

#[test]
fn close_long_signal_targets_flat_position() {
    let target = equal_weight_target(&signal(SignalSide::CloseLong), dec!(2));

    assert_eq!(target.target_qty, dec!(0));
}

#[test]
fn close_short_signal_targets_flat_position() {
    let target = equal_weight_target(&signal(SignalSide::CloseShort), dec!(2));

    assert_eq!(target.target_qty, dec!(0));
}

fn signal(side: SignalSide) -> SignalEvent {
    SignalEvent {
        strategy_id: "moving_average_cross".to_string(),
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        side,
        confidence: 1.0,
        ts: chrono::DateTime::from_timestamp_millis(1).unwrap(),
    }
}
