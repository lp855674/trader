#![forbid(unsafe_code)]

mod cancel;
mod live;
mod manager;
mod run_spec;

pub use cancel::CancellationFlag;
pub use live::{
    AlertSinkSettings, LiveRuntime, LiveRuntimeSettings, StartupRecoveryUnmatchedOpenOrdersPolicy,
};
pub use manager::{
    RunSpawnError, RuntimeManager, RuntimeRunInfo, RuntimeRunMetadata, RuntimeRunSnapshot,
    RuntimeRunStatus,
};
pub use run_spec::{
    BrokerSpec, DataInputSpec, DataSpec, PaperSpec, PortfolioSpec, RiskSpec, RunSpec, StrategySpec,
};
