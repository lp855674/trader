#![forbid(unsafe_code)]

use data::Bar;
use events::SignalEvent;

pub trait AlphaModel: Send + Sync {
    fn on_bar(&mut self, bar: &Bar) -> Option<SignalEvent>;

    fn on_bar_for_symbol(&mut self, _symbol: &str, bar: &Bar) -> Option<SignalEvent> {
        self.on_bar(bar)
    }
}

pub struct CompositeAlphaModel {
    models: Vec<Box<dyn AlphaModel + Send + Sync>>,
}

impl CompositeAlphaModel {
    pub fn new(models: Vec<Box<dyn AlphaModel + Send + Sync>>) -> Self {
        Self { models }
    }
}

pub struct NetSignalAlphaModel {
    models: Vec<Box<dyn AlphaModel + Send + Sync>>,
}

impl NetSignalAlphaModel {
    pub fn new(models: Vec<Box<dyn AlphaModel + Send + Sync>>) -> Self {
        Self { models }
    }
}

pub struct MajorityVoteAlphaModel {
    models: Vec<Box<dyn AlphaModel + Send + Sync>>,
}

impl MajorityVoteAlphaModel {
    pub fn new(models: Vec<Box<dyn AlphaModel + Send + Sync>>) -> Self {
        Self { models }
    }
}

pub struct WeightedAlphaModel {
    model: Box<dyn AlphaModel + Send + Sync>,
    weight: f64,
}

impl WeightedAlphaModel {
    pub fn new(model: Box<dyn AlphaModel + Send + Sync>, weight: f64) -> Self {
        Self { model, weight }
    }
}

impl AlphaModel for WeightedAlphaModel {
    fn on_bar(&mut self, bar: &Bar) -> Option<SignalEvent> {
        self.model
            .on_bar(bar)
            .map(|signal| weighted_signal(signal, self.weight))
    }

    fn on_bar_for_symbol(&mut self, symbol: &str, bar: &Bar) -> Option<SignalEvent> {
        self.model
            .on_bar_for_symbol(symbol, bar)
            .map(|signal| weighted_signal(signal, self.weight))
    }
}

impl AlphaModel for CompositeAlphaModel {
    fn on_bar(&mut self, bar: &Bar) -> Option<SignalEvent> {
        self.models
            .iter_mut()
            .filter_map(|model| model.on_bar(bar))
            .max_by(|left, right| left.confidence.total_cmp(&right.confidence))
    }

    fn on_bar_for_symbol(&mut self, symbol: &str, bar: &Bar) -> Option<SignalEvent> {
        self.models
            .iter_mut()
            .filter_map(|model| model.on_bar_for_symbol(symbol, bar))
            .max_by(|left, right| left.confidence.total_cmp(&right.confidence))
    }
}

impl AlphaModel for NetSignalAlphaModel {
    fn on_bar(&mut self, bar: &Bar) -> Option<SignalEvent> {
        net_signal(self.models.iter_mut().filter_map(|model| model.on_bar(bar)))
    }

    fn on_bar_for_symbol(&mut self, symbol: &str, bar: &Bar) -> Option<SignalEvent> {
        net_signal(
            self.models
                .iter_mut()
                .filter_map(|model| model.on_bar_for_symbol(symbol, bar)),
        )
    }
}

impl AlphaModel for MajorityVoteAlphaModel {
    fn on_bar(&mut self, bar: &Bar) -> Option<SignalEvent> {
        majority_vote(self.models.iter_mut().filter_map(|model| model.on_bar(bar)))
    }

    fn on_bar_for_symbol(&mut self, symbol: &str, bar: &Bar) -> Option<SignalEvent> {
        majority_vote(
            self.models
                .iter_mut()
                .filter_map(|model| model.on_bar_for_symbol(symbol, bar)),
        )
    }
}

fn weighted_signal(mut signal: SignalEvent, weight: f64) -> SignalEvent {
    signal.confidence *= weight;
    signal
}

fn net_signal(signals: impl IntoIterator<Item = SignalEvent>) -> Option<SignalEvent> {
    let mut net_confidence = 0.0;
    let mut positive_signal = None;
    let mut negative_signal = None;

    for signal in signals {
        if !signal.confidence.is_finite() || signal.confidence <= 0.0 {
            continue;
        }
        match signal.side {
            events::SignalSide::Buy | events::SignalSide::CloseShort => {
                net_confidence += signal.confidence;
                positive_signal = strongest_signal(positive_signal, signal);
            }
            events::SignalSide::Sell | events::SignalSide::CloseLong => {
                net_confidence -= signal.confidence;
                negative_signal = strongest_signal(negative_signal, signal);
            }
        }
    }

    const TIE_EPSILON: f64 = 1e-12;
    if net_confidence > TIE_EPSILON {
        let mut signal = positive_signal?;
        signal.confidence = net_confidence;
        Some(signal)
    } else if net_confidence < -TIE_EPSILON {
        let mut signal = negative_signal?;
        signal.confidence = net_confidence.abs();
        Some(signal)
    } else {
        None
    }
}

fn majority_vote(signals: impl IntoIterator<Item = SignalEvent>) -> Option<SignalEvent> {
    let mut positive = VoteBucket::default();
    let mut negative = VoteBucket::default();

    for signal in signals {
        if !signal.confidence.is_finite() || signal.confidence <= 0.0 {
            continue;
        }
        match signal.side {
            events::SignalSide::Buy | events::SignalSide::CloseShort => positive.add(signal),
            events::SignalSide::Sell | events::SignalSide::CloseLong => negative.add(signal),
        }
    }

    if positive.count > negative.count {
        positive.into_signal()
    } else if negative.count > positive.count {
        negative.into_signal()
    } else {
        None
    }
}

#[derive(Default)]
struct VoteBucket {
    count: u32,
    confidence_sum: f64,
    strongest: Option<SignalEvent>,
}

impl VoteBucket {
    fn add(&mut self, signal: SignalEvent) {
        self.count += 1;
        self.confidence_sum += signal.confidence;
        self.strongest = strongest_signal(self.strongest.take(), signal);
    }

    fn into_signal(self) -> Option<SignalEvent> {
        let mut signal = self.strongest?;
        signal.confidence = self.confidence_sum / f64::from(self.count);
        Some(signal)
    }
}

fn strongest_signal(current: Option<SignalEvent>, candidate: SignalEvent) -> Option<SignalEvent> {
    match current {
        Some(current) if current.confidence >= candidate.confidence => Some(current),
        _ => Some(candidate),
    }
}
