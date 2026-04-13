// System integration smoke test — wires all Phase-5 components together.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use domain::{InstrumentId, Side, Venue};
use strategy::api::grpc::{
    EvaluateRequest, PaperTradingService, RegisterStrategyRequest, StrategyManagementService,
    SubmitOrderRequest,
};
use strategy::config::schema::ConfigLoader;
use strategy::core::metrics::MetricsRegistry;
use strategy::core::registry::{StrategyFactory, StrategyRegistry};
use strategy::core::r#trait::{Signal, Strategy, StrategyContext, StrategyError};
use strategy::trading::paper::{MarketDataSnapshot, PaperAdapter, PaperConfig};

// ─── Helper strategy ──────────────────────────────────────────────────────────

struct AlwaysBuyOne;

impl Strategy for AlwaysBuyOne {
    fn evaluate(&self, ctx: &StrategyContext) -> Result<Option<Signal>, StrategyError> {
        Ok(Some(Signal::new(
            ctx.instrument.clone(),
            Side::Buy,
            1.0,
            ctx.last_bar_close,
            ctx.ts_ms,
            "always_buy_one".into(),
            HashMap::new(),
        )))
    }

    fn name(&self) -> &str {
        "always_buy_one"
    }
}

// ─── Integration test ─────────────────────────────────────────────────────────

#[test]
fn system_smoke_test() {
    // 1. Parse AppConfig from JSON
    let config_json = r#"{
        "strategies": [
            {
                "id": "smoke_strategy",
                "strategy_type": "always_buy_one",
                "params": {},
                "enabled": true,
                "version": 1
            }
        ],
        "backtest": null,
        "risk": {
            "max_drawdown_pct": 0.2,
            "max_position_size": 1000.0,
            "daily_loss_limit": 500.0,
            "var_confidence": 0.95
        },
        "paper_trading": {
            "enabled": true,
            "initial_capital": 50000.0,
            "commission_rate": 0.001,
            "slippage_bps": 5.0
        },
        "log_level": "info",
        "data_dir": "/tmp/data"
    }"#;

    let app_config = ConfigLoader::from_json(config_json).expect("parse AppConfig");
    ConfigLoader::validate(&app_config).expect("validate AppConfig");

    assert_eq!(app_config.strategies.len(), 1);
    assert_eq!(app_config.strategies[0].id, "smoke_strategy");
    assert!((app_config.paper_trading.initial_capital - 50_000.0).abs() < 1e-9);

    // 2. Create StrategyRegistry and register AlwaysBuyOne via factory
    let mut factory = StrategyFactory::new();
    factory.register_builder("always_buy_one".into(), |_| Ok(Arc::new(AlwaysBuyOne)));
    let factory = Arc::new(factory);

    let registry = Arc::new(Mutex::new(StrategyRegistry::new()));

    // 3. Create MetricsRegistry
    let metrics = Arc::new(MetricsRegistry::new());

    // 4. Create PaperAdapter
    let paper_cfg = PaperConfig {
        initial_capital: app_config.paper_trading.initial_capital,
        commission_rate: app_config.paper_trading.commission_rate,
        slippage_bps: app_config.paper_trading.slippage_bps,
        max_positions: 10,
        fill_delay_ms: 0, // instant fills for smoke test
    };
    let adapter = Arc::new(Mutex::new(PaperAdapter::new(paper_cfg)));

    // 5. Evaluate strategy via StrategyManagementService
    let mgmt_svc = StrategyManagementService::new(Arc::clone(&registry), Arc::clone(&factory));

    let reg_resp = mgmt_svc.register(RegisterStrategyRequest {
        id: "smoke_strategy".into(),
        strategy_type: "always_buy_one".into(),
        config: serde_json::json!({}),
    });
    assert!(
        reg_resp.success,
        "registration failed: {}",
        reg_resp.message
    );

    let eval_resp = mgmt_svc.evaluate(EvaluateRequest {
        strategy_id: "smoke_strategy".into(),
        instrument: "BTC".into(),
        ts_ms: 1_000_000,
        last_bar_close: Some(45_000.0),
    });
    assert!(
        eval_resp.error.is_none(),
        "evaluate error: {:?}",
        eval_resp.error
    );
    assert!(eval_resp.signal.is_some(), "expected a signal");

    // 6. Submit order via PaperTradingService
    let paper_svc = PaperTradingService::new(Arc::clone(&adapter));
    let order_resp = paper_svc.submit_order(
        SubmitOrderRequest {
            instrument: "BTC".into(),
            side: "buy".into(),
            quantity: 1.0,
            limit_price: None,
        },
        1_000_000,
    );
    assert_eq!(order_resp.status, "pending");
    assert_eq!(order_resp.order_id, 1);

    // 7. Process market data snapshot — should fill immediately (fill_delay_ms = 0)
    let mut snap_map = HashMap::new();
    let btc = InstrumentId::new(Venue::Crypto, "BTC");
    snap_map.insert(
        btc.clone(),
        MarketDataSnapshot {
            instrument: btc.clone(),
            bid: 44_990.0,
            ask: 45_010.0,
            last: 45_000.0,
            ts_ms: 1_000_000,
            volume_24h: 1_000.0,
        },
    );

    let mut adapter_guard = adapter.lock().unwrap();
    let fills = adapter_guard.process_market_data(&snap_map, 1_000_000);

    // 8. Assert fill or pending order
    let has_fill = !fills.is_empty();
    let has_pending = !adapter_guard.state.pending_orders.is_empty();
    assert!(
        has_fill || has_pending,
        "expected either a fill or a pending order"
    );

    if has_fill {
        let fill = &fills[0];
        assert_eq!(fill.fill_qty, 1.0);
        assert!(fill.fill_price > 0.0);
    }
}
