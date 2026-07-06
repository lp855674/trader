# api 技术文档

## 职责

`crates/api` 是 Axum 控制面和查询面，负责 REST / WebSocket router、AppState、运行启动/控制、read model 转 response、事件广播和服务端日志保留任务。

## 关键实现

- `router()` 提供基础 health；`router_with_state(state)` 注册 `/api/v1/*` 控制面和查询面。
- `AppState` 持有 `storage::Db`、server config、runtime manager、event bus 和必要装配信息。
- 运行启动端点包括 `POST /api/v1/backtests`、`/paper-runs`、`/replays`、`/live-runs`；请求必须提供明确 run config 来源，handler 会组装 runtime 并把运行结果和状态写回 storage。
- Live run 的公共 API shape 保持在 run/status/stop 控制面，内部可由 runtime supervisor/worker protocol 维护进程隔离和终态回退。
- 查询接口读取 storage read model 后映射成 API-owned response struct。
- run-scoped 查询优先使用 `/api/v1/runs/{run_id}/...`，覆盖 orders、fills、positions、balances、snapshots、metrics、events、order-events、risk-events、insights、portfolio-targets、crypto-positions、reconciliation 和 system logs。
- reference/config/ops 查询覆盖 fee rules、funding rates、crypto market meta、corporate actions、ingestion status、config lifecycle、system logs 和 logging metrics。
- WebSocket `/ws` 支持按 `run_id` 订阅事件，以及对 active replay run 发送 pause/resume/seek/speed 控制。

## 输入输出与持久化

输入来自 HTTP request、WebSocket 控制和 server config；输出是 API response、WebSocket event 和 runtime command。API 不直接拼 SQL，所有持久化必须走 `storage::Db` 语义方法。

## 边界与约束

- API response 不得直接暴露 storage record。
- API 不得绕过 Runtime、Risk、Execution、OMS 提供手动实盘下单通道。
- handler 可以装配 runtime，但业务规则必须留在 crate 中。
- 真实 broker/paper 外部送单必须受 config/env 和显式开关保护。
- 事件查询返回 API-owned JSON response；payload 对外应是结构化 JSON，不应把 JSON 再双重编码成字符串。
- 兼容性 top-level 查询可以保留，但新集成应优先使用显式 run scope，避免隐式 run ownership。

## 测试与验证

修改 endpoint 需要同步本文件和相关模块文档，并覆盖 response shape、错误路径、运行控制和 WebSocket 消费路径。
