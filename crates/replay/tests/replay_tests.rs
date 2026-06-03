use data::Bar;
use replay::{ReplayController, ReplayRuntime, ReplayStatus};
use rust_decimal_macros::dec;

#[tokio::test]
async fn replay_emits_all_bars() {
    let bars = vec![
        Bar::new(1, dec!(1), dec!(1), dec!(1), dec!(10), dec!(1)),
        Bar::new(2, dec!(1), dec!(1), dec!(1), dec!(11), dec!(1)),
    ];
    let summary = ReplayRuntime::new(100).replay_bars(bars).await;

    assert_eq!(summary.bars, 2);
    assert_eq!(summary.speed, 100);
}

#[test]
fn replay_controller_pauses_resumes_seeks_and_updates_speed() {
    let mut controller = ReplayController::new("run-1", 100);

    assert_eq!(controller.state().run_id, "run-1");
    assert_eq!(controller.state().status, ReplayStatus::Running);
    assert_eq!(controller.state().speed, 100);
    assert_eq!(controller.state().offset, 0);

    controller.pause();
    assert_eq!(controller.state().status, ReplayStatus::Paused);

    controller.seek(42);
    assert_eq!(controller.state().offset, 42);

    controller.set_speed(0);
    assert_eq!(controller.state().speed, 1);

    controller.resume();
    assert_eq!(controller.state().status, ReplayStatus::Running);
}
