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
