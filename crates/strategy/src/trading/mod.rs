pub mod paper;
pub use paper::{PaperAdapter, PaperConfig, PaperState, MarketDataSnapshot};

pub mod intent;
pub use intent::{IntentProcessor, StrategyOrderIntent, IntentConfig, IntentError};

pub mod position;
pub use position::{PositionManager, PositionEntry, ExposureLimit, PositionError};
