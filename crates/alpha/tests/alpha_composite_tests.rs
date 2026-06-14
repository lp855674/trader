use alpha::{
    AlphaModel, CompositeAlphaModel, MajorityVoteAlphaModel, NetSignalAlphaModel,
    WeightedAlphaModel,
};
use data::Bar;
use events::{SignalEvent, SignalSide};
use rust_decimal_macros::dec;

#[test]
fn composite_alpha_returns_highest_confidence_signal() {
    let mut alpha = CompositeAlphaModel::new(vec![
        Box::new(FixedAlphaModel::signal("low", SignalSide::Buy, 0.4)),
        Box::new(FixedAlphaModel::signal("high", SignalSide::Sell, 0.9)),
    ]);

    let signal = alpha.on_bar(&bar()).unwrap();

    assert_eq!(signal.strategy_id, "high");
    assert_eq!(signal.side, SignalSide::Sell);
    assert_eq!(signal.confidence, 0.9);
}

#[test]
fn composite_alpha_returns_none_when_no_models_emit_signal() {
    let mut alpha = CompositeAlphaModel::new(vec![
        Box::new(FixedAlphaModel::none()),
        Box::new(FixedAlphaModel::none()),
    ]);

    assert!(alpha.on_bar(&bar()).is_none());
}

#[test]
fn net_signal_alpha_offsets_conflicting_signal_confidence() {
    let mut alpha = NetSignalAlphaModel::new(vec![
        Box::new(FixedAlphaModel::signal("buy", SignalSide::Buy, 0.8)),
        Box::new(FixedAlphaModel::signal("sell", SignalSide::Sell, 0.2)),
    ]);

    let signal = alpha.on_bar(&bar()).unwrap();

    assert_eq!(signal.strategy_id, "buy");
    assert_eq!(signal.side, SignalSide::Buy);
    assert!((signal.confidence - 0.6).abs() < 1e-9);
}

#[test]
fn net_signal_alpha_returns_none_when_conflicting_scores_cancel() {
    let mut alpha = NetSignalAlphaModel::new(vec![
        Box::new(FixedAlphaModel::signal("buy", SignalSide::Buy, 0.5)),
        Box::new(FixedAlphaModel::signal("sell", SignalSide::Sell, 0.5)),
    ]);

    assert!(alpha.on_bar(&bar()).is_none());
}

#[test]
fn majority_vote_alpha_prefers_more_signals_over_higher_confidence() {
    let mut alpha = MajorityVoteAlphaModel::new(vec![
        Box::new(FixedAlphaModel::signal("buy-low", SignalSide::Buy, 0.2)),
        Box::new(FixedAlphaModel::signal("buy-high", SignalSide::Buy, 0.4)),
        Box::new(FixedAlphaModel::signal("sell", SignalSide::Sell, 0.9)),
    ]);

    let signal = alpha.on_bar(&bar()).unwrap();

    assert_eq!(signal.strategy_id, "buy-high");
    assert_eq!(signal.side, SignalSide::Buy);
    assert!((signal.confidence - 0.3).abs() < 1e-9);
}

#[test]
fn majority_vote_alpha_returns_none_when_vote_counts_tie() {
    let mut alpha = MajorityVoteAlphaModel::new(vec![
        Box::new(FixedAlphaModel::signal("buy", SignalSide::Buy, 0.9)),
        Box::new(FixedAlphaModel::signal("sell", SignalSide::Sell, 0.1)),
    ]);

    assert!(alpha.on_bar(&bar()).is_none());
}

#[test]
fn weighted_alpha_scales_signal_confidence() {
    let mut alpha = WeightedAlphaModel::new(
        Box::new(FixedAlphaModel::signal("weighted", SignalSide::Buy, 0.8)),
        0.5,
    );

    let signal = alpha.on_bar(&bar()).unwrap();

    assert_eq!(signal.strategy_id, "weighted");
    assert_eq!(signal.side, SignalSide::Buy);
    assert_eq!(signal.confidence, 0.4);
}

struct FixedAlphaModel {
    signal: Option<SignalEvent>,
}

impl FixedAlphaModel {
    fn signal(strategy_id: &str, side: SignalSide, confidence: f64) -> Self {
        Self {
            signal: Some(SignalEvent {
                strategy_id: strategy_id.to_string(),
                symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
                side,
                confidence,
                ts: chrono::Utc::now(),
            }),
        }
    }

    fn none() -> Self {
        Self { signal: None }
    }
}

impl AlphaModel for FixedAlphaModel {
    fn on_bar(&mut self, _bar: &Bar) -> Option<SignalEvent> {
        self.signal.clone()
    }
}

fn bar() -> Bar {
    Bar::new(1, dec!(100), dec!(100), dec!(100), dec!(100), dec!(1))
}
