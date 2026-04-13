# trader

量化交易后端（Rust），架构见 [`docs/specs/2026-03-29-quant-backend-architecture-design.md`](docs/specs/2026-03-29-quant-backend-architecture-design.md)，实现计划见 [`docs/superpowers/plans/2026-03-29-quantd-mvp-implementation-plan.md`](docs/superpowers/plans/2026-03-29-quantd-mvp-implementation-plan.md)。

## 构建与测试

```bash
cargo test
cargo run -p quantd
```

默认在启动时迁移数据库、按环境决定是否写入 MVP seed，并在缺省情况下把运行模式初始化为 `observe_only`，然后监听 HTTP。

环境变量：

- `QUANTD_DATABASE_URL` — SQLite 连接串（默认 `sqlite:quantd.db`）
- `QUANTD_HTTP_BIND` — 监听地址（默认 `127.0.0.1:8080`）
- `RUST_LOG` — 如 `info`
- `QUANTD_API_KEY` — 若设置，则 `/v1/*` 需要鉴权（`Authorization: Bearer <key>` 或 `X-API-Key: <key>`）
- `QUANTD_ENV` — 默认 `dev`；`prod` 下默认不写入 seed（除非 `QUANTD_ALLOW_SEED`）
- `QUANTD_ALLOW_SEED` — `1/true/yes` 允许在 `prod` 写入 seed 并跑启动 tick
- `QUANTD_UNIVERSE_LOOP_ENABLED` — `1/true/yes` 启用后台半自动 universe loop（默认关闭）
- `QUANTD_UNIVERSE_LOOP_INTERVAL_SECS` — 后台 loop 间隔秒数，默认 `60`
- `QUANTD_UNIVERSE_LOOP_VENUE` — 后台 loop 的 venue，默认 `US_EQUITY`
- `QUANTD_UNIVERSE_LOOP_ACCOUNT_ID` — 后台 loop 使用的账户，默认 `acc_mvp_paper`
- `QUANTD_EXEC_SYMBOL_COOLDOWN_SECS` — 同账户、同标的、同方向的最小重复下单间隔秒数，默认 `300`

### Longbridge（可选：真实行情 + 实盘下单）

在 [Longbridge OpenAPI 快速开始](https://open.longbridge.com/zh-CN/docs/getting-started) 开通应用凭证后，设置以下三个环境变量；**三者均非空**时 `quantd` 会连接 Longbridge，并为美股/港股 ingest 使用 K 线拉取（写入本地 `bars`），同时为账户 `acc_lb_live` 注册实盘执行适配器。

- `LONGBRIDGE_APP_KEY`、`LONGBRIDGE_APP_SECRET`、`LONGBRIDGE_ACCESS_TOKEN` — 用户中心「应用凭证」（传统 API Key；与 OAuth access token 不是同一种）

可选：

- `LONGBRIDGE_REGION` — 如 `cn`、`hk`，覆盖接入点（见官方文档）
- `QUANTD_LB_US_SYMBOL` — 美股标的 Longbridge 符号，默认 `AAPL.US`
- `QUANTD_LB_HK_SYMBOL` — 港股标的 Longbridge 符号，默认 `700.HK`

**行为说明：**

- **模拟盘**：`account_id` 为 `acc_mvp_paper`（或其它 paper 账户）时仍走本地 SQLite **PaperAdapter**，与 Longbridge 无关。
- **实盘**：`POST /v1/tick` 使用 `account_id: "acc_lb_live"` 时，会通过 Longbridge **`TradeContext::submit_order`** 下单；当前实现为**限价单（LO）**，与真实账户资金与持仓联动，**请仅在理解风险的前提下使用**。
- **失败护栏**：若已配置 Longbridge 凭证但启动时连接失败，`quantd` 会把运行模式写回 `observe_only`，并向 `reconciliation_snapshots` 追加一条 `broker_connect_failed` 记录。

## API

- `GET /health`
- `GET /v1/instruments`
- `GET /v1/orders?account_id=<id>` — 返回该账户订单列表（MVP paper 账户默认 `acc_mvp_paper`）
- `GET /v1/runtime/mode`
- `PUT /v1/runtime/mode` — 允许值：`enabled` / `observe_only` / `paper_only` / `degraded`
- `GET /v1/runtime/allowlist`
- `PUT /v1/runtime/allowlist`
- `POST /v1/runtime/cycle` — 基于当前 allowlist 手动跑一轮 universe 评分与筛选；`observe_only` 下只记结果不下单；`paper_only/enabled` 下也会先经过 execution guard，再决定是否真正下单
- `GET /v1/runtime/cycle/latest` — 查询最近一轮 universe 结果
- `GET /v1/runtime/cycle/history?limit=10` — 查询最近多轮 universe 历史
- `GET /v1/runtime/execution-state?account_id=<id>` — 查询当前本地持仓、未完成订单，以及最近一轮 cycle 的 accepted/placed/skipped 摘要
- `POST /v1/tick` — 对指定 `venue` + `symbol` 跑一轮 ingest → 策略 → 风控 → 模拟成交；若实际下单成功，会向 WebSocket 订阅者广播 `order_created`（含 `order_id` / `venue` / `symbol`）

`POST /v1/tick` 请求体示例：

```json
{
  "venue": "US_EQUITY",
  "symbol": "AAPL",
  "account_id": "acc_mvp_paper"
}
```

`account_id` 可省略，省略时与启动 seed 一致为 `acc_mvp_paper`。若已配置 Longbridge 且需**实盘**试单，将 `account_id` 设为 `acc_lb_live`（见上文风险说明）。

- `GET /v1/stream` — WebSocket；连接后先发 `hello`，随后推送 `order_created` 等事件（每帧含 `event_id`）；若出现业务/序列化等问题可收到 `kind: error` 帧（含 `error_code`），与规格 §7.1 一致。券商侧错误可能映射为 HTTP **502**、`error_code: broker_error`。

## Semi-Auto Operator Checklist

1. 启动 `services/lstm-service`
2. 验证 `GET http://127.0.0.1:8000/health`
3. 确认 `GET /v1/runtime/mode` 返回 `observe_only`
4. 通过 `PUT /v1/runtime/allowlist` 设置本轮允许交易的标的
5. 手动触发一轮 `POST /v1/tick` 或后续批量调度，检查日志与 `signals` / `risk_decisions`
6. 检查 `reconciliation_snapshots`，确认没有新的失败状态
7. 仅在日志、本地账、本地控制面与券商状态一致时，再切到 `enabled` 或 `paper_only`
8. 若需半自动运行，再开启 `QUANTD_UNIVERSE_LOOP_ENABLED=1`

### Execution Guard Notes

- `accepted` 不等于一定 `placed`；执行前还会检查幂等 key 与 symbol cooldown
- 当前可能出现的 `skipped.reason` 包括：
  - `guard_duplicate_idempotency`
  - `guard_open_order_exists`
  - `guard_cooldown_active`
  - `guard_same_direction_position_open`
- `GET /v1/runtime/execution-state` 适合做半自动值守面板的数据源：先看 `open_orders` / `positions`，再看 `latest_cycle.skipped`
