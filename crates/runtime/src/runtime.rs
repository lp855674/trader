#![forbid(unsafe_code)]

mod cancel;
mod live;
mod manager;
mod process;
mod reconciliation_gate;
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
pub use reconciliation_gate::{
    ReconciliationGateAuditLogContext, evaluate_live_reconciliation_gate_from_storage,
    evaluate_reconciliation_gate_from_storage, format_reconciliation_gate_failure,
    format_reconciliation_gate_failures, parse_reconciliation_gate_account_requirement,
    record_reconciliation_gate_decision, should_enforce_live_reconciliation_gate,
};
pub use run_spec::{
    BrokerSpec, DataInputSpec, DataSpec, PaperSpec, PortfolioSpec, RiskSpec, RunSpec, StrategySpec,
};
pub use worker_protocol::{
    LiveWorkerCommand, LiveWorkerEvent, LiveWorkerLaunchSpec, parse_worker_command_line,
    parse_worker_event_line, worker_command_line, worker_event_line,
};
