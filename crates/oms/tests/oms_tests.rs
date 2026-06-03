use oms::OrderStateMachine;
use rust_decimal::Decimal;
use trader_core::OrderStatus;

#[test]
fn submitted_order_can_fill() {
    let mut machine = OrderStateMachine::new();
    machine.submit().unwrap();
    machine.accept().unwrap();
    machine.fill().unwrap();

    assert_eq!(machine.status(), OrderStatus::Filled);
}

#[test]
fn partial_fill_tracks_cumulative_and_remaining_quantity() {
    let mut machine = OrderStateMachine::with_order_qty(Decimal::from(10));
    machine.submit().unwrap();
    machine.accept().unwrap();

    machine.record_fill(Decimal::from(4)).unwrap();
    assert_eq!(machine.status(), OrderStatus::PartiallyFilled);
    assert_eq!(machine.filled_qty(), Decimal::from(4));
    assert_eq!(machine.remaining_qty(), Decimal::from(6));

    machine.record_fill(Decimal::from(6)).unwrap();
    assert_eq!(machine.status(), OrderStatus::Filled);
    assert_eq!(machine.remaining_qty(), Decimal::ZERO);
}

#[test]
fn rejects_overfill() {
    let mut machine = OrderStateMachine::with_order_qty(Decimal::from(10));
    machine.submit().unwrap();
    machine.accept().unwrap();

    assert_eq!(
        machine.record_fill(Decimal::from(11)).unwrap_err(),
        oms::OmsError::Overfill
    );
}

#[test]
fn cancel_requested_order_can_cancel_before_fill() {
    let mut machine = OrderStateMachine::with_order_qty(Decimal::from(10));
    machine.submit().unwrap();
    machine.request_cancel().unwrap();
    machine.cancel().unwrap();

    assert_eq!(machine.status(), OrderStatus::Canceled);
}

#[test]
fn duplicate_fill_report_is_applied_once() {
    let mut machine = OrderStateMachine::with_order_qty(Decimal::from(10));
    machine.submit().unwrap();
    machine.accept().unwrap();

    assert!(
        machine
            .apply_fill_report("fill-1", Decimal::from(4))
            .unwrap()
    );
    assert!(
        !machine
            .apply_fill_report("fill-1", Decimal::from(4))
            .unwrap()
    );

    assert_eq!(machine.status(), OrderStatus::PartiallyFilled);
    assert_eq!(machine.filled_qty(), Decimal::from(4));
    assert_eq!(machine.remaining_qty(), Decimal::from(6));
}

#[test]
fn late_cancel_report_after_fill_is_ignored() {
    let mut machine = OrderStateMachine::with_order_qty(Decimal::from(10));
    machine.submit().unwrap();
    machine.accept().unwrap();
    machine.record_fill(Decimal::from(10)).unwrap();

    assert!(!machine.apply_cancel_report("cancel-1").unwrap());

    assert_eq!(machine.status(), OrderStatus::Filled);
}

#[test]
fn reject_report_moves_submitted_order_to_rejected() {
    let mut machine = OrderStateMachine::with_order_qty(Decimal::from(10));
    machine.submit().unwrap();

    assert!(machine.apply_reject_report("reject-1").unwrap());

    assert_eq!(machine.status(), OrderStatus::Rejected);
}
