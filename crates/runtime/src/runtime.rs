#![forbid(unsafe_code)]

mod cancel;
mod manager;

pub use cancel::CancellationFlag;
pub use manager::{RunSpawnError, RuntimeManager};
