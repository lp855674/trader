# trader 技术说明

## 产品

- 独立量化后端 **`quantd`**：ingest → 策略 → 风控（MVP 全放行）→ paper 执行 → SQLite 台账。
- 对外 **HTTP**（`/health`, `/v1/instruments`）与 **WebSocket**（`/v1/stream`）。

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
| `exec`     | `ExecutionAdapter`、`PaperAdapter`、`ExecutionRouter` |
| `strategy` | 策略 trait 与 MVP 规则策略 |
| `pipeline` | ingest → 策略 → 风控 → 执行（`quantd` 与 `api` 共用） |
| `api`      | axum 路由 |
| `quantd`   | 二进制；集成测试通过 `quantd` lib 重导出 `pipeline` |

## Workspace 依赖

- 内部 crate（`domain`、`db`、`pipeline` 等）在根 `Cargo.toml` 的 `[workspace.dependencies]` 中以 `path = "crates/…"` 声明一次；各 member 使用 `name.workspace = true`，避免重复 path。

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

## 非目标（当前）

- 真实 live 券商/交易所接入、gRPC、Qlib 在线路径。
