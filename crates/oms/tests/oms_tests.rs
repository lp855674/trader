use oms::OrderStateMachine;
use trader_core::OrderStatus;

#[test]
fn submitted_order_can_fill() {
    let mut machine = OrderStateMachine::new();
    machine.submit().unwrap();
    machine.accept().unwrap();
    machine.fill().unwrap();

    assert_eq!(machine.status(), OrderStatus::Filled);
}
