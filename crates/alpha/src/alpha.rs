#![forbid(unsafe_code)]

use data::Bar;
use events::SignalEvent;

pub trait AlphaModel {
    fn on_bar(&mut self, bar: &Bar) -> Option<SignalEvent>;
}
