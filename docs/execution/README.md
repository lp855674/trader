# Execution System Documentation

## Architecture

The `exec` crate provides a full order execution pipeline for the trader platform.

### Core Modules

| Module | Purpose |
|--------|---------|
| `core/` | Order FSM, position tracking, type definitions |
| `quality/` | Slippage, commission, market impact, cost models |
| `orders/` | Stop orders, iceberg orders, TWAP/VWAP algorithms |
| `queue/` | Batch execution, priority queues, order routing |
| `monitor/` | Metrics, alerts, distributed tracing, PnL |
| `persistence/` | WAL, snapshots, order/fill/position repositories |
| `adapters/` | Longbridge broker adapter |
| `api/` | gRPC, REST, WebSocket, health endpoints |
| `config/` | Configuration schema with validation |
| `system/` | System integration and graceful shutdown |

## API Reference

### REST Endpoints
- `POST /orders` — Submit a new order
- `GET /orders/{id}` — Get order status
- `GET /health/live` — Liveness probe
- `GET /health/ready` — Readiness probe

### gRPC Services
- `ExecutionService/SubmitOrder` — Submit order via gRPC
- `ExecutionService/GetOrderStatus` — Query order status

### WebSocket
- Connect to `/ws/events` for real-time order updates

## Configuration

```json
{
  "execution": {
    "max_order_size": 100000.0,
    "max_position_pct": 0.10,
    "default_slippage_bps": 5.0
  },
  "broker": {
    "venue": "paper",
    "api_url": "http://localhost:8080",
    "timeout_ms": 5000,
    "max_connections": 10
  },
  "risk_limits": {
    "max_drawdown_pct": 0.20,
    "max_daily_loss": 50000.0,
    "max_leverage": 2.0
  },
  "monitoring": {
    "metrics_interval_secs": 30,
    "alert_threshold_ms": 500,
    "enable_tracing": true
  }
}
```

## Integration Guide

### Paper Trading
```rust
use exec::{PaperAdapter, ExecutionRouter};

let db = Db::connect("sqlite::memory:").await?;
let adapter = Arc::new(PaperAdapter::new(db));
let router = ExecutionRouter::new(routes);
```

### Production (Longbridge)
```rust
use exec::adapters::LongbridgeAdapter;
// Configure via ExecConfig from JSON/env
let cfg = ExecConfig::default();
let sys = SystemIntegration::new(cfg);
sys.validate_config()?;
```

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| Order rejected | Risk limit breach | Check `RiskLimits` in config |
| High slippage | Low liquidity | Use TWAP/VWAP algorithm |
| WAL corruption | Crash during write | Run WAL replay on startup |
| Alert storm | Threshold too low | Increase `alert_threshold_ms` |
