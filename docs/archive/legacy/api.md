# Trader API Design

Version: v1.0  
Status: Draft  
Service: trader-server  
Protocol: REST + WebSocket  
Target Markets: A股 / 港股 / 美股 / 数字货币  

---

# 0. 当前 V1 本地实现状态

当前代码实现的是 V1 local-verifiable API surface，不是生产实盘 API。

已实现并由 `scripts/v1-smoke.ps1` 覆盖：

```text
GET  /api/v1/health
POST /api/v1/preflight/paper
POST /api/v1/backtests
POST /api/v1/paper-runs
POST /api/v1/replays
POST /api/v1/live-runs
GET  /api/v1/live-runs/{run_id}/status
POST /api/v1/live-runs/{run_id}/stop
GET  /api/v1/brokers/status
GET  /api/v1/brokers/account/{account_id}?broker={broker}
GET  /api/v1/fee-rules
POST /api/v1/fee-rules
GET  /api/v1/runs/{run_id}/orders
GET  /api/v1/runs/{run_id}/fills
GET  /api/v1/runs/{run_id}/positions
GET  /api/v1/funding-rates
GET  /api/v1/crypto-market-meta
GET  /api/v1/corporate-actions
GET  /api/v1/ingestion/status
GET  /api/v1/runs/{run_id}/account-balances
GET  /api/v1/runs/{run_id}/portfolio-snapshots
GET  /api/v1/runs/{run_id}/cash-snapshots
GET  /api/v1/runs/{run_id}/position-snapshots
GET  /api/v1/runs/{run_id}/metrics
GET  /api/v1/runs
GET  /api/v1/runs/{run_id}
GET  /api/v1/configs
POST /api/v1/configs
GET  /api/v1/configs/{name}
GET  /api/v1/configs/{name}/latest
GET  /api/v1/configs/{name}/published
GET  /api/v1/configs/{name}/{version}
PUT  /api/v1/configs/{name}/{version}/state
GET  /api/v1/configs/{name}/diff
POST /api/v1/configs/{name}/{version}/rollback
GET  /api/v1/configs/{config_id}/releases
GET  /api/v1/configs/{config_id}/audits
GET  /api/v1/runs/{run_id}/config-version
GET  /api/v1/logs
GET  /api/v1/system-logs
GET  /api/v1/ops/logging/metrics
GET  /api/v1/runs/{run_id}/status
POST /api/v1/runs/{run_id}/cancel
GET  /api/v1/events
GET  /api/v1/runs/{run_id}/events
GET  /api/v1/runs/{run_id}/order-events
GET  /api/v1/runs/{run_id}/risk-events
GET  /api/v1/runs/{run_id}/insights
GET  /api/v1/runs/{run_id}/cash-snapshots
GET  /api/v1/runs/{run_id}/position-snapshots
GET  /api/v1/runs/{run_id}/reconciliation
GET  /api/v1/reconciliation-drifts
GET  /api/v1/runs/{run_id}/reconciliation-drifts
GET  /api/v1/reconciliation-alerts/summary
GET  /api/v1/runs/{run_id}/reconciliation-alerts/summary
GET  /api/v1/reconciliation-alert-deliveries/summary
GET  /api/v1/runs/{run_id}/reconciliation-alert-deliveries/summary
GET  /api/v1/runs/{run_id}/portfolio-targets
GET  /api/v1/runs/{run_id}/system-logs
GET  /api/v1/runs/{run_id}/crypto-positions
POST /api/v1/replay/{run_id}/pause
POST /api/v1/replay/{run_id}/resume
POST /api/v1/replay/{run_id}/seek/{offset}
POST /api/v1/replay/{run_id}/speed/{speed}
GET  /ws
```

Run launch endpoints (`POST /api/v1/backtests`, `/paper-runs`, `/replays`, `/live-runs`) require an explicit run config in the JSON body. Provide exactly one of `config_toml`, `config_ref`, or `config`; optional `mode`, `run_id`, `strategy_ref`, and `strategy` only override the supplied config and are not config sources. `strategy_ref` references a stored config lifecycle version whose content is a strategy template, replaces the supplied config's `strategy`, and records `_provenance.strategy_ref` in the final snapshot. The optional inline `strategy` patch is applied after `strategy_ref` and currently supports `name`, `symbols`, `fast_window`, and `slow_window`; the persisted run config snapshot and `GET /api/v1/runs/{run_id}` response expose the final config after overrides.

Live runs started through `POST /api/v1/live-runs` are process-isolated internally. The public request and response shape is unchanged; active status may come from supervisor process state, while terminal status falls back to the SQLite run record written by the worker runtime.

REST event query responses use an API-owned response model. `payload` is returned as structured JSON, not as a double-encoded JSON string:

`GET /api/v1/runs/{run_id}/order-events`, `GET /api/v1/runs/{run_id}/risk-events`, `GET /api/v1/runs/{run_id}/insights`, and `GET /api/v1/runs/{run_id}/portfolio-targets` are read-only projection queries derived from `event_store`. They do not replace `event_store` as the immutable audit truth and do not provide any manual trading command path. `order-events` supports `order_id`, `client_order_id`, `broker_order_id`, `account_id`, `symbol`, `status`, `event_type`, `from_ms`, `to_ms`, and `limit` filters for startup recovery / order lifecycle troubleshooting. `risk-events` supports `risk_type`, `decision`, `account_id`, `symbol`, `from_ms`, `to_ms`, and `limit` filters for pre-trade rejection and reconciliation drift audit.

`GET /api/v1/runs/{run_id}/crypto-positions` and `GET /api/v1/funding-rates` are read-only queries over the contract storage boundary. Decimal values are returned as strings. Paper runtime now writes simulated contract position lifecycle and funding settlement state to `crypto_positions`; funding-rate rows are exposed from the `funding_rates` storage boundary.

`POST /api/v1/fee-rules` creates a storage-backed fee rule with optional tiers. `GET /api/v1/fee-rules` queries the effective rule for `market`, `exchange`, `asset_class`, optional `symbol`, and optional `at_ms`. `volume_window` is returned on every rule and accepts `run`, `rolling_30d`, or `calendar_month`; omitted create requests default to `run`. Runtime fee tiers use rule-level volume: `run` starts with zero historical fee volume, `rolling_30d` seeds from prior persisted fills in the last 30 days and evicts fills as they leave the runtime window, and `calendar_month` seeds from the current UTC month and resets across UTC month boundaries.

Run-owned read models should be queried through explicit run-scoped routes:

- `GET /api/v1/runs/{run_id}/orders`
- `GET /api/v1/runs/{run_id}/fills`
- `GET /api/v1/runs/{run_id}/positions`
- `GET /api/v1/runs/{run_id}/account-balances`
- `GET /api/v1/runs/{run_id}/portfolio-snapshots`
- `GET /api/v1/runs/{run_id}/cash-snapshots`
- `GET /api/v1/runs/{run_id}/position-snapshots`
- `GET /api/v1/runs/{run_id}/metrics`

Legacy top-level endpoints such as `GET /api/v1/orders?run_id={run_id}`, `GET /api/v1/fills?run_id={run_id}`, `GET /api/v1/positions?run_id={run_id}`, `GET /api/v1/account-balances?run_id={run_id}`, `GET /api/v1/portfolio/snapshots?run_id={run_id}`, `GET /api/v1/cash/snapshots?run_id={run_id}`, `GET /api/v1/positions/snapshots?run_id={run_id}`, and `GET /api/v1/metrics?run_id={run_id}` still exist for compatibility. They require explicit run scope and no longer resolve run ownership through `[run_defaults].config_path`. New integrations should use the explicit `GET /api/v1/runs/{run_id}/...` routes.

`GET /api/v1/crypto-market-meta` and `GET /api/v1/corporate-actions` are read-only queries over reference-data storage boundaries. Reference-data ingestion can populate Binance market metadata, Binance funding rates, and Yahoo corporate actions through the CLI/scheduled ingestion layer. `GET /api/v1/ingestion/status` reports the latest ingestion tracker entries recorded in `system_logs`. `GET /api/v1/logs` exposes paginated runtime/system logs with `run_id`, `level`, `target`, `from_ms`, `to_ms`, `search`, `limit`, and `offset` filters and returns `{ logs, total, limit, offset }`. `GET /api/v1/system-logs` and `GET /api/v1/runs/{run_id}/system-logs` remain available for direct list readback. `GET /api/v1/ops/logging/metrics` exposes in-process writer dropped-log metrics plus active `[logging]` writer settings; CLI also supports retention purge, `logs count` / `logs tail` / `logs metrics`, JSONL export, and `logs ship` for HTTP NDJSON collector handoff. `trader-server` runs a background retention scheduler using `[logging].retention_days`; CLI/API run launch paths also perform startup cleanup. `logs ship` accepts optional `--max-retries`, `--retry-backoff-ms`, and `--signature-secret-env`; network errors, HTTP 429, and HTTP 5xx are retried with linearly increasing backoff, while non-retryable 4xx statuses fail immediately. When signing is enabled, requests include `X-Trader-Log-Timestamp` and `X-Trader-Log-Signature: v1=<hmac-sha256>`, signing `timestamp.body`.

`GET /api/v1/runs/{run_id}/cash-snapshots` and `GET /api/v1/runs/{run_id}/position-snapshots` query paper/live reconciliation snapshot storage by explicit run id. They support optional time and symbol/currency filters. Decimal values are returned as strings. Live runs write a baseline cash snapshot at startup and can periodically capture fake broker cash/position snapshots when `[live].broker_snapshot_interval_ms` is configured. Startup recovery also reads broker open orders and executions for any local recoverable orders before the main loop begins. The default policy is `[live.startup_recovery] unmatched_open_orders = "fail"`, which marks the run failed when the broker reports an unknown remote open order; set `warn_only` only when the operator explicitly accepts that degraded recovery path. Cash drift and broker position missing/quantity drift against the latest runtime snapshots are projected as `reconciliation_drift` risk events; when `[live.alerts]` is enabled, downstream alert routing supports legacy single-sink fields (`sink = "file"` with `file_path`, or `sink = "webhook"` with `webhook_url`) and multi-sink `[[live.alerts.sinks]]` entries so file JSONL append and webhook JSON POST delivery can run together. Optional `cooldown_ms` suppresses repeated downstream sends for the same alert dedup key within the cooldown window; sink-level values override `[live.alerts]` defaults. Webhook sinks also support `webhook_timeout_ms`, `webhook_max_retries`, and `webhook_auth_token` for a bearer-authenticated local MVP delivery policy; `system_logs` and drift audit surfaces still record every alert. Real broker-reported cash/position scheduling remains production hardening work.

`GET /api/v1/runs/{run_id}/reconciliation` summarizes persisted cash snapshots, position snapshots, and `risk_events` with `risk_type = "reconciliation_drift"` for the run. `GET /api/v1/reconciliation-drifts` and `GET /api/v1/runs/{run_id}/reconciliation-drifts` provide drift-audit readback with `run_id` / `account_id` / `symbol` / `from_ms` / `to_ms` / `limit` filters. `GET /api/v1/reconciliation-alerts/summary` and `GET /api/v1/runs/{run_id}/reconciliation-alerts/summary` aggregate `runtime.alert` log records for persisted reconciliation alerts. `GET /api/v1/reconciliation-alert-deliveries/summary` and `GET /api/v1/runs/{run_id}/reconciliation-alert-deliveries/summary` aggregate `runtime.alert_delivery` records for downstream delivery status by sink and outcome.

`GET /api/v1/ingestion/status` response:

```json
{
  "sources": [
    {
      "name": "binance",
      "table": "funding_rates",
      "ts_ms": 1700000000000,
      "rows_fetched": 3,
      "rows_upserted": 2,
      "duration_ms": 25
    }
  ]
}
```

```json
[
  {
    "event_id": "01890f0e-d8b1-7cc6-94f4-8f9f0f7f0a11",
    "ts_ms": 1700000000000,
    "source": "sample-ma-cross",
    "category": "algorithm.alpha.generated",
    "payload": {
      "run_id": "sample-ma-cross",
      "symbol": "AAPL"
    }
  }
]
```

WebSocket 当前支持：

```text
subscribe       -> replay persisted events for run_id
replay_control  -> pause / resume / seek / speed
```

Broker status 返回 Futu、Binance、OKX、Interactive Brokers 的 deterministic fake adapters。Live runtime 验证本地 lifecycle、broker status、stop，以及可配置的 fake broker cash/position snapshot 调度；不连接真实 broker、不接收凭证、不发真实订单。仅用于本地 smoke 的 fake adapter 还支持 `[broker] fake_startup_unmatched_open_order = true`，用于触发 startup recovery 未匹配远端 open order 分支并验证默认 `fail` / 显式 `warn_only` 策略。

---

# 1. 设计目标

Trader API 负责对外提供服务端控制与实时状态推送能力。

Trader 是服务端项目，不包含 Dashboard 前端。

API 分为两类：

```text
REST API
  ↓
查询、启动、停止、配置、控制类操作

WebSocket API
  ↓
实时行情、订单、成交、持仓、账户、PnL、风控、Replay 推送与控制
```

API 的设计目标：

```text
支持策略启动 / 停止 / 参数更新
支持 Backtest / Replay / Paper / Live 控制
支持订单、成交、持仓、账户、绩效查询
支持 WebSocket 实时推送
支持 Replay pause / resume / seek / speed
支持数字货币账户、仓位、资金费率、保证金状态推送
支持统一错误格式
支持未来接入 Dashboard 或第三方控制端
```

---

# 2. API 分层

```text
trader-server
  ↓
trader-api crate
  ↓
REST Router
WebSocket Router
Command Handler
Query Handler
Event Broadcaster
Runtime Manager
```

---

# 3. 服务端职责

`trader-server` 负责：

```text
启动 HTTP Server
启动 WebSocket Server
加载配置
初始化 SQLite
初始化 Event Bus
初始化 Runtime Manager
初始化 Broker Adapter
初始化 Market Data Adapter
处理 REST 请求
处理 WebSocket 连接
向客户端推送事件
```

不负责：

```text
前端页面
复杂图表渲染
直接暴露数据库
绕过 OMS 下单
绕过 Risk 控制
```

---

# 4. 基础信息

## 4.1 默认地址

```text
Host: 127.0.0.1
Port: 8080
Base URL: http://127.0.0.1:8080
WebSocket URL: ws://127.0.0.1:8080/ws
```

---

## 4.2 API Prefix

```text
/api/v1
```

示例：

```text
GET  /api/v1/health
POST /api/v1/strategies/start
GET  /api/v1/runs/{run_id}/orders
```

---

## 4.3 Content-Type

REST API 使用：

```http
Content-Type: application/json
Accept: application/json
```

WebSocket 使用 JSON 消息。

---

# 5. 通用响应格式

## 5.1 成功响应

```json
{
  "success": true,
  "data": {},
  "error": null,
  "request_id": "req_001",
  "ts": 1700000000000
}
```

---

## 5.2 错误响应

```json
{
  "success": false,
  "data": null,
  "error": {
    "code": "INVALID_ARGUMENT",
    "message": "invalid market",
    "details": {}
  },
  "request_id": "req_001",
  "ts": 1700000000000
}
```

---

## 5.3 分页响应

```json
{
  "success": true,
  "data": {
    "items": [],
    "page": 1,
    "page_size": 50,
    "total": 120,
    "has_next": true
  },
  "error": null,
  "request_id": "req_001",
  "ts": 1700000000000
}
```

---

# 6. 通用枚举

## 6.1 Market

```text
CN
HK
US
CRYPTO
MIXED
```

---

## 6.2 AssetClass

```text
EQUITY
CRYPTO_SPOT
CRYPTO_PERP
CRYPTO_FUTURE
```

---

## 6.3 Runtime Mode

```text
BACKTEST
REPLAY
PAPER
LIVE
```

---

## 6.4 RunStatus

```text
CREATED
RUNNING
PAUSED
FINISHED
FAILED
STOPPED
```

---

## 6.5 OrderStatus

```text
NEW
PENDING_SUBMIT
SUBMITTED
PARTIALLY_FILLED
FILLED
PENDING_CANCEL
CANCELLED
REJECTED
EXPIRED
UNKNOWN
SYNCING
```

---

## 6.6 Side

```text
BUY
SELL
```

---

## 6.7 OrderType

```text
MARKET
LIMIT
STOP
STOP_LIMIT
POST_ONLY
```

---

## 6.8 TimeInForce

```text
DAY
GTC
IOC
FOK
```

---

## 6.9 PositionSide

```text
LONG
SHORT
NET
```

---

## 6.10 MarginMode

```text
CROSS
ISOLATED
```

---

# 7. REST API 总览

```text
System
  GET  /api/v1/health
  GET  /api/v1/version

Runtime
  GET  /api/v1/runs
  GET  /api/v1/runs/{run_id}
  GET  /api/v1/runs/{run_id}/system-logs
  GET  /api/v1/runs/{run_id}/crypto-positions
  POST /api/v1/runs/{run_id}/stop

Strategy
  POST /api/v1/strategies/start
  POST /api/v1/strategies/{strategy_id}/stop
  POST /api/v1/strategies/{strategy_id}/pause
  POST /api/v1/strategies/{strategy_id}/resume
  POST /api/v1/strategies/{strategy_id}/params
  GET  /api/v1/strategies
  GET  /api/v1/strategies/{strategy_id}

Backtest
  POST /api/v1/backtests
  GET  /api/v1/runs/{run_id}
  GET  /api/v1/runs/{run_id}/metrics

Replay
  POST /api/v1/replays
  POST /api/v1/replay/{run_id}/pause
  POST /api/v1/replay/{run_id}/resume
  POST /api/v1/replay/{run_id}/seek/{offset}
  POST /api/v1/replay/{run_id}/speed/{speed}

Launch POST requests must include `Content-Type: application/json` and exactly one config source:

```json
{
  "config_toml": "[runtime]\nmode = \"paper\"\n...",
  "mode": "paper",
  "run_id": "paper-run-001",
  "strategy_ref": {
    "name": "ma-cross-template",
    "version": 1,
    "published": false
  },
  "strategy": {
    "name": "moving_average_cross",
    "symbols": ["US:NASDAQ:AAPL:EQUITY"],
    "fast_window": 2,
    "slow_window": 3
  }
}
```

`config_ref` can reference a stored full run config version, and `config` can inline a JSON config object. `strategy_ref` can reference a stored strategy template version. Strategy assembly is applied in this order: explicit run config source, `strategy_ref`, then inline `strategy`. `mode` and `run_id` are independent runtime overrides.

Orders
  GET  /api/v1/runs/{run_id}/orders

Fills
  GET  /api/v1/runs/{run_id}/fills

Positions
  GET  /api/v1/runs/{run_id}/positions
  GET  /api/v1/runs/{run_id}/crypto-positions

Accounts
  GET  /api/v1/runs/{run_id}/account-balances
  GET  /api/v1/runs/{run_id}/cash-snapshots

Portfolio
  GET  /api/v1/runs/{run_id}/portfolio-snapshots
  GET  /api/v1/runs/{run_id}/position-snapshots

Metrics
  GET  /api/v1/runs/{run_id}/metrics

Risk
  GET  /api/v1/risk-events

Market Data
  GET  /api/v1/instruments
  GET  /api/v1/instruments/{market}/{exchange}/{symbol}
  GET  /api/v1/candles
  GET  /api/v1/ticks
  GET  /api/v1/funding-rates
  GET  /api/v1/crypto-market-meta
  GET  /api/v1/corporate-actions
  GET  /api/v1/open-interest

Config
  GET  /api/v1/configs
  POST /api/v1/configs
  GET  /api/v1/configs/{name}
  GET  /api/v1/configs/{name}/latest
  GET  /api/v1/configs/{name}/published
  GET  /api/v1/configs/{name}/{version}
  PUT  /api/v1/configs/{name}/{version}/state
  GET  /api/v1/configs/{name}/diff
  POST /api/v1/configs/{name}/{version}/rollback
  GET  /api/v1/configs/{config_id}/releases
  GET  /api/v1/configs/{config_id}/audits
  GET  /api/v1/runs/{run_id}/config-version
```

---

# 8. System API

---

## 8.1 Health Check

```http
GET /api/v1/health
```

响应：

```json
{
  "success": true,
  "data": {
    "status": "ok",
    "server_time": 1700000000000,
    "database": "ok",
    "event_bus": "ok"
  },
  "error": null,
  "request_id": "req_001",
  "ts": 1700000000000
}
```

---

## 8.2 Version

```http
GET /api/v1/version
```

响应：

```json
{
  "success": true,
  "data": {
    "name": "Trader",
    "version": "0.1.0",
    "git_commit": "abc123",
    "build_time": "2026-01-01T00:00:00Z"
  },
  "error": null,
  "request_id": "req_001",
  "ts": 1700000000000
}
```

---

# 9. Runtime API

---

## 9.1 查询运行列表

```http
GET /api/v1/runs
```

Query 参数：

| 参数        | 类型      | 说明                                              |
| --------- | ------- | ----------------------------------------------- |
| mode      | string  | BACKTEST / REPLAY / PAPER / LIVE                |
| market    | string  | CN / HK / US / CRYPTO / MIXED                   |
| status    | string  | CREATED / RUNNING / FINISHED / FAILED / STOPPED |
| page      | integer | 页码                                              |
| page_size | integer | 每页数量                                            |

示例：

```http
GET /api/v1/runs?mode=BACKTEST&page=1&page_size=50
```

响应：

```json
{
  "success": true,
  "data": {
    "items": [
      {
        "id": "run_001",
        "name": "ma_cross_btc",
        "strategy_name": "ma_cross",
        "mode": "BACKTEST",
        "market": "CRYPTO",
        "status": "FINISHED",
        "started_at": 1700000000000,
        "ended_at": 1700003600000,
        "initial_cash": "10000",
        "final_equity": "10820.5",
        "base_currency": "USDT"
      }
    ],
    "page": 1,
    "page_size": 50,
    "total": 1,
    "has_next": false
  },
  "error": null,
  "request_id": "req_001",
  "ts": 1700000000000
}
```

---

## 9.2 查询单个运行

```http
GET /api/v1/runs/{run_id}
```

响应：

```json
{
  "success": true,
  "data": {
    "id": "run_001",
    "name": "ma_cross_btc",
    "strategy_name": "ma_cross",
    "mode": "BACKTEST",
    "market": "CRYPTO",
    "status": "FINISHED",
    "started_at": 1700000000000,
    "ended_at": 1700003600000,
    "initial_cash": "10000",
    "final_cash": "10200",
    "final_equity": "10820.5",
    "base_currency": "USDT",
    "config": {},
    "params_json": {},
    "git_commit": "abc123",
    "engine_version": "0.1.0"
  },
  "error": null,
  "request_id": "req_001",
  "ts": 1700000000000
}
```

---

## 9.3 查询运行系统日志

```http
GET /api/v1/runs/{run_id}/system-logs
GET /api/v1/system-logs
```

当前 V1 返回 API 启动 Backtest、Paper、Replay、Live 时写入的运行生命周期日志，以及 ingestion/runtime/system 组件写入的结构化日志。两个端点都支持 `level`、`target`、`from_ms`、`to_ms`、`limit` 过滤；全局端点还支持 `run_id`。`fields` 是结构化 JSON，不返回 `fields_json` 字符串：

```json
[
  {
    "id": "3e68fef2-3c54-4e0e-8f22-1ad1f901f000",
    "run_id": "sample-ma-cross",
    "ts_ms": 1700000000000,
    "level": "INFO",
    "target": "api.run",
    "message": "paper run completed",
    "fields": {
      "mode": "paper",
      "status": "completed",
      "signals": 1,
      "orders": 1
    },
    "created_at_ms": 1700000000000
  }
]
```

---

## 9.4 查询订单事件投影

```http
GET /api/v1/runs/{run_id}/order-events
```

当前 V1 返回由 `broker.order.*` 与 `algorithm.oms.*` 运行事件派生的只读订单审计投影。`payload` 是结构化 JSON，不返回 `payload_json` 字符串。该接口支持以下 Query 参数：

| 参数 | 类型 | 说明 |
| --- | --- | --- |
| order_id | string | 过滤本地订单 ID |
| client_order_id | string | 过滤幂等 client order id |
| broker_order_id | string | 过滤远端 broker order id |
| account_id | string | 过滤账户 |
| symbol | string | 过滤标的 |
| status | string | 过滤投影状态，例如 `SUBMITTED` / `FILLED` |
| event_type | string | 过滤事件类型，例如 `broker.order.recovered` |
| from_ms | integer | 起始时间 |
| to_ms | integer | 结束时间 |
| limit | integer | 返回条数上限 |

典型用途：

- 查 live 启动恢复写入的 `broker.order.recovered`
- 按 `client_order_id` 或 `broker_order_id` 追单
- 定位 `broker.order.failed` / `algorithm.oms.*` 分支

响应示例：

```json
[
  {
    "id": "order_event_001",
    "event_id": "event_001",
    "run_id": "live-startup-recovery",
    "order_id": "order-recover",
    "client_order_id": "client-recover",
    "broker_order_id": "broker-recover",
    "account_id": "paper",
    "symbol": "US:NASDAQ:AAPL:EQUITY",
    "status": "FILLED",
    "event_type": "broker.order.recovered",
    "message": "startup recovery matched broker order state",
    "ts_ms": 1700000000000,
    "payload": {
      "run_id": "live-startup-recovery",
      "order_id": "order-recover",
      "client_order_id": "client-recover",
      "broker_order_id": "broker-recover",
      "status": "FILLED",
      "executions": 1,
      "recovery_source": "startup"
    }
  }
]
```

---

## 9.5 查询风控事件投影

```http
GET /api/v1/runs/{run_id}/risk-events
```

当前 V1 返回由 `algorithm.risk.*` 运行事件派生的只读风控审计投影。`payload` 是结构化 JSON，不返回 `payload_json` 字符串。该接口支持以下 Query 参数：

| 参数 | 类型 | 说明 |
| --- | --- | --- |
| risk_type | string | 过滤风险类型，例如 `reconciliation_drift` |
| decision | string | 过滤决策，例如 `approved` / `warn` / `rejected` |
| account_id | string | 过滤账户 |
| symbol | string | 过滤标的 |
| from_ms | integer | 起始时间 |
| to_ms | integer | 结束时间 |
| limit | integer | 返回条数上限 |

典型用途：

- 查询 pre-trade 风控拒单
- 查询 live reconciliation drift 投影
- 按账户/标的回放风险决策时间线

响应示例：

```json
[
  {
    "id": "risk_event_001",
    "event_id": "event_002",
    "run_id": "live-cash-drift",
    "account_id": "live-account",
    "symbol": "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP",
    "risk_type": "reconciliation_drift",
    "decision": "warn",
    "reason": "position_qty_drift",
    "threshold": "1",
    "observed_value": "2",
    "ts_ms": 1700000000000,
    "payload": {
      "run_id": "live-cash-drift",
      "risk_type": "reconciliation_drift",
      "decision": "warn",
      "reason": "position_qty_drift",
      "threshold": "1",
      "observed_value": "2"
    }
  }
]
```

---

## 9.6 查询策略信号投影

```http
GET /api/v1/runs/{run_id}/insights
```

当前 V1 返回由 `algorithm.alpha.generated` 运行事件派生的只读策略信号投影。`payload` 是结构化 JSON，不返回 `payload_json` 字符串：

```json
[
  {
    "id": "insight_001",
    "event_id": "event_001",
    "run_id": "sample-ma-cross",
    "strategy": "moving_average_cross",
    "symbol": "US:NASDAQ:AAPL:EQUITY",
    "side": "BUY",
    "confidence": "0.75",
    "ts_ms": 1700000000000,
    "payload": {
      "run_id": "sample-ma-cross",
      "symbol": "US:NASDAQ:AAPL:EQUITY",
      "side": "BUY",
      "confidence": "0.75"
    }
  }
]
```

---

## 9.7 查询组合目标投影

```http
GET /api/v1/runs/{run_id}/portfolio-targets
```

当前 V1 返回由 `algorithm.portfolio.target` 运行事件派生的只读组合目标投影。它用于运行后分析，不是下单接口：

```json
[
  {
    "id": "target_001",
    "event_id": "event_002",
    "run_id": "sample-ma-cross",
    "account_id": "paper",
    "symbol": "US:NASDAQ:AAPL:EQUITY",
    "target_qty": "10",
    "ts_ms": 1700000000000,
    "payload": {
      "run_id": "sample-ma-cross",
      "account_id": "paper",
      "symbol": "US:NASDAQ:AAPL:EQUITY",
      "target_qty": "10"
    }
  }
]
```

---

## 9.8 停止运行

```http
POST /api/v1/runs/{run_id}/stop
```

请求：

```json
{
  "reason": "manual stop"
}
```

响应：

```json
{
  "success": true,
  "data": {
    "run_id": "run_001",
    "status": "STOPPING"
  },
  "error": null,
  "request_id": "req_001",
  "ts": 1700000000000
}
```

---

# 10. Strategy API

---

## 10.1 启动策略

```http
POST /api/v1/strategies/start
```

请求：

```json
{
  "name": "btc_ma_cross_paper",
  "strategy_name": "ma_cross",
  "mode": "PAPER",
  "market": "CRYPTO",
  "base_currency": "USDT",
  "initial_cash": "10000",
  "symbols": [
    {
      "market": "CRYPTO",
      "exchange": "BINANCE",
      "symbol": "BTCUSDT",
      "asset_class": "CRYPTO_SPOT"
    }
  ],
  "params": {
    "fast": 20,
    "slow": 60,
    "target_percent": "0.5"
  },
  "risk": {
    "max_position_percent": "0.8",
    "max_drawdown": "0.2",
    "max_order_value": "2000"
  }
}
```

响应：

```json
{
  "success": true,
  "data": {
    "run_id": "run_001",
    "strategy_id": "strategy_001",
    "status": "RUNNING"
  },
  "error": null,
  "request_id": "req_001",
  "ts": 1700000000000
}
```

---

## 10.2 停止策略

```http
POST /api/v1/strategies/{strategy_id}/stop
```

请求：

```json
{
  "reason": "manual stop",
  "cancel_open_orders": true
}
```

响应：

```json
{
  "success": true,
  "data": {
    "strategy_id": "strategy_001",
    "status": "STOPPING"
  },
  "error": null,
  "request_id": "req_001",
  "ts": 1700000000000
}
```

---

## 10.3 暂停策略

```http
POST /api/v1/strategies/{strategy_id}/pause
```

请求：

```json
{
  "reason": "manual pause"
}
```

响应：

```json
{
  "success": true,
  "data": {
    "strategy_id": "strategy_001",
    "status": "PAUSED"
  },
  "error": null,
  "request_id": "req_001",
  "ts": 1700000000000
}
```

---

## 10.4 恢复策略

```http
POST /api/v1/strategies/{strategy_id}/resume
```

响应：

```json
{
  "success": true,
  "data": {
    "strategy_id": "strategy_001",
    "status": "RUNNING"
  },
  "error": null,
  "request_id": "req_001",
  "ts": 1700000000000
}
```

---

## 10.5 更新策略参数

```http
POST /api/v1/strategies/{strategy_id}/params
```

请求：

```json
{
  "params": {
    "fast": 30,
    "slow": 90,
    "target_percent": "0.4"
  }
}
```

响应：

```json
{
  "success": true,
  "data": {
    "strategy_id": "strategy_001",
    "status": "UPDATED",
    "params": {
      "fast": 30,
      "slow": 90,
      "target_percent": "0.4"
    }
  },
  "error": null,
  "request_id": "req_001",
  "ts": 1700000000000
}
```

---

## 10.6 查询策略列表

```http
GET /api/v1/strategies
```

响应：

```json
{
  "success": true,
  "data": {
    "items": [
      {
        "strategy_id": "strategy_001",
        "run_id": "run_001",
        "name": "btc_ma_cross_paper",
        "strategy_name": "ma_cross",
        "mode": "PAPER",
        "market": "CRYPTO",
        "status": "RUNNING"
      }
    ]
  },
  "error": null,
  "request_id": "req_001",
  "ts": 1700000000000
}
```

---

# 11. Backtest API

---

## 11.1 启动回测

```http
POST /api/v1/backtests/start
```

请求：

```json
{
  "name": "aapl_ma_cross",
  "strategy_name": "ma_cross",
  "market": "US",
  "base_currency": "USD",
  "initial_cash": "100000",
  "symbols": [
    {
      "market": "US",
      "exchange": "NASDAQ",
      "symbol": "AAPL",
      "asset_class": "EQUITY"
    }
  ],
  "data": {
    "source": "parquet",
    "timeframe": "1d",
    "start": 1704067200000,
    "end": 1735689600000
  },
  "params": {
    "fast": 20,
    "slow": 60,
    "target_percent": "0.5"
  }
}
```

响应：

```json
{
  "success": true,
  "data": {
    "run_id": "run_001",
    "status": "RUNNING"
  },
  "error": null,
  "request_id": "req_001",
  "ts": 1700000000000
}
```

---

## 11.2 查询回测

```http
GET /api/v1/backtests/{run_id}
```

响应：

```json
{
  "success": true,
  "data": {
    "run_id": "run_001",
    "status": "FINISHED",
    "started_at": 1700000000000,
    "ended_at": 1700003600000,
    "initial_cash": "100000",
    "final_equity": "112300",
    "base_currency": "USD"
  },
  "error": null,
  "request_id": "req_001",
  "ts": 1700000000000
}
```

---

## 11.3 查询回测报告

```http
GET /api/v1/backtests/{run_id}/report
```

响应：

```json
{
  "success": true,
  "data": {
    "run_id": "run_001",
    "summary": {
      "total_return": "0.123",
      "annual_return": "0.108",
      "max_drawdown": "0.071",
      "sharpe": "1.32",
      "sortino": "1.81",
      "win_rate": "0.54",
      "turnover": "2.3"
    },
    "orders_count": 42,
    "fills_count": 40
  },
  "error": null,
  "request_id": "req_001",
  "ts": 1700000000000
}
```

---

# 12. Replay API

---

## 12.1 启动 Replay

```http
POST /api/v1/replay/start
```

请求：

```json
{
  "name": "btc_replay",
  "strategy_name": "ma_cross",
  "market": "CRYPTO",
  "base_currency": "USDT",
  "initial_cash": "10000",
  "symbols": [
    {
      "market": "CRYPTO",
      "exchange": "BINANCE",
      "symbol": "BTCUSDT",
      "asset_class": "CRYPTO_SPOT"
    }
  ],
  "data": {
    "source": "parquet",
    "timeframe": "1m",
    "start": 1704067200000,
    "end": 1704153600000
  },
  "speed": "10x",
  "params": {
    "fast": 20,
    "slow": 60,
    "target_percent": "0.5"
  }
}
```

响应：

```json
{
  "success": true,
  "data": {
    "run_id": "run_001",
    "replay_id": "replay_001",
    "status": "RUNNING",
    "speed": "10x"
  },
  "error": null,
  "request_id": "req_001",
  "ts": 1700000000000
}
```

---

## 12.2 暂停 Replay

```http
POST /api/v1/replay/{run_id}/pause
```

响应：

```json
{
  "success": true,
  "data": {
    "run_id": "run_001",
    "status": "PAUSED"
  },
  "error": null,
  "request_id": "req_001",
  "ts": 1700000000000
}
```

---

## 12.3 恢复 Replay

```http
POST /api/v1/replay/{run_id}/resume
```

响应：

```json
{
  "success": true,
  "data": {
    "run_id": "run_001",
    "status": "RUNNING"
  },
  "error": null,
  "request_id": "req_001",
  "ts": 1700000000000
}
```

---

## 12.4 跳转 Replay 时间

```http
POST /api/v1/replay/{run_id}/seek
```

请求：

```json
{
  "ts": 1704070800000
}
```

响应：

```json
{
  "success": true,
  "data": {
    "run_id": "run_001",
    "status": "SEEKING",
    "target_ts": 1704070800000
  },
  "error": null,
  "request_id": "req_001",
  "ts": 1700000000000
}
```

---

## 12.5 修改 Replay 速度

```http
POST /api/v1/replay/{run_id}/speed
```

请求：

```json
{
  "speed": "50x"
}
```

响应：

```json
{
  "success": true,
  "data": {
    "run_id": "run_001",
    "speed": "50x"
  },
  "error": null,
  "request_id": "req_001",
  "ts": 1700000000000
}
```

---

## 12.6 停止 Replay

```http
POST /api/v1/replay/{run_id}/stop
```

响应：

```json
{
  "success": true,
  "data": {
    "run_id": "run_001",
    "status": "STOPPING"
  },
  "error": null,
  "request_id": "req_001",
  "ts": 1700000000000
}
```

---

# 13. Orders API

---

## 13.1 查询订单列表

```http
GET /api/v1/runs/{run_id}/orders
```

Path 参数：

| 参数     | 类型     | 说明    |
| ------ | ------ | ----- |
| run_id | string | 运行 ID |

Query 参数：

| 参数          | 类型      | 说明                    |
| ----------- | ------- | --------------------- |
| market      | string  | CN / HK / US / CRYPTO |
| exchange    | string  | 交易所                   |
| symbol      | string  | 标的                    |
| asset_class | string  | 资产类型                  |
| status      | string  | 订单状态                  |
| side        | string  | BUY / SELL            |
| page        | integer | 页码                    |
| page_size   | integer | 每页数量                  |

示例：

```http
GET /api/v1/runs/run_001/orders?status=FILLED&page=1&page_size=50
```

响应：

```json
{
  "success": true,
  "data": {
    "items": [
      {
        "id": "ord_001",
        "run_id": "run_001",
        "client_order_id": "cli_001",
        "broker_order_id": "brk_001",
        "market": "CRYPTO",
        "exchange": "BINANCE",
        "symbol": "BTCUSDT",
        "asset_class": "CRYPTO_SPOT",
        "side": "BUY",
        "order_type": "LIMIT",
        "price": "68000",
        "qty": "0.01",
        "filled_qty": "0.01",
        "remaining_qty": "0",
        "avg_fill_price": "68000",
        "status": "FILLED",
        "created_at": 1700000000000,
        "updated_at": 1700000100000
      }
    ],
    "page": 1,
    "page_size": 50,
    "total": 1,
    "has_next": false
  },
  "error": null,
  "request_id": "req_001",
  "ts": 1700000000000
}
```

---

## 13.2 查询单个订单

```http
GET /api/v1/orders/{order_id}
```

响应：

```json
{
  "success": true,
  "data": {
    "id": "ord_001",
    "run_id": "run_001",
    "client_order_id": "cli_001",
    "broker_order_id": "brk_001",
    "market": "CRYPTO",
    "exchange": "BINANCE",
    "symbol": "BTCUSDT",
    "asset_class": "CRYPTO_SPOT",
    "side": "BUY",
    "order_type": "LIMIT",
    "time_in_force": "GTC",
    "price": "68000",
    "qty": "0.01",
    "filled_qty": "0.01",
    "remaining_qty": "0",
    "avg_fill_price": "68000",
    "status": "FILLED",
    "reduce_only": false,
    "post_only": false,
    "leverage": null,
    "margin_mode": null,
    "position_side": "NET",
    "created_at": 1700000000000,
    "updated_at": 1700000100000
  },
  "error": null,
  "request_id": "req_001",
  "ts": 1700000000000
}
```

---

## 13.3 撤单

```http
POST /api/v1/orders/{order_id}/cancel
```

请求：

```json
{
  "reason": "manual cancel"
}
```

响应：

```json
{
  "success": true,
  "data": {
    "order_id": "ord_001",
    "status": "PENDING_CANCEL"
  },
  "error": null,
  "request_id": "req_001",
  "ts": 1700000000000
}
```

---

# 14. Fills API

---

## 14.1 查询成交列表

```http
GET /api/v1/runs/{run_id}/fills
```

Path 参数：

| 参数     | 类型     | 说明    |
| ------ | ------ | ----- |
| run_id | string | 运行 ID |

Query 参数：

| 参数          | 类型      | 说明    |
| ----------- | ------- | ----- |
| order_id    | string  | 订单 ID |
| market      | string  | 市场    |
| exchange    | string  | 交易所   |
| symbol      | string  | 标的    |
| asset_class | string  | 资产类型  |
| start_ts    | integer | 开始时间  |
| end_ts      | integer | 结束时间  |
| page        | integer | 页码    |
| page_size   | integer | 每页数量  |

响应：

```json
{
  "success": true,
  "data": {
    "items": [
      {
        "id": "fill_001",
        "run_id": "run_001",
        "order_id": "ord_001",
        "market": "CRYPTO",
        "exchange": "BINANCE",
        "symbol": "BTCUSDT",
        "asset_class": "CRYPTO_SPOT",
        "side": "BUY",
        "price": "68000",
        "qty": "0.01",
        "gross_amount": "680",
        "fee": "0.68",
        "tax": "0",
        "funding_fee": "0",
        "net_amount": "680.68",
        "currency": "USDT",
        "liquidity": "TAKER",
        "is_maker": false,
        "ts": 1700000100000
      }
    ],
    "page": 1,
    "page_size": 50,
    "total": 1,
    "has_next": false
  },
  "error": null,
  "request_id": "req_001",
  "ts": 1700000000000
}
```

---

# 15. Positions API

---

## 15.1 查询普通持仓

用于：

```text
A股
港股
美股
数字货币现货
```

```http
GET /api/v1/runs/{run_id}/positions
```

Path 参数：

| 参数     | 类型     | 说明    |
| ------ | ------ | ----- |
| run_id | string | 运行 ID |

Query 参数：

| 参数          | 类型     | 说明    |
| ----------- | ------ | ----- |
| market      | string | 市场    |
| exchange    | string | 交易所   |
| symbol      | string | 标的    |
| asset_class | string | 资产类型  |

响应：

```json
{
  "success": true,
  "data": {
    "items": [
      {
        "run_id": "run_001",
        "market": "CRYPTO",
        "exchange": "BINANCE",
        "symbol": "BTCUSDT",
        "asset_class": "CRYPTO_SPOT",
        "qty": "0.01",
        "available_qty": "0.01",
        "avg_price": "68000",
        "market_price": "69000",
        "market_value": "690",
        "cost_basis": "680",
        "unrealized_pnl": "10",
        "realized_pnl": "0",
        "currency": "USDT",
        "updated_ts": 1700000000000
      }
    ]
  },
  "error": null,
  "request_id": "req_001",
  "ts": 1700000000000
}
```

---

## 15.2 查询数字货币合约持仓

```http
GET /api/v1/runs/{run_id}/crypto-positions
```

Path 参数：

| 参数            | 类型     | 说明                 |
| ------------- | ------ | ------------------ |
| run_id        | string | 运行 ID              |

当前本地实现按 `run_id` 查询 `crypto_positions` storage boundary。它是只读查询面；paper runtime 会在模拟合约成交和资金费结算后写入合约持仓状态，Decimal 字段以字符串返回。

CLI 查询：

```powershell
trader positions list --run-id run_001 --account paper --exchange BINANCE
```

响应为数组：

```json
[
  {
    "run_id": "run_001",
    "account_id": "paper",
    "exchange": "BINANCE",
    "symbol": "BTCUSDT_PERP",
    "asset_class": "CRYPTO_PERP",
    "margin_mode": "cross",
    "position_side": "short",
    "leverage": "3.5",
    "qty": "-0.250",
    "avg_price": "65001.0000",
    "margin_used": "1625.025",
    "funding_fee": "-1.50",
    "realized_pnl": "2.00",
    "unrealized_pnl": "20.0001",
    "updated_at_ms": 1700000000000
  }
]
```

---

# 16. Account API

---

## 16.1 查询账户余额

```http
GET /api/v1/runs/{run_id}/account-balances
```

Path 参数：

| 参数     | 类型     | 说明    |
| ------ | ------ | ----- |
| run_id | string | 运行 ID |

Query 参数：

| 参数       | 类型     | 说明      |
| -------- | ------ | ------- |
| market   | string | 市场      |
| exchange | string | 交易所     |
| asset    | string | 币种 / 货币 |

响应：

```json
{
  "success": true,
  "data": {
    "items": [
      {
        "run_id": "run_001",
        "market": "CRYPTO",
        "exchange": "BINANCE",
        "asset": "USDT",
        "total": "10000",
        "available": "9320",
        "frozen": "680",
        "borrowed": "0",
        "interest": "0",
        "updated_ts": 1700000000000
      },
      {
        "run_id": "run_001",
        "market": "CRYPTO",
        "exchange": "BINANCE",
        "asset": "BTC",
        "total": "0.01",
        "available": "0.01",
        "frozen": "0",
        "borrowed": "0",
        "interest": "0",
        "updated_ts": 1700000000000
      }
    ]
  },
  "error": null,
  "request_id": "req_001",
  "ts": 1700000000000
}
```

---

## 16.2 查询现金

```http
GET /api/v1/cash
```

Query 参数：

| 参数       | 类型     | 说明                     |
| -------- | ------ | ---------------------- |
| run_id   | string | 运行 ID                  |
| currency | string | CNY / HKD / USD / USDT |

响应：

```json
{
  "success": true,
  "data": {
    "items": [
      {
        "run_id": "run_001",
        "currency": "USD",
        "cash": "100000",
        "available_cash": "80000",
        "frozen_cash": "20000",
        "ts": 1700000000000
      }
    ]
  },
  "error": null,
  "request_id": "req_001",
  "ts": 1700000000000
}
```

---

# 17. Portfolio API

---

## 17.1 查询当前组合

```http
GET /api/v1/portfolio
```

Query 参数：

| 参数     | 类型     | 说明    |
| ------ | ------ | ----- |
| run_id | string | 运行 ID |

响应：

```json
{
  "success": true,
  "data": {
    "run_id": "run_001",
    "base_currency": "USDT",
    "cash": "9320",
    "market_value": "690",
    "equity": "10010",
    "margin_used": "0",
    "margin_available": "0",
    "realized_pnl": "0",
    "unrealized_pnl": "10",
    "total_fee": "0.68",
    "total_tax": "0",
    "total_funding_fee": "0",
    "drawdown": "0",
    "drawdown_pct": "0",
    "daily_return": "0.001",
    "cumulative_return": "0.001",
    "ts": 1700000000000
  },
  "error": null,
  "request_id": "req_001",
  "ts": 1700000000000
}
```

---

## 17.2 查询组合快照

```http
GET /api/v1/runs/{run_id}/portfolio-snapshots
```

Path 参数：

| 参数     | 类型     | 说明    |
| ------ | ------ | ----- |
| run_id | string | 运行 ID |

Query 参数：

| 参数       | 类型      | 说明    |
| -------- | ------- | ----- |
| start_ts | integer | 开始时间  |
| end_ts   | integer | 结束时间  |

响应：

```json
{
  "success": true,
  "data": {
    "items": [
      {
        "ts": 1700000000000,
        "equity": "10000",
        "cash": "10000",
        "market_value": "0",
        "drawdown_pct": "0",
        "cumulative_return": "0"
      },
      {
        "ts": 1700003600000,
        "equity": "10100",
        "cash": "9320",
        "market_value": "780",
        "drawdown_pct": "0",
        "cumulative_return": "0.01"
      }
    ]
  },
  "error": null,
  "request_id": "req_001",
  "ts": 1700000000000
}
```

---

## 17.3 查询现金快照

```http
GET /api/v1/runs/{run_id}/cash-snapshots?currency=USD&from_ms=1700000000000&to_ms=1700000600000
```

通过 run-scoped endpoint 查询指定运行。支持 `currency`、`from_ms`、`to_ms` 可选过滤。返回数组：

```json
[
  {
    "id": 1,
    "run_id": "sample-ma-cross",
    "ts_ms": 1700000000000,
    "currency": "USD",
    "cash": "99980",
    "available_cash": "99980",
    "frozen_cash": "0",
    "created_at_ms": 1700000000000
  }
]
```

---

## 17.4 查询持仓快照

```http
GET /api/v1/runs/{run_id}/position-snapshots?symbol=BTCUSDT_PERP&position_side=LONG&from_ms=1700000000000&to_ms=1700000600000
```

通过 run-scoped endpoint 查询指定运行。支持 `symbol`、`position_side`、`from_ms`、`to_ms` 可选过滤。返回数组：

```json
[
  {
    "id": 1,
    "run_id": "sample-ma-cross",
    "ts_ms": 1700000000000,
    "market": "US",
    "exchange": "NASDAQ",
    "symbol": "US:NASDAQ:AAPL:EQUITY",
    "asset_class": "EQUITY",
    "position_side": null,
    "qty": "1",
    "available_qty": "1",
    "avg_price": "20",
    "entry_price": "20",
    "market_price": null,
    "mark_price": null,
    "market_value": null,
    "unrealized_pnl": null,
    "realized_pnl": null,
    "currency": "USD",
    "created_at_ms": 1700000000000
  }
]
```

---

## 17.5 查询对账状态

```http
GET /api/v1/runs/{run_id}/reconciliation
```

返回指定 run 的 snapshot 覆盖和 drift 投影状态：

```json
{
  "run_id": "sample-ma-cross",
  "status": "drift",
  "cash_snapshots": 1,
  "position_snapshots": 1,
  "latest_cash_ts_ms": 1700000000000,
  "latest_position_ts_ms": 1700000000000,
  "drift_events": [
    {
      "id": "risk-1",
      "event_id": "evt-1",
      "run_id": "sample-ma-cross",
      "account_id": "paper",
      "symbol": "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP",
      "risk_type": "reconciliation_drift",
      "decision": "rejected",
      "reason": "position_qty_drift",
      "threshold": "1",
      "observed_value": "5",
      "ts_ms": 1700000000000,
      "payload": {}
    }
  ]
}
```

`status` 为 `ok` 表示当前没有 drift 投影事件，为 `drift` 表示已记录 reconciliation drift。

---

# 18. Metrics API

---

## 18.1 查询绩效指标

```http
GET /api/v1/runs/{run_id}/metrics
```

Path 参数：

| 参数     | 类型     | 说明    |
| ------ | ------ | ----- |
| run_id | string | 运行 ID |

响应：

```json
{
  "success": true,
  "data": {
    "run_id": "run_001",
    "total_return": "0.12",
    "annual_return": "0.18",
    "max_drawdown": "0.07",
    "sharpe": "1.42",
    "sortino": "1.88",
    "win_rate": "0.54",
    "profit_factor": "1.35",
    "turnover": "2.1",
    "order_count": 100,
    "fill_count": 96,
    "fill_rate": "0.96",
    "cancel_rate": "0.04",
    "total_fee": "128.5",
    "total_tax": "0",
    "total_funding_fee": "-3.2"
  },
  "error": null,
  "request_id": "req_001",
  "ts": 1700000000000
}
```

---

## 18.2 查询日志 writer 运维指标

```http
GET /api/v1/ops/logging/metrics
```

响应：

```json
{
  "metrics": {
    "dropped_logs": 0
  },
  "enabled": true,
  "level": "info",
  "categories": [],
  "buffer_size": 1000,
  "batch_size": 100,
  "flush_interval_ms": 5000
}
```

`dropped_logs` 是当前 API 进程内 `LogWriterMetrics` 的累计 dropped count，用于判断 tracing channel backpressure 是否导致日志丢弃。

---

# 19. Risk API

---

## 19.1 查询风控事件

```http
GET /api/v1/risk-events
```

Query 参数：

| 参数        | 类型      | 说明                                |
| --------- | ------- | --------------------------------- |
| run_id    | string  | 运行 ID                             |
| risk_type | string  | 风控类型                              |
| severity  | string  | INFO / WARNING / ERROR / CRITICAL |
| start_ts  | integer | 开始时间                              |
| end_ts    | integer | 结束时间                              |
| page      | integer | 页码                                |
| page_size | integer | 每页数量                              |

响应：

```json
{
  "success": true,
  "data": {
    "items": [
      {
        "id": "risk_001",
        "run_id": "run_001",
        "ts": 1700000000000,
        "market": "CRYPTO",
        "exchange": "BINANCE",
        "symbol": "BTCUSDT",
        "asset_class": "CRYPTO_PERP",
        "risk_type": "LIQUIDATION_RISK",
        "severity": "WARNING",
        "action": "ADJUST",
        "message": "margin ratio too high",
        "order_id": null
      }
    ],
    "page": 1,
    "page_size": 50,
    "total": 1,
    "has_next": false
  },
  "error": null,
  "request_id": "req_001",
  "ts": 1700000000000
}
```

---

# 20. Market Data API

---

## 20.1 查询标的信息

```http
GET /api/v1/instruments
```

Query 参数：

| 参数          | 类型     | 说明                    |
| ----------- | ------ | --------------------- |
| market      | string | CN / HK / US / CRYPTO |
| exchange    | string | 交易所                   |
| asset_class | string | 资产类型                  |
| symbol      | string | 标的                    |

响应：

```json
{
  "success": true,
  "data": {
    "items": [
      {
        "market": "CRYPTO",
        "exchange": "BINANCE",
        "symbol": "BTCUSDT",
        "name": "BTC/USDT",
        "asset_class": "CRYPTO_SPOT",
        "base_asset": "BTC",
        "quote_asset": "USDT",
        "settlement_asset": null,
        "lot_size": "0.00001",
        "tick_size": "0.01",
        "min_qty": "0.00001",
        "min_notional": "5",
        "price_precision": 2,
        "qty_precision": 5,
        "is_active": true,
        "is_tradable": true
      }
    ]
  },
  "error": null,
  "request_id": "req_001",
  "ts": 1700000000000
}
```

---

## 20.2 查询单个标的

```http
GET /api/v1/instruments/{market}/{exchange}/{symbol}
```

示例：

```http
GET /api/v1/instruments/CRYPTO/BINANCE/BTCUSDT
```

---

## 20.3 查询 K 线

```http
GET /api/v1/candles
```

Query 参数：

| 参数          | 类型      | 说明                    |
| ----------- | ------- | --------------------- |
| market      | string  | CN / HK / US / CRYPTO |
| exchange    | string  | 交易所                   |
| symbol      | string  | 标的                    |
| asset_class | string  | 资产类型                  |
| timeframe   | string  | 1d / 1m / 5m / 1h     |
| start_ts    | integer | 开始时间                  |
| end_ts      | integer | 结束时间                  |
| limit       | integer | 数量                    |

响应：

```json
{
  "success": true,
  "data": {
    "items": [
      {
        "ts": 1700000000000,
        "market": "CRYPTO",
        "exchange": "BINANCE",
        "symbol": "BTCUSDT",
        "asset_class": "CRYPTO_SPOT",
        "timeframe": "1m",
        "open": "68000",
        "high": "68100",
        "low": "67900",
        "close": "68050",
        "volume": "120.5",
        "quote_volume": "8190000"
      }
    ]
  },
  "error": null,
  "request_id": "req_001",
  "ts": 1700000000000
}
```

---

## 20.4 查询资金费率

```http
GET /api/v1/funding-rates
```

Query 参数：

| 参数       | 类型      | 说明   |
| -------- | ------- | ---- |
| exchange | string  | 交易所  |
| symbol   | string  | 合约   |
| start_ms | integer | 开始时间 |
| end_ms   | integer | 结束时间 |

当前本地实现查询 `funding_rates` storage boundary，返回 `[start_ms, end_ms)` 时间窗口内的数据。响应为数组：

CLI 查询：

```powershell
trader funding list --exchange BINANCE --symbol BTCUSDT_PERP --from 1700000000000 --to 1700100000000
```

```json
[
  {
    "id": "funding_001",
    "exchange": "BINANCE",
    "symbol": "BTCUSDT_PERP",
    "funding_time_ms": 1700000000000,
    "funding_rate": "0.0001",
    "mark_price": "68000",
    "source": "exchange"
  }
]
```

---

## 20.5 查询数字货币市场元数据

```http
GET /api/v1/crypto-market-meta
```

Query 参数：

| 参数       | 类型     | 说明 |
| -------- | ------ | ---- |
| exchange | string | 交易所 |
| symbol   | string | 合约或交易对 |

当前本地实现查询 `crypto_market_meta` storage boundary。没有匹配记录时返回空数组；Binance exchangeInfo ingestion 已可通过 CLI 或 scheduled ingestion 写入该 storage boundary，生产级限流退避和陈旧数据告警仍是后续 hardening。响应为数组：

```json
[
  {
    "id": 1,
    "exchange": "BINANCE",
    "symbol": "BTCUSDT_PERP",
    "base_asset": "BTC",
    "quote_asset": "USDT",
    "instrument_type": "PERP",
    "contract_type": "LINEAR",
    "contract_size": "1",
    "settlement_asset": "USDT",
    "min_notional": "10",
    "min_qty": "0.001",
    "max_qty": "100",
    "price_precision": 2,
    "qty_precision": 3,
    "price_tick": "0.10",
    "qty_step": "0.001",
    "maker_fee_rate": "0.0002",
    "taker_fee_rate": "0.0004",
    "funding_interval_hours": 8,
    "max_leverage": "50",
    "margin_modes": ["CROSS", "ISOLATED"],
    "is_inverse": false,
    "is_active": true,
    "created_at_ms": 1700000000000,
    "updated_at_ms": 1700000000000
  }
]
```

---

## 20.6 查询公司行动元数据

```http
GET /api/v1/corporate-actions
```

Query 参数：

| 参数       | 类型      | 说明 |
| -------- | ------- | ---- |
| market   | string  | 市场 |
| symbol   | string  | 标的 |
| start_ms | integer | 开始时间 |
| end_ms   | integer | 结束时间 |

当前本地实现查询 `corporate_actions_meta` storage boundary，返回 `[start_ms, end_ms)` 时间窗口内的数据。Yahoo corporate actions ingestion 已可通过 CLI 或 scheduled ingestion 写入该 storage boundary，生产级限流退避和陈旧数据告警仍是后续 hardening。响应为数组：

```json
[
  {
    "id": 1,
    "market": "US",
    "exchange": "NASDAQ",
    "symbol": "US:NASDAQ:AAPL:EQUITY",
    "action_type": "SPLIT",
    "ex_date_ms": 1700000000000,
    "record_date_ms": 1700086400000,
    "payable_date_ms": 1700172800000,
    "ratio": "4:1",
    "cash_amount": null,
    "currency": null,
    "source": "exchange",
    "created_at_ms": 1700000000000,
    "updated_at_ms": 1700000000000
  }
]
```

---

## 20.7 查询 Open Interest

```http
GET /api/v1/open-interest
```

Query 参数：

| 参数       | 类型      | 说明   |
| -------- | ------- | ---- |
| exchange | string  | 交易所  |
| symbol   | string  | 合约   |
| start_ts | integer | 开始时间 |
| end_ts   | integer | 结束时间 |

响应：

```json
{
  "success": true,
  "data": {
    "items": [
      {
        "ts": 1700000000000,
        "exchange": "BINANCE",
        "symbol": "BTCUSDT",
        "open_interest": "120000",
        "open_interest_value": "8160000000"
      }
    ]
  },
  "error": null,
  "request_id": "req_001",
  "ts": 1700000000000
}
```

---

## 20.8 Fee Rules

创建 fee rule：

```http
POST /api/v1/fee-rules
```

```json
{
  "id": "fee-us-nasdaq-equity",
  "market": "US",
  "exchange": "NASDAQ",
  "asset_class": "EQUITY",
  "symbol": null,
  "volume_window": "rolling_30d",
  "maker_bps": "1",
  "taker_bps": "2",
  "minimum_fee": "0.01",
  "tax_bps": null,
  "exchange_fee_bps": null,
  "effective_from_ms": 1700000000000,
  "effective_to_ms": null,
  "tiers": [
    {
      "id": "fee-us-nasdaq-equity-tier-1",
      "volume_from": "0",
      "volume_to": "100000",
      "maker_bps": "1",
      "taker_bps": "2"
    }
  ]
}
```

`volume_window` 可选值：

| 值 | 说明 |
| --- | --- |
| `run` | 默认值；运行开始时不读取历史成交，tier 只按本次运行内累计成交名义金额推进。 |
| `rolling_30d` | 运行启动时读取同账户、同规则 scope 最近 30 天历史成交作为初始 tier volume；运行中在每次成交计费前剔除滑出 30 天窗口的成交，再累计本次成交。 |
| `calendar_month` | 运行启动时读取同账户、同规则 scope 当月 1 日 00:00:00 UTC 至启动时刻的历史成交作为初始 tier volume；运行中在每次成交计费前按 UTC 月初剔除上月成交，跨月后只累计当前 UTC 月成交。 |

查询当前有效 fee rule：

```http
GET /api/v1/fee-rules?market=US&exchange=NASDAQ&asset_class=EQUITY&symbol=US:NASDAQ:AAPL:EQUITY&at_ms=1700000000000
```

响应：

```json
{
  "rule": {
    "id": "fee-us-nasdaq-equity",
    "market": "US",
    "exchange": "NASDAQ",
    "asset_class": "EQUITY",
    "symbol": null,
    "volume_window": "rolling_30d",
    "maker_bps": "1",
    "taker_bps": "2",
    "minimum_fee": "0.01",
    "tax_bps": null,
    "exchange_fee_bps": null,
    "effective_from_ms": 1700000000000,
    "effective_to_ms": null
  },
  "tiers": [
    {
      "id": "fee-us-nasdaq-equity-tier-1",
      "fee_rule_id": "fee-us-nasdaq-equity",
      "volume_from": "0",
      "volume_to": "100000",
      "maker_bps": "1",
      "taker_bps": "2"
    }
  ]
}
```

---

# 21. Config API

当前 V1 同时保留两类配置面：

- API 启动 Backtest、Paper、Replay 或 Live，以及 CLI 启动 Backtest、Paper 或 Replay 时，会把本次运行使用的 TOML 文件保存为 `configs` 表中的 `RUN` 配置快照，使用 checksum 作为 release version，并把 run 绑定到该 config version。
- 管理型配置使用 `POST /api/v1/configs` 创建不可变版本，支持 `draft -> pending_review -> approved -> published -> archived` 生命周期、JSON diff、rollback、状态变更审计，以及 `target_env=production` 时的独立审批发布约束、轻量 role policy 和 pending approval queue。

当前接口返回未包 envelope 的 JSON。生产级 RBAC 身份认证、多环境权限矩阵和多人审批队列仍是后续 production-hardening 工作。

---

## 21.1 查询原始配置列表

```http
GET /api/v1/configs
```

响应：

```json
[
  {
    "id": "run:sample-ma-cross",
    "name": "sample-ma-cross",
    "config_type": "RUN",
    "format": "TOML",
    "content": "[runtime]\nmode = \"paper\"\nrun_id = \"sample-ma-cross\"\n",
    "checksum": "fnv1a64:0000000000000000",
    "created_at_ms": 1700000000000,
    "updated_at_ms": 1700000000000
  }
]
```

---

## 21.2 创建管理型配置版本

```http
POST /api/v1/configs
```

请求：

```json
{
  "name": "paper-risk",
  "content": {
    "enabled": true,
    "risk": {
      "max_order_notional": "1000"
    }
  },
  "created_by": "ops",
  "parent_version": null,
  "target_env": "production",
  "rollout": "canary",
  "ts_ms": 1700000000000
}
```

响应状态：`201 Created`

```json
{
  "id": "config:paper-risk:v1",
  "name": "paper-risk",
  "version": 1,
  "content": {
    "enabled": true,
    "risk": {
      "max_order_notional": "1000"
    }
  },
  "state": "draft",
  "parent_version": null,
  "created_by": "ops",
  "created_at_ms": 1700000000000,
  "state_changed_at_ms": 1700000000000,
  "state_changed_by": "ops",
  "state_change_reason": null,
  "target_env": "production",
  "rollout": "canary",
  "approved_by": null,
  "approved_at_ms": null,
  "published_by": null,
  "published_at_ms": null
}
```

---

## 21.3 查询配置版本

```http
GET /api/v1/configs/{name}
```

响应：

```json
[
  {
    "id": "config:paper-risk:v1",
    "name": "paper-risk",
    "version": 1,
    "content": {
      "enabled": true
    },
    "state": "published",
    "parent_version": null,
    "created_by": "ops",
    "created_at_ms": 1700000000000,
    "state_changed_at_ms": 1700000000300,
    "state_changed_by": "ops",
    "state_change_reason": "rollout",
    "target_env": "production",
    "rollout": "canary",
    "approved_by": "risk-owner",
    "approved_at_ms": 1700000000200,
    "published_by": "ops",
    "published_at_ms": 1700000000300
  }
]
```

---

## 21.4 查询 latest/published/specific 版本

```http
GET /api/v1/configs/{name}/latest
GET /api/v1/configs/{name}/published
GET /api/v1/configs/{name}/{version}
```

未找到时返回 `404`。成功响应为单个 `ConfigVersionResponse`。

---

## 21.5 更新配置状态

```http
PUT /api/v1/configs/{name}/{version}/state
```

有效转换：

- `draft -> pending_review`
- `draft -> archived`
- `pending_review -> approved`
- `approved -> published`
- `approved -> archived`
- `published -> archived`

当配置的 `target_env` 为 `production` 时，`published` 转换要求：

- 当前版本已经处于 `approved`。
- `approved_by` 存在。
- `changed_by` 不能等于 `approved_by`，即发布人不能是最后一次审批人。
- 如果请求提供 `actor_role`，则 production 状态变更会执行轻量 role policy：`release_manager` 可提交 review、publish、archive；`approver` 可 approve。

请求：

```json
{
  "new_state": "approved",
  "changed_by": "ops",
  "actor_role": "approver",
  "reason": "review passed",
  "ts_ms": 1700000000300
}
```

响应为更新后的 `ConfigVersionResponse`。每次有效状态变更都会写入 `config_audits`、`config_releases` 和 `event_store` 的 `config.state.changed` 事件。

---

## 21.6 查询待审批配置

```http
GET /api/v1/config-approvals/pending?target_env=production
```

响应：

```json
[
  {
    "id": "config:paper-risk:v1",
    "name": "paper-risk",
    "version": 1,
    "content": {
      "enabled": true
    },
    "state": "pending_review",
    "parent_version": null,
    "created_by": "ops",
    "created_at_ms": 1700000000000,
    "state_changed_at_ms": 1700000000100,
    "state_changed_by": "release",
    "state_change_reason": "request approval",
    "target_env": "production",
    "rollout": "canary",
    "approved_by": null,
    "approved_at_ms": null,
    "published_by": null,
    "published_at_ms": null
  }
]
```

---

## 21.7 Diff 两个配置版本

```http
GET /api/v1/configs/{name}/diff?v1=1&v2=2
```

响应：

```json
{
  "name": "paper-risk",
  "version_a": 1,
  "version_b": 2,
  "added": ["risk.max_position"],
  "removed": [],
  "changed": [
    {
      "path": "risk.max_order_notional",
      "before": "1000",
      "after": "1500"
    }
  ]
}
```

---

## 21.8 Rollback 配置版本

```http
POST /api/v1/configs/{name}/{version}/rollback
```

Rollback 不会覆盖旧版本；它复制目标版本内容并创建一个新的 `draft` 版本，`parent_version` 指向被 rollback 的版本。

请求：

```json
{
  "actor": "ops",
  "reason": "restore stable config",
  "ts_ms": 1700000000400
}
```

响应状态：`201 Created`，响应体为新 draft 版本的 `ConfigVersionResponse`。

---

## 21.9 查询配置发布和审计记录

```http
GET /api/v1/configs/{config_id}/releases
GET /api/v1/configs/{config_id}/audits
```

发布响应：

```json
[
  {
    "id": "config:paper-risk:v1:1",
    "config_id": "config:paper-risk:v1",
    "version": "1",
    "status": "published",
    "released_by": "ops",
    "notes": "rollout",
    "created_at_ms": 1700000000000,
    "updated_at_ms": 1700000000300
  }
]
```

---

## 21.10 查询运行绑定的配置版本

```http
GET /api/v1/runs/{run_id}/config-version
```

响应：

```json
{
  "run_id": "sample-ma-cross",
  "config_id": "run:sample-ma-cross",
  "version": "fnv1a64:0000000000000000",
  "bound_at_ms": 1700000000000
}
```

---

# 22. WebSocket API

---

# 22.1 WebSocket Endpoint

```text
ws://127.0.0.1:8080/ws
```

---

# 22.2 WebSocket 消息格式

当前本地实现支持两个客户端消息类型：按 `run_id` 订阅事件，以及控制 active replay run。

```json
{
  "type": "subscribe",
  "run_id": "run_001"
}
```

字段说明：

| 字段      | 说明      |
| ------- | ------- |
| type    | 消息类型    |
| run_id  | 运行 ID |
| action  | replay_control 使用 |
| offset  | seek 使用 |
| speed   | speed 使用 |

---

# 22.3 客户端消息类型

```text
subscribe
replay_control
```

---

# 22.4 服务端消息类型

```text
event
replay_state
error
```

---

# 22.5 WebSocket Channel

WebSocket 当前按 `run_id` 过滤 persisted/runtime events，不维护独立 channel 订阅列表。

---

# 22.6 subscribe

客户端发送：

```json
{
  "type": "subscribe",
  "run_id": "run_001"
}
```

服务端先回放该 run 已持久化的事件，再持续推送匹配该 run 的 runtime event：

```json
{
  "type": "event",
  "event": {
    "source": "run_001",
    "payload": {}
  }
}
```

---

# 22.7 关闭订阅

当前未实现独立 unsubscribe 消息；关闭 WebSocket 连接即可停止订阅。

---

# 22.8 连接保活

当前未实现独立 ping/pong 消息；连接存活由 WebSocket transport 处理。

---

# 23. WebSocket 控制消息

当前 WebSocket 控制面只实现 replay 控制。运行启动、停止、配置变更等控制操作走 REST API。

## 23.1 replay_control

```json
{
  "type": "replay_control",
  "run_id": "run_001",
  "action": "pause"
}
```

```json
{
  "type": "replay_control",
  "run_id": "run_001",
  "action": "seek",
  "offset": 25
}
```

```json
{
  "type": "replay_control",
  "run_id": "run_001",
  "action": "speed",
  "speed": 50
}
```

Replay control succeeds only when `{run_id}` is registered as a currently running replay run. Unknown run ids return `unknown_replay_run`; stale, completed, or non-replay run ids return `inactive_replay_run`.

---

# 24. WebSocket 推送消息

WebSocket 实际推送外层统一为：

```json
{
  "type": "event",
  "event": {}
}
```

下面示例展示的是 `event` 内部可能承载的领域事件数据形态。

---

## 24.1 MarketEvent

```json
{
  "id": "evt_001",
  "type": "MarketEvent",
  "channel": "market",
  "run_id": "run_001",
  "ts": 1700000000000,
  "data": {
    "market": "CRYPTO",
    "exchange": "BINANCE",
    "symbol": "BTCUSDT",
    "asset_class": "CRYPTO_SPOT",
    "last_price": "68000",
    "bid_price": "67999.9",
    "ask_price": "68000.1",
    "volume": "120.5"
  }
}
```

---

## 24.2 OrderEvent

```json
{
  "id": "evt_002",
  "type": "OrderEvent",
  "channel": "orders",
  "run_id": "run_001",
  "ts": 1700000000000,
  "data": {
    "order_id": "ord_001",
    "client_order_id": "cli_001",
    "broker_order_id": "brk_001",
    "market": "CRYPTO",
    "exchange": "BINANCE",
    "symbol": "BTCUSDT",
    "asset_class": "CRYPTO_SPOT",
    "side": "BUY",
    "order_type": "LIMIT",
    "price": "68000",
    "qty": "0.01",
    "filled_qty": "0.01",
    "remaining_qty": "0",
    "avg_fill_price": "68000",
    "old_status": "SUBMITTED",
    "new_status": "FILLED",
    "message": "order filled"
  }
}
```

---

## 24.3 FillEvent

```json
{
  "id": "evt_003",
  "type": "FillEvent",
  "channel": "fills",
  "run_id": "run_001",
  "ts": 1700000000000,
  "data": {
    "fill_id": "fill_001",
    "order_id": "ord_001",
    "market": "CRYPTO",
    "exchange": "BINANCE",
    "symbol": "BTCUSDT",
    "asset_class": "CRYPTO_SPOT",
    "side": "BUY",
    "price": "68000",
    "qty": "0.01",
    "fee": "0.68",
    "tax": "0",
    "funding_fee": "0",
    "currency": "USDT",
    "liquidity": "TAKER",
    "is_maker": false
  }
}
```

---

## 24.4 PositionEvent

```json
{
  "id": "evt_004",
  "type": "PositionEvent",
  "channel": "positions",
  "run_id": "run_001",
  "ts": 1700000000000,
  "data": {
    "market": "CRYPTO",
    "exchange": "BINANCE",
    "symbol": "BTCUSDT",
    "asset_class": "CRYPTO_SPOT",
    "qty": "0.01",
    "available_qty": "0.01",
    "avg_price": "68000",
    "market_price": "69000",
    "market_value": "690",
    "unrealized_pnl": "10",
    "realized_pnl": "0",
    "currency": "USDT"
  }
}
```

---

## 24.5 CryptoPositionEvent

```json
{
  "id": "evt_005",
  "type": "CryptoPositionEvent",
  "channel": "crypto_positions",
  "run_id": "run_001",
  "ts": 1700000000000,
  "data": {
    "exchange": "BINANCE",
    "symbol": "BTCUSDT",
    "asset_class": "CRYPTO_PERP",
    "position_side": "LONG",
    "qty": "0.1",
    "entry_price": "68000",
    "mark_price": "69000",
    "liquidation_price": "52000",
    "leverage": "5",
    "margin_mode": "ISOLATED",
    "margin_asset": "USDT",
    "initial_margin": "1360",
    "maintenance_margin": "200",
    "unrealized_pnl": "100",
    "realized_pnl": "0",
    "funding_fee": "-1.2",
    "margin_ratio": "0.12"
  }
}
```

---

## 24.6 PortfolioEvent

```json
{
  "id": "evt_006",
  "type": "PortfolioEvent",
  "channel": "portfolio",
  "run_id": "run_001",
  "ts": 1700000000000,
  "data": {
    "base_currency": "USDT",
    "cash": "9320",
    "market_value": "690",
    "equity": "10010",
    "margin_used": "0",
    "margin_available": "0",
    "realized_pnl": "0",
    "unrealized_pnl": "10",
    "total_fee": "0.68",
    "total_tax": "0",
    "total_funding_fee": "0",
    "drawdown": "0",
    "drawdown_pct": "0",
    "daily_return": "0.001",
    "cumulative_return": "0.001"
  }
}
```

---

## 24.7 AccountEvent

```json
{
  "id": "evt_007",
  "type": "AccountEvent",
  "channel": "accounts",
  "run_id": "run_001",
  "ts": 1700000000000,
  "data": {
    "market": "CRYPTO",
    "exchange": "BINANCE",
    "asset": "USDT",
    "total": "10000",
    "available": "9320",
    "frozen": "680",
    "borrowed": "0",
    "interest": "0"
  }
}
```

---

## 24.8 RiskEvent

```json
{
  "id": "evt_008",
  "type": "RiskEvent",
  "channel": "risk",
  "run_id": "run_001",
  "ts": 1700000000000,
  "data": {
    "market": "CRYPTO",
    "exchange": "BINANCE",
    "symbol": "BTCUSDT",
    "asset_class": "CRYPTO_PERP",
    "risk_type": "LIQUIDATION_RISK",
    "severity": "WARNING",
    "action": "ADJUST",
    "message": "margin ratio too high",
    "order_id": null
  }
}
```

---

## 24.9 ReplayEvent

```json
{
  "id": "evt_009",
  "type": "ReplayEvent",
  "channel": "replay",
  "run_id": "run_001",
  "ts": 1700000000000,
  "data": {
    "status": "RUNNING",
    "speed": "10x",
    "current_ts": 1704070800000,
    "start_ts": 1704067200000,
    "end_ts": 1704153600000,
    "progress": "0.25"
  }
}
```

---

## 24.10 SystemEvent

```json
{
  "id": "evt_010",
  "type": "SystemEvent",
  "channel": "system",
  "run_id": "run_001",
  "ts": 1700000000000,
  "data": {
    "level": "INFO",
    "source": "runtime",
    "message": "strategy started"
  }
}
```

---

# 25. WebSocket 错误消息

```json
{
  "id": "msg_001",
  "type": "Error",
  "channel": "system",
  "run_id": null,
  "ts": 1700000000000,
  "data": {
    "code": "INVALID_MESSAGE",
    "message": "unknown message type",
    "details": {}
  }
}
```

---

# 26. Error Code

```text
INVALID_ARGUMENT
INVALID_MESSAGE
NOT_FOUND
ALREADY_EXISTS
RUNTIME_NOT_FOUND
STRATEGY_NOT_FOUND
ORDER_NOT_FOUND
BROKER_NOT_CONNECTED
MARKET_DATA_NOT_READY
RISK_REJECTED
MARKET_RULE_REJECTED
INSUFFICIENT_CASH
INSUFFICIENT_POSITION
INVALID_ORDER_STATUS
REPLAY_NOT_RUNNING
INTERNAL_ERROR
```

---

# 27. WebSocket 连接生命周期

```text
Client Connect
  ↓
Server Accept
  ↓
Client subscribe
  ↓
Server pushes event messages
  ↓
Client Disconnect
```

---

# 28. 安全设计

V1 本地开发阶段可以不启用鉴权。

但接口预留：

```http
Authorization: Bearer <token>
```

后续支持：

```text
API Token
JWT
IP Allowlist
Read-only Token
Admin Token
```

控制类接口必须可限制权限：

```text
启动策略
停止策略
撤单
实盘下单
修改参数
Replay 控制
```

---

# 29. REST 与 WebSocket 分工

REST 适合：

```text
查询历史数据
查询订单列表
查询成交列表
查询运行记录
查询报表
启动一次性任务
配置管理
```

WebSocket 适合：

```text
实时行情
实时订单
实时成交
实时持仓
实时账户
实时风控
Replay 控制
策略参数热更新
```

---

# 30. V1 不做的 API

V1 不提供：

```text
直接手动下单 API
直接绕过 OMS 的 Broker API
直接数据库 SQL API
多用户权限系统
复杂 RBAC
前端 Dashboard
文件上传 API
策略源码上传 API
```

---

# 31. API 结论

Trader API v1 的核心是：

```text
REST 负责查询和管理
WebSocket 负责实时推送和实时控制
所有交易指令必须经过 Runtime / MarketRule / Risk / OMS
任何接口都不能绕过 OMS 直接访问 Broker
任何接口都不能让策略直接访问数据库
```

V1 成功标准：

```text
可以通过 REST 启动 Backtest
可以通过 REST 启动 Replay
可以通过 REST 启动 Paper Strategy
可以通过 REST 查询订单、成交、持仓、账户、绩效
可以通过 WebSocket 订阅实时事件
可以通过 WebSocket 控制 Replay
可以通过 WebSocket 接收订单、成交、组合、风控事件
可以支持股票和数字货币统一事件格式
```
