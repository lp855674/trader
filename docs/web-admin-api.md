# Web 管理页接口文档

Version: v1.0  
Status: Draft  
Source of Truth: `crates/api/src/api.rs`

---

## 1. 目标

本文只给 Web 管理页使用，按“页面 -> 接口 -> 数据字段”组织，方便前端直接对接。

---

## 2. 基本约定

- Base URL: `http://127.0.0.1:8080`
- API Prefix: `/api/v1`
- WebSocket: `ws://127.0.0.1:8080/ws`
- 当前接口多数返回裸 JSON，不使用统一 envelope。
- 金额、数量、PnL 等精度字段统一按字符串处理。

---

## 3. 页面映射

| 页面 | 主要接口 |
| --- | --- |
| 总览 | `/health` `/metrics` `/runs/{run_id}/metrics` `/brokers/status` `/brokers/account/{account_id}` `/ingestion/status` `/ops/logging/metrics` `/runs` |
| 运行管理 | `/preflight/paper` `/backtests` `/paper-runs` `/replays` `/live-runs` `/runs/{run_id}` `/runs/{run_id}/status` `/runs/{run_id}/cancel` `/live-runs/{run_id}/stop` |
| Replay 控制 | `/replay/{run_id}/pause` `/replay/{run_id}/resume` `/replay/{run_id}/seek/{offset}` `/replay/{run_id}/speed/{speed}` |
| 订单/成交/持仓 | `/runs/{run_id}/orders` `/runs/{run_id}/fills` `/runs/{run_id}/positions` `/runs/{run_id}/account-balances` `/runs/{run_id}/portfolio-snapshots` `/runs/{run_id}/cash-snapshots` `/runs/{run_id}/position-snapshots` |
| 风控/审计 | `/runs/{run_id}/reconciliation` `/reconciliation-drifts` `/reconciliation-alerts/summary` `/reconciliation-alert-deliveries/summary` `/runs/{run_id}/risk-events` |
| 配置管理 | `/configs` `/configs/{name}` `/configs/{name}/latest` `/configs/{name}/published` `/configs/{name}/{version}` `/configs/{name}/{version}/state` `/configs/{name}/{version}/rollback` `/config-approvals/pending` `/configs/{name}/diff` `/configs/{config_id}/releases` `/configs/{config_id}/audits` `/runs/{run_id}/config-version` |
| 日志中心 | `/logs` `/system-logs` `/runs/{run_id}/system-logs` `/events` `/runs/{run_id}/events` `/runs/{run_id}/order-events` `/runs/{run_id}/risk-events` `/runs/{run_id}/insights` `/runs/{run_id}/portfolio-targets` |
| 实时看板 | `/ws` |

---

## 4. 总览页

### 4.1 健康检查

`GET /api/v1/health`

用于页面首屏探活。

### 4.2 指标

`GET /api/v1/metrics`

推荐在多运行场景使用 `GET /api/v1/runs/{run_id}/metrics`。顶层 `/metrics` 目前仍保留兼容，但语义仍绑定当前 server 加载的配置。

用于展示订单数、成交数、权益曲线摘要等。
返回重点字段：`order_count`、`fill_count`、`total_return`、`sharpe`、`sortino`、`max_drawdown`、`win_rate`。

### 4.3 Broker 状态

`GET /api/v1/brokers/status`
`GET /api/v1/brokers/account/{account_id}`

用于展示当前可用 broker 和账户快照。
`brokers/status` 返回 broker 列表，`brokers/account/{account_id}` 返回单账户资金/持仓快照。

### 4.4 采集状态

`GET /api/v1/ingestion/status`

用于展示参考数据抓取进度。

### 4.5 日志写入状态

`GET /api/v1/ops/logging/metrics`

用于展示日志缓存、丢弃数、flush 配置。

---

## 5. 运行管理页

### 5.0 纸面预检

`GET /api/v1/preflight/paper`

用于在启动 Paper 前检查配置、broker、账户、风控开关和可用行情条数。

### 5.1 创建运行

- `POST /api/v1/backtests`
- `POST /api/v1/paper-runs`
- `POST /api/v1/replays`
- `POST /api/v1/live-runs`

前端建议把这四类做成统一“启动运行”表单，区别只在模式和少量参数。
`POST /api/v1/backtests` 返回回测摘要；`POST /api/v1/paper-runs` 和 `POST /api/v1/live-runs` 返回 `run_id` + `status`；`POST /api/v1/replays` 返回 replay 摘要。

### 5.2 运行列表/详情

- `GET /api/v1/runs`
- `GET /api/v1/runs/{run_id}`
- `GET /api/v1/runs/{run_id}/status`

`runs` 列表字段：`id`、`name`、`mode`、`status`、`started_at_ms`、`ended_at_ms`、`error`、`config`。
`runs/{run_id}/status` 只返回 `run_id`、`status`、`error`，适合轮询。

### 5.3 停止/取消

- `POST /api/v1/runs/{run_id}/cancel`
- `POST /api/v1/live-runs/{run_id}/stop`

---

## 6. Replay 控制

`POST /api/v1/replay/{run_id}/pause`
`POST /api/v1/replay/{run_id}/resume`
`POST /api/v1/replay/{run_id}/seek/{offset}`
`POST /api/v1/replay/{run_id}/speed/{speed}`

适合做成固定控制条。

---

## 7. 交易数据页

### 7.1 列表接口

- `GET /api/v1/runs/{run_id}/orders`
- `GET /api/v1/runs/{run_id}/fills`
- `GET /api/v1/runs/{run_id}/positions`
- `GET /api/v1/runs/{run_id}/account-balances`

字段重点：
- `orders`: `client_order_id`、`broker_order_id`、`symbol`、`side`、`order_type`、`price`、`qty`、`filled_qty`、`status`
- `fills`: `order_id`、`symbol`、`side`、`price`、`qty`、`fee`、`ts_ms`
- `positions`: `account_id`、`symbol`、`qty`、`avg_price`、`updated_at_ms`
- `account-balances`: `account_id`、`asset`、`total`、`available`、`frozen`

### 7.2 快照接口

- `GET /api/v1/runs/{run_id}/portfolio-snapshots`
- `GET /api/v1/runs/{run_id}/cash-snapshots`
- `GET /api/v1/runs/{run_id}/position-snapshots`

建议默认按 `run_id` 或时间区间过滤，不要一次性拉全量。
`cash-snapshots` 支持 `currency/from_ms/to_ms`，`position-snapshots` 支持 `symbol/position_side/from_ms/to_ms`。

兼容说明：
- 顶层 `/orders` `/fills` `/positions` `/account-balances` `/portfolio/snapshots` `/cash/snapshots` `/positions/snapshots` 仍可用，但只适合当前单配置本地验证链路。
- Web 管理页接入多 run 时，应统一改用显式 `runs/{run_id}` 路由。

---

## 8. 风控与审计页

### 8.1 Reconciliation

- `GET /api/v1/runs/{run_id}/reconciliation`
- `GET /api/v1/runs/{run_id}/reconciliation-drifts`
- `GET /api/v1/reconciliation-drifts`

`reconciliation` 返回 `status`、`cash_snapshots`、`position_snapshots`、`drift_events`、`latest_cash_ts_ms`、`latest_position_ts_ms`。

### 8.2 告警摘要

- `GET /api/v1/reconciliation-alerts/summary`
- `GET /api/v1/runs/{run_id}/reconciliation-alerts/summary`
- `GET /api/v1/reconciliation-alert-deliveries/summary`
- `GET /api/v1/runs/{run_id}/reconciliation-alert-deliveries/summary`

### 8.3 风险事件

- `GET /api/v1/runs/{run_id}/risk-events`

---

## 9. 配置管理页

### 9.1 配置浏览

- `GET /api/v1/configs`
- `GET /api/v1/configs/{name}`
- `GET /api/v1/configs/{name}/latest`
- `GET /api/v1/configs/{name}/published`
- `GET /api/v1/configs/{name}/{version}`
- `GET /api/v1/runs/{run_id}/config-version`

`configs/{name}` 返回同名配置版本列表；`configs/{name}/latest` 和 `published` 返回单个版本；`runs/{run_id}/config-version` 返回该 run 绑定的配置版本。

### 9.2 配置编辑

- `POST /api/v1/configs`
- `PUT /api/v1/configs/{name}/{version}/state`
- `POST /api/v1/configs/{name}/{version}/rollback`
- `GET /api/v1/configs/{name}/diff?v1=1&v2=2`

创建请求字段：`name`、`content`、`created_by`、`parent_version`、`target_env`、`rollout`。
状态变更请求字段：`new_state`、`changed_by`、`actor_role`、`reason`。
Rollback 请求字段：`actor`、`reason`。

### 9.3 审批与审计

- `GET /api/v1/config-approvals/pending`
- `GET /api/v1/configs/{config_id}/releases`
- `GET /api/v1/configs/{config_id}/audits`

---

## 10. 日志中心

### 10.1 分页日志

`GET /api/v1/logs`

返回 `{ logs, total, limit, offset }`，适合表格分页。
支持 `run_id`、`level`、`target`、`from_ms`、`to_ms`、`search`、`limit`、`offset`。

### 10.2 原始日志

`GET /api/v1/system-logs`
`GET /api/v1/runs/{run_id}/system-logs`

适合详情页和实时 tail。

---

## 11. WebSocket

### 11.1 连接

`ws://127.0.0.1:8080/ws`

### 11.2 建议订阅

- `system`
- `orders`
- `fills`
- `positions`
- `portfolio`
- `risk`
- `replay`
- `crypto_positions`

用于实时刷新运行状态、成交、持仓和 Replay 控制状态。

### 11.3 消息类型

- 客户端：`Subscribe`、`Unsubscribe`、`Ping`、`ReplayControl`
- 服务端：`Subscribed`、`CommandAck`、`MarketEvent`、`OrderEvent`、`FillEvent`、`PositionEvent`、`PortfolioEvent`、`AccountEvent`、`RiskEvent`、`SystemEvent`、`ReplayEvent`、`Pong`、`Error`

---

## 12. 前端实现建议

- 列表页统一做分页、筛选、排序。
- 运行详情页按 `run_id` 聚合订单、成交、日志、风控和快照。
- 长列表优先走 `logs`，不要默认拉 `system-logs` 全量。
- 需要实时性的区域走 WebSocket，其他区域走 REST。
