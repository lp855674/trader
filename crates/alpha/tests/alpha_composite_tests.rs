use alpha::{AlphaModel, CompositeAlphaModel};
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
