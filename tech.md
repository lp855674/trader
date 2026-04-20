# trader 技术说明

## 产品

- 独立量化后端 **`quantd`**：ingest → 策略 → 风控（MVP 全放行）→ paper 执行 → SQLite 台账。
- 对外 **HTTP**（`/health`, `/v1/instruments`）与 **WebSocket**（`/v1/stream`）。
- 半自动控制面新增：`/v1/runtime/mode` 与 `/v1/runtime/allowlist`。
- 半自动单轮调度接口：`POST /v1/runtime/cycle` 与 `GET /v1/runtime/cycle/latest`。
- 终端交易面新增统一入口 **`trader`**：同时提供 CLI 子命令与全屏 TUI，作为 `quantd` 的 HTTP / WS 客户端工作。

## 流水线参数

- `pipeline::VenueTickParams`：单次 `run_one_tick_for_venue` 的账户、标的 `symbol`、时间戳；与 `ingest` / `exec` 解耦。
- `run_one_tick_for_venue` 返回 `Result<Option<exec::OrderAck>, PipelineError>`：策略未产出信号时为 `Ok(None)`；下单成功则为 `Ok(Some(ack))`（供 HTTP/WS 推送 `order_id`）。

## Crate 边界

| crate      | 职责 |
|------------|------|
| `domain`   | 纯类型 |
| `config`   | 环境变量配置 |
| `db`       | SQLite + 迁移 + 仓储函数（`NewBar` / `NewOrder` / `NewFill` 封装写入） |
| `ingest`   | `IngestAdapter` 与 mock 实现 |
| `longbridge_adapters` | Longbridge：`QuoteContext` K 线 ingest、`TradeContext` 实盘下单（MO） |
| `exec`     | `ExecutionAdapter`、`PaperAdapter`、`ExecutionRouter` |
| `strategy` | 策略 trait 与 MVP 规则策略 |
| `pipeline` | ingest → 策略 → 风控 → 执行（`quantd` 与 `api` 共用） |
| `api`      | axum 路由 |
| `quantd`   | 二进制；集成测试通过 `quantd` lib 重导出 `pipeline` |
| `marketdata` | 研究/离线数据处理（Polars DataFrame 与 `NormalizedBar` 对齐） |
| `terminal_core` | 终端共享模型与错误映射 |
| `terminal_client` | `quantd` HTTP / WebSocket 客户端 |
| `terminal_tui` | 终端多面板状态机、表单与渲染骨架 |
| `trader` | 统一 CLI / TUI 二进制入口 |

## Workspace 依赖

- 内部 crate（`domain`、`db`、`pipeline` 等）在根 `Cargo.toml` 的 `[workspace.dependencies]` 中以 `path = "crates/…"` 声明一次；各 member 使用 `name.workspace = true`，避免重复 path。
- **库入口路径（与内部其它 Rust 工程对齐）**：各 library crate 在 `Cargo.toml` 中显式写 `[lib] path = "src/<crate 名>.rs"`（例如 `domain` → `src/domain.rs`，`longbridge_adapters` → `src/longbridge_adapters.rs`），二进制 `quantd` 使用 `[[bin]] path = "src/main.rs"` 与 `[lib] path = "src/quantd.rs"`。工作区 `edition` 为 **2024**（见根 `Cargo.toml`）。

## WebSocket（规格 §7.1 对齐）

- 连接后首帧：`{"kind":"hello","schema_version":1}`。
- 业务事件示例：`{"event_id":"<uuid>","kind":"order_created","payload":{"order_id":"…","venue":"US_EQUITY","symbol":"…"}}`。
- 错误帧示例（`error_code` 仅出现在 `kind: error`）：`{"event_id":"<uuid>","kind":"error","error_code":"execution_not_configured","message":"…"}`。HTTP JSON 错误体同样使用 `error_code` 字段（与 WS 命名一致）。

## 配置（MVP）

- `QUANTD_DATABASE_URL`：默认 `sqlite:quantd.db`
- `QUANTD_HTTP_BIND`：默认 `127.0.0.1:8080`
- `QUANTD_API_KEY`：若设置，则 `/v1/*`（含 WebSocket `/v1/stream`）需要鉴权；支持 `Authorization: Bearer <key>` 或 `X-API-Key: <key>`。
- `QUANTD_ENV`：默认 `dev`；`prod` 下默认不写入 MVP seed，除非 `QUANTD_ALLOW_SEED`。
- `QUANTD_ALLOW_SEED`：`1/true/yes` 允许在 `prod` 写入 seed 并跑启动 tick。

### Longbridge（可选）

- 凭证：`LONGBRIDGE_APP_KEY`、`LONGBRIDGE_APP_SECRET`、`LONGBRIDGE_ACCESS_TOKEN`（见 [官方快速开始](https://open.longbridge.com/zh-CN/docs/getting-started)）。
- `quantd` 在三个变量均非空时 `LongbridgeClients::connect()`；成功则 `ensure_longbridge_live_account`，注册 `acc_lb_live` → `LongbridgeTradeAdapter`，并对 US/HK venue 使用 `LongbridgeCandleIngest`（否则回退 mock）。
- 若已配置 Longbridge 凭证但连接失败，`quantd` 会强制写回运行模式 `observe_only`，并记录一条 `reconciliation_snapshots.status = broker_connect_failed` 的失败快照。
- Paper 账户路径不变；Longbridge 错误在 API 层可表现为 `PipelineError::Exec(ExecError::Longbridge(..))` → HTTP 502、`error_code: broker_error`。

## 运行控制面

- 默认运行模式：`observe_only`（首次启动且 DB 中尚未写入 `runtime_controls.mode` 时自动补齐）。
- 允许模式：`enabled`、`observe_only`、`paper_only`、`degraded`。
- `GET /v1/runtime/mode`：读取当前模式；缺省值按 `observe_only` 返回。
- `PUT /v1/runtime/mode`：写入运行模式，非法值返回 HTTP 400。
- `GET /v1/runtime/allowlist`：返回标的 allowlist 与 `enabled` 标志。
- `PUT /v1/runtime/allowlist`：整表替换 allowlist；空 symbol 拒绝写入。
- `POST /v1/runtime/cycle`：按 allowlist 执行一轮 universe ingest → score → rank；`enabled/paper_only` 才会继续执行下单。
- execution guard：`enabled/paper_only` 下，accepted symbol 在真正执行前还会检查稳定 `idempotency_key`、本地未完成订单、symbol cooldown 与本地同向持仓；命中时仅记 `skipped.reason`，不再重复下单。
- `GET /v1/runtime/cycle/latest`：读取最近一轮结果，当前持久化落在 `system_config.key = runtime.last_cycle`。
- `GET /v1/runtime/cycle/history`：读取最近多轮结构化历史；底层使用 `runtime_cycle_runs` / `runtime_cycle_symbols`。
- `GET /v1/runtime/execution-state`：按 `account_id` 返回本地持仓、未完成订单，以及最近一轮 cycle 的执行摘要（`accepted` / `placed` / `skipped`）。
- `GET /v1/runtime/reconciliation/latest`：按 `account_id` 返回当前运行模式、本地持仓、本地未完成订单，以及最近一次 `reconciliation_snapshots` 快照。
- `POST /v1/orders`：显式提交 terminal 订单；当前仅支持限价单，走 execution router 而不是 `POST /v1/tick`。
- `POST /v1/orders/:order_id/cancel`：终端撤单；请求体提供 `account_id`。
- `POST /v1/orders/:order_id/amend`：终端改价改量；请求体提供 `account_id`、`qty` 与可选 `limit_price`。
- `GET /v1/terminal/overview`：返回 terminal 主屏聚合数据：runtime mode、watchlist、positions、open orders。
- `GET /v1/quotes/:symbol`：返回单标的 terminal quote 视图与最近 bars。
- `quantd` 可选后台 loop：`QUANTD_UNIVERSE_LOOP_ENABLED=1` 时按固定间隔触发 `run_universe_cycle`；默认关闭。
- `QUANTD_EXEC_SYMBOL_COOLDOWN_SECS`：同账户、同 instrument、同方向的最小重复下单间隔；默认 `300` 秒。

## 股票数据（研究侧：Polars / Rust）

- 运行期 K 线仍以 SQLite `bars` + `domain::NormalizedBar` 为准；**离线/研究** 以 **Rust 版 Polars** 作为 OHLCV 的基础表示。
- `crates/marketdata` 提供 `BarsFrame`：DataFrame 列与 `NormalizedBar` 一致：`ts_ms`, `open`, `high`, `low`, `close`, `volume`。
- 若需多标的面板，可在 DataFrame 上增加 `symbol`（Utf8）列，与 `ts_ms` 联合使用（目前未在 `BarsFrame` 强制建模）。

## 非目标（当前）

- gRPC、Qlib 在线路径；除 Longbridge 外的其它券商/交易所接入。
