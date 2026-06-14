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
GET  /api/v1/preflight/paper
POST /api/v1/backtests
POST /api/v1/paper-runs
POST /api/v1/replays
POST /api/v1/live-runs
GET  /api/v1/live-runs/{run_id}/status
POST /api/v1/live-runs/{run_id}/stop
GET  /api/v1/brokers/status
GET  /api/v1/brokers/account/{account_id}
GET  /api/v1/orders
GET  /api/v1/fills
GET  /api/v1/positions
GET  /api/v1/account-balances
GET  /api/v1/portfolio/snapshots
GET  /api/v1/metrics
GET  /api/v1/runs
GET  /api/v1/runs/{run_id}
GET  /api/v1/runs/{run_id}/status
POST /api/v1/runs/{run_id}/cancel
GET  /api/v1/events
GET  /api/v1/runs/{run_id}/events
GET  /api/v1/runs/{run_id}/order-events
GET  /api/v1/runs/{run_id}/risk-events
POST /api/v1/replay/{run_id}/pause
POST /api/v1/replay/{run_id}/resume
POST /api/v1/replay/{run_id}/seek/{offset}
POST /api/v1/replay/{run_id}/speed/{speed}
GET  /ws
```

REST event query responses use an API-owned response model. `payload` is returned as structured JSON, not as a double-encoded JSON string:

`GET /api/v1/runs/{run_id}/order-events` and `GET /api/v1/runs/{run_id}/risk-events` are read-only audit projection queries derived from `event_store`. They do not replace `event_store` as the immutable audit truth and do not provide any manual trading command path.

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

Broker status 返回 Futu、Binance、OKX、Interactive Brokers 的 deterministic fake adapters。Live runtime 只验证本地 lifecycle、broker status 和 stop，不连接真实 broker、不接收凭证、不发真实订单。

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
GET  /api/v1/orders
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
  POST /api/v1/backtests/start
  GET  /api/v1/backtests/{run_id}
  GET  /api/v1/backtests/{run_id}/report

Replay
  POST /api/v1/replay/start
  POST /api/v1/replay/{run_id}/pause
  POST /api/v1/replay/{run_id}/resume
  POST /api/v1/replay/{run_id}/seek
  POST /api/v1/replay/{run_id}/speed
  POST /api/v1/replay/{run_id}/stop

Orders
  GET  /api/v1/orders
  GET  /api/v1/orders/{order_id}
  POST /api/v1/orders/{order_id}/cancel

Fills
  GET  /api/v1/fills

Positions
  GET  /api/v1/positions
  GET  /api/v1/crypto-positions

Accounts
  GET  /api/v1/account-balances
  GET  /api/v1/cash

Portfolio
  GET  /api/v1/portfolio
  GET  /api/v1/portfolio/snapshots

Metrics
  GET  /api/v1/metrics
  GET  /api/v1/metrics/{run_id}

Risk
  GET  /api/v1/risk-events

Market Data
  GET  /api/v1/instruments
  GET  /api/v1/instruments/{market}/{exchange}/{symbol}
  GET  /api/v1/candles
  GET  /api/v1/ticks
  GET  /api/v1/funding-rates
  GET  /api/v1/open-interest

Config
  GET  /api/v1/configs
  GET  /api/v1/configs/{name}
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

## 9.3 停止运行

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
GET /api/v1/orders
```

Query 参数：

| 参数          | 类型      | 说明                    |
| ----------- | ------- | --------------------- |
| run_id      | string  | 运行 ID                 |
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
GET /api/v1/orders?run_id=run_001&status=FILLED&page=1&page_size=50
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
GET /api/v1/fills
```

Query 参数：

| 参数          | 类型      | 说明    |
| ----------- | ------- | ----- |
| run_id      | string  | 运行 ID |
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
GET /api/v1/positions
```

Query 参数：

| 参数          | 类型     | 说明    |
| ----------- | ------ | ----- |
| run_id      | string | 运行 ID |
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
GET /api/v1/crypto-positions
```

Query 参数：

| 参数            | 类型     | 说明                 |
| ------------- | ------ | ------------------ |
| run_id        | string | 运行 ID              |
| exchange      | string | 交易所                |
| symbol        | string | 合约                 |
| position_side | string | LONG / SHORT / NET |

响应：

```json
{
  "success": true,
  "data": {
    "items": [
      {
        "run_id": "run_001",
        "exchange": "BINANCE",
        "symbol": "BTCUSDT",
        "asset_class": "CRYPTO_PERP",
        "position_side": "LONG",
        "qty": "0.1",
        "available_qty": "0.1",
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
        "margin_ratio": "0.12",
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

# 16. Account API

---

## 16.1 查询账户余额

```http
GET /api/v1/account-balances
```

Query 参数：

| 参数       | 类型     | 说明      |
| -------- | ------ | ------- |
| run_id   | string | 运行 ID   |
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
GET /api/v1/portfolio/snapshots
```

Query 参数：

| 参数       | 类型      | 说明    |
| -------- | ------- | ----- |
| run_id   | string  | 运行 ID |
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

# 18. Metrics API

---

## 18.1 查询绩效指标

```http
GET /api/v1/metrics
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
| start_ts | integer | 开始时间 |
| end_ts   | integer | 结束时间 |

响应：

```json
{
  "success": true,
  "data": {
    "items": [
      {
        "exchange": "BINANCE",
        "symbol": "BTCUSDT",
        "funding_time": 1700000000000,
        "funding_rate": "0.0001",
        "mark_price": "68000",
        "index_price": "67980"
      }
    ]
  },
  "error": null,
  "request_id": "req_001",
  "ts": 1700000000000
}
```

---

## 20.5 查询 Open Interest

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

# 21. Config API

---

## 21.1 查询配置列表

```http
GET /api/v1/configs
```

响应：

```json
{
  "success": true,
  "data": {
    "items": [
      {
        "id": "cfg_001",
        "name": "server",
        "config_type": "SYSTEM",
        "format": "TOML",
        "created_at": 1700000000000,
        "updated_at": 1700000000000
      }
    ]
  },
  "error": null,
  "request_id": "req_001",
  "ts": 1700000000000
}
```

---

## 21.2 查询单个配置

```http
GET /api/v1/configs/{name}
```

响应：

```json
{
  "success": true,
  "data": {
    "id": "cfg_001",
    "name": "server",
    "config_type": "SYSTEM",
    "format": "TOML",
    "content": "...",
    "checksum": "abc123",
    "created_at": 1700000000000,
    "updated_at": 1700000000000
  },
  "error": null,
  "request_id": "req_001",
  "ts": 1700000000000
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

所有 WebSocket 消息使用统一 envelope。

```json
{
  "id": "msg_001",
  "type": "Subscribe",
  "channel": "system",
  "run_id": "run_001",
  "ts": 1700000000000,
  "data": {}
}
```

字段说明：

| 字段      | 说明      |
| ------- | ------- |
| id      | 消息 ID   |
| type    | 消息类型    |
| channel | 频道      |
| run_id  | 可选运行 ID |
| ts      | 时间戳     |
| data    | 消息数据    |

---

# 22.3 客户端消息类型

```text
Subscribe
Unsubscribe
StartStrategy
StopStrategy
UpdateParams
ReplayControl
Ping
```

---

# 22.4 服务端消息类型

```text
Subscribed
Unsubscribed
CommandAck
CommandRejected
MarketEvent
OrderEvent
FillEvent
PositionEvent
PortfolioEvent
AccountEvent
RiskEvent
SystemEvent
ReplayEvent
Pong
Error
```

---

# 22.5 WebSocket Channel

```text
market
orders
fills
positions
crypto_positions
portfolio
accounts
risk
metrics
system
replay
```

---

# 22.6 Subscribe

客户端发送：

```json
{
  "id": "msg_001",
  "type": "Subscribe",
  "channel": "system",
  "run_id": null,
  "ts": 1700000000000,
  "data": {
    "channels": [
      "market",
      "orders",
      "fills",
      "positions",
      "portfolio",
      "risk",
      "system",
      "replay"
    ],
    "run_id": "run_001"
  }
}
```

服务端响应：

```json
{
  "id": "msg_001",
  "type": "Subscribed",
  "channel": "system",
  "run_id": "run_001",
  "ts": 1700000000000,
  "data": {
    "channels": [
      "market",
      "orders",
      "fills",
      "positions",
      "portfolio",
      "risk",
      "system",
      "replay"
    ]
  }
}
```

---

# 22.7 Unsubscribe

客户端发送：

```json
{
  "id": "msg_002",
  "type": "Unsubscribe",
  "channel": "system",
  "run_id": "run_001",
  "ts": 1700000000000,
  "data": {
    "channels": [
      "market",
      "orders"
    ]
  }
}
```

---

# 22.8 Ping / Pong

客户端发送：

```json
{
  "id": "msg_ping_001",
  "type": "Ping",
  "channel": "system",
  "run_id": null,
  "ts": 1700000000000,
  "data": {}
}
```

服务端响应：

```json
{
  "id": "msg_ping_001",
  "type": "Pong",
  "channel": "system",
  "run_id": null,
  "ts": 1700000000100,
  "data": {}
}
```

---

# 23. WebSocket 控制消息

---

## 23.1 StartStrategy

```json
{
  "id": "msg_003",
  "type": "StartStrategy",
  "channel": "system",
  "run_id": null,
  "ts": 1700000000000,
  "data": {
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
    }
  }
}
```

服务端响应：

```json
{
  "id": "msg_003",
  "type": "CommandAck",
  "channel": "system",
  "run_id": "run_001",
  "ts": 1700000000000,
  "data": {
    "command": "StartStrategy",
    "status": "ACCEPTED",
    "strategy_id": "strategy_001",
    "run_id": "run_001"
  }
}
```

---

## 23.2 StopStrategy

```json
{
  "id": "msg_004",
  "type": "StopStrategy",
  "channel": "system",
  "run_id": "run_001",
  "ts": 1700000000000,
  "data": {
    "strategy_id": "strategy_001",
    "cancel_open_orders": true,
    "reason": "manual stop"
  }
}
```

---

## 23.3 UpdateParams

```json
{
  "id": "msg_005",
  "type": "UpdateParams",
  "channel": "system",
  "run_id": "run_001",
  "ts": 1700000000000,
  "data": {
    "strategy_id": "strategy_001",
    "params": {
      "fast": 30,
      "slow": 90,
      "target_percent": "0.4"
    }
  }
}
```

---

## 23.4 ReplayControl

```json
{
  "id": "msg_006",
  "type": "ReplayControl",
  "channel": "replay",
  "run_id": "run_001",
  "ts": 1700000000000,
  "data": {
    "action": "pause"
  }
}
```

```json
{
  "id": "msg_007",
  "type": "ReplayControl",
  "channel": "replay",
  "run_id": "run_001",
  "ts": 1700000000000,
  "data": {
    "action": "seek",
    "target_ts": 1704070800000
  }
}
```

```json
{
  "id": "msg_008",
  "type": "ReplayControl",
  "channel": "replay",
  "run_id": "run_001",
  "ts": 1700000000000,
  "data": {
    "action": "speed",
    "speed": "50x"
  }
}
```

---

# 24. WebSocket 推送消息

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
Client Subscribe
  ↓
Server Subscribed
  ↓
Server Push Events
  ↓
Client Ping
  ↓
Server Pong
  ↓
Client Unsubscribe
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
