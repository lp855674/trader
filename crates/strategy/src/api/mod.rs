pub mod optimizer;
pub use optimizer::{OptimizationRequest, OptimizationResponse, OptimizationService, OptimizationStatus};

pub mod grpc;
pub use grpc::{StrategyManagementService, PaperTradingService, MetricsService};

pub mod http;
pub use http::{HttpRouter, HealthResponse, ApiError};
