#![forbid(unsafe_code)]

mod cancel;
mod live;
mod manager;
mod process;
mod run_spec;
mod worker_protocol;

pub use cancel::CancellationFlag;
pub use live::{
    AlertSinkSettings, LiveRuntime, LiveRuntimeSettings, StartupRecoveryUnmatchedOpenOrdersPolicy,
};
pub use manager::{
    RunSpawnError, RuntimeManager, RuntimeRunInfo, RuntimeRunMetadata, RuntimeRunSnapshot,
    RuntimeRunStatus,
};
pub use process::{
    LiveProcessError, LiveProcessSnapshot, LiveProcessStatus, LiveProcessSupervisor,
    LiveProcessSupervisorOptions,
};
pub use run_spec::{
    BrokerSpec, DataInputSpec, DataSpec, PaperSpec, PortfolioSpec, RiskSpec, RunSpec, StrategySpec,
};
pub use worker_protocol::{
    LiveWorkerCommand, LiveWorkerEvent, LiveWorkerLaunchSpec, parse_worker_command_line,
    parse_worker_event_line, worker_command_line, worker_event_line,
};
