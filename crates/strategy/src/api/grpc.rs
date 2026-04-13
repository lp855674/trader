// gRPC-style in-process service structs.  No tonic — pure Rust, but designed
// so the handler logic can be wired to a real framework later.

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::core::metrics::MetricsRegistry;
use crate::core::registry::{StrategyFactory, StrategyRegistry};
use crate::core::r#trait::StrategyContext;
use crate::trading::paper::PaperAdapter;
use domain::{InstrumentId, Side, Venue};

// ─── Request / Response types ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterStrategyRequest {
    pub id: String,
    pub strategy_type: String,
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterStrategyResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluateRequest {
    pub strategy_id: String,
    pub instrument: String,
    pub ts_ms: i64,
    pub last_bar_close: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluateResponse {
    pub signal: Option<serde_json::Value>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitOrderRequest {
    pub instrument: String,
    pub side: String,
    pub quantity: f64,
    pub limit_price: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitOrderResponse {
    pub order_id: u64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetMetricsRequest {
    pub strategy_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetMetricsResponse {
    pub metrics: Option<serde_json::Value>,
    pub found: bool,
}

// ─── StrategyManagementService ────────────────────────────────────────────────

pub struct StrategyManagementService {
    registry: Arc<Mutex<StrategyRegistry>>,
    factory: Arc<StrategyFactory>,
}

impl StrategyManagementService {
    pub fn new(registry: Arc<Mutex<StrategyRegistry>>, factory: Arc<StrategyFactory>) -> Self {
        Self { registry, factory }
    }

    pub fn register(&self, req: RegisterStrategyRequest) -> RegisterStrategyResponse {
        match self.factory.create(&req.strategy_type, req.config.clone()) {
            Ok(strategy) => {
                let reg = self.registry.lock().unwrap();
                reg.register(req.id.clone(), strategy, req.config);
                RegisterStrategyResponse {
                    success: true,
                    message: format!("Registered strategy '{}'", req.id),
                }
            }
            Err(e) => RegisterStrategyResponse {
                success: false,
                message: e.to_string(),
            },
        }
    }

    pub fn evaluate(&self, req: EvaluateRequest) -> EvaluateResponse {
        // Parse instrument — format "venue:symbol" or just "symbol"
        let instrument = if req.instrument.contains(':') {
            let parts: Vec<&str> = req.instrument.splitn(2, ':').collect();
            InstrumentId::new(Venue::Crypto, parts[1])
        } else {
            InstrumentId::new(Venue::Crypto, req.instrument.as_str())
        };

        let strategy = {
            let reg = self.registry.lock().unwrap();
            reg.get(&req.strategy_id)
        };

        match strategy {
            None => EvaluateResponse {
                signal: None,
                error: Some(format!("Strategy '{}' not found", req.strategy_id)),
            },
            Some(s) => {
                let mut ctx = StrategyContext::new(instrument, req.ts_ms);
                ctx.update(req.last_bar_close, Some(req.ts_ms));

                match s.evaluate(&ctx) {
                    Ok(Some(sig)) => {
                        let val = serde_json::to_value(&sig).unwrap_or(serde_json::Value::Null);
                        EvaluateResponse {
                            signal: Some(val),
                            error: None,
                        }
                    }
                    Ok(None) => EvaluateResponse {
                        signal: None,
                        error: None,
                    },
                    Err(e) => EvaluateResponse {
                        signal: None,
                        error: Some(e.to_string()),
                    },
                }
            }
        }
    }
}

// ─── PaperTradingService ──────────────────────────────────────────────────────

pub struct PaperTradingService {
    adapter: Arc<Mutex<PaperAdapter>>,
}

impl PaperTradingService {
    pub fn new(adapter: Arc<Mutex<PaperAdapter>>) -> Self {
        Self { adapter }
    }

    pub fn submit_order(&self, req: SubmitOrderRequest, ts_ms: i64) -> SubmitOrderResponse {
        let instrument = if req.instrument.contains(':') {
            let parts: Vec<&str> = req.instrument.splitn(2, ':').collect();
            InstrumentId::new(Venue::Crypto, parts[1])
        } else {
            InstrumentId::new(Venue::Crypto, req.instrument.as_str())
        };

        let side = if req.side.to_lowercase() == "buy" {
            Side::Buy
        } else {
            Side::Sell
        };

        let mut adapter = self.adapter.lock().unwrap();
        let order_id = adapter.submit_order(instrument, side, req.quantity, req.limit_price, ts_ms);
        SubmitOrderResponse {
            order_id,
            status: "pending".into(),
        }
    }

    pub fn get_state(&self) -> serde_json::Value {
        let adapter = self.adapter.lock().unwrap();
        let state = &adapter.state;
        serde_json::json!({
            "capital": state.capital,
            "ts_ms": state.ts_ms,
            "pending_orders": state.pending_orders.len(),
            "fills": state.fills.len(),
            "positions": state.positions.len(),
        })
    }
}

// ─── MetricsService ───────────────────────────────────────────────────────────

pub struct MetricsService {
    registry: Arc<MetricsRegistry>,
}

impl MetricsService {
    pub fn new(registry: Arc<MetricsRegistry>) -> Self {
        Self { registry }
    }

    pub fn get_metrics(&self, req: GetMetricsRequest) -> GetMetricsResponse {
        match self.registry.snapshot(&req.strategy_id) {
            None => GetMetricsResponse {
                metrics: None,
                found: false,
            },
            Some(m) => {
                let val = serde_json::json!({
                    "evaluations": m.evaluations,
                    "signals_generated": m.signals_generated,
                    "signals_suppressed": m.signals_suppressed,
                    "errors": m.errors,
                    "avg_eval_ns": m.avg_eval_ns(),
                    "signal_rate": m.signal_rate(),
                    "cache_hit_rate": m.cache_hit_rate(),
                });
                GetMetricsResponse {
                    metrics: Some(val),
                    found: true,
                }
            }
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;

    use crate::core::registry::{StrategyFactory, StrategyRegistry};
    use crate::core::r#trait::{Signal, Strategy, StrategyContext, StrategyError};
    use crate::trading::paper::{PaperAdapter, PaperConfig};
    use domain::{InstrumentId, Side, Venue};

    // A simple strategy for tests
    struct AlwaysBuyFixed;
    impl Strategy for AlwaysBuyFixed {
        fn evaluate(&self, ctx: &StrategyContext) -> Result<Option<Signal>, StrategyError> {
            Ok(Some(Signal::new(
                ctx.instrument.clone(),
                Side::Buy,
                1.0,
                Some(100.0),
                ctx.ts_ms,
                "fixed_buy".into(),
                HashMap::new(),
            )))
        }
        fn name(&self) -> &str {
            "always_buy_fixed"
        }
    }

    fn make_services() -> (
        StrategyManagementService,
        Arc<Mutex<StrategyRegistry>>,
        Arc<StrategyFactory>,
    ) {
        let mut factory = StrategyFactory::new();
        factory.register_builder("always_buy_fixed".into(), |_| Ok(Arc::new(AlwaysBuyFixed)));
        let factory = Arc::new(factory);
        let registry = Arc::new(Mutex::new(StrategyRegistry::new()));
        let svc = StrategyManagementService::new(Arc::clone(&registry), Arc::clone(&factory));
        (svc, registry, factory)
    }

    #[test]
    fn register_strategy() {
        let (svc, _, _) = make_services();
        let resp = svc.register(RegisterStrategyRequest {
            id: "s1".into(),
            strategy_type: "always_buy_fixed".into(),
            config: serde_json::json!({}),
        });
        assert!(resp.success, "expected success: {}", resp.message);
    }

    #[test]
    fn register_unknown_type_fails() {
        let (svc, _, _) = make_services();
        let resp = svc.register(RegisterStrategyRequest {
            id: "s1".into(),
            strategy_type: "unknown_type".into(),
            config: serde_json::json!({}),
        });
        assert!(!resp.success);
    }

    #[test]
    fn evaluate_returns_signal() {
        let (svc, _, _) = make_services();
        svc.register(RegisterStrategyRequest {
            id: "s1".into(),
            strategy_type: "always_buy_fixed".into(),
            config: serde_json::json!({}),
        });
        let resp = svc.evaluate(EvaluateRequest {
            strategy_id: "s1".into(),
            instrument: "BTC".into(),
            ts_ms: 1000,
            last_bar_close: Some(99.0),
        });
        assert!(resp.error.is_none(), "unexpected error: {:?}", resp.error);
        assert!(resp.signal.is_some());
    }

    #[test]
    fn evaluate_unknown_strategy_errors() {
        let (svc, _, _) = make_services();
        let resp = svc.evaluate(EvaluateRequest {
            strategy_id: "ghost".into(),
            instrument: "BTC".into(),
            ts_ms: 0,
            last_bar_close: None,
        });
        assert!(resp.error.is_some());
    }

    #[test]
    fn paper_service_submit_order() {
        let adapter = Arc::new(Mutex::new(PaperAdapter::new(PaperConfig::default())));
        let svc = PaperTradingService::new(Arc::clone(&adapter));

        let resp = svc.submit_order(
            SubmitOrderRequest {
                instrument: "BTC".into(),
                side: "buy".into(),
                quantity: 1.0,
                limit_price: None,
            },
            1000,
        );
        assert_eq!(resp.order_id, 1);
        assert_eq!(resp.status, "pending");

        let state = svc.get_state();
        assert_eq!(state["pending_orders"], 1);
    }

    #[test]
    fn metrics_service_not_found() {
        let registry = Arc::new(MetricsRegistry::new());
        let svc = MetricsService::new(registry);
        let resp = svc.get_metrics(GetMetricsRequest {
            strategy_id: "ghost".into(),
        });
        assert!(!resp.found);
        assert!(resp.metrics.is_none());
    }

    #[test]
    fn metrics_service_found() {
        use std::time::Duration;
        let registry = Arc::new(MetricsRegistry::new());
        registry.record_evaluation("s1", Duration::from_nanos(100), true, false);
        let svc = MetricsService::new(Arc::clone(&registry));
        let resp = svc.get_metrics(GetMetricsRequest {
            strategy_id: "s1".into(),
        });
        assert!(resp.found);
        let m = resp.metrics.unwrap();
        assert_eq!(m["evaluations"], 1);
    }
}
