pub mod optimizer;
pub use optimizer::{
    OptimizationRequest, OptimizationResponse, OptimizationService, OptimizationStatus,
};

pub mod grpc;
pub use grpc::{MetricsService, PaperTradingService, StrategyManagementService};

pub mod http;
pub use http::{ApiError, HealthResponse, HttpRouter};
