# trader 技术说明

## 产品

- 独立量化后端 **`quantd`**：ingest → 策略 → 风控（MVP 全放行）→ paper 执行 → SQLite 台账。
- 对外 **HTTP**（`/health`, `/v1/instruments`）与 **WebSocket**（`/v1/stream`）。

## Crate 边界

| crate     | 职责 |
|-----------|------|
| `domain`  | 纯类型 |
| `config`  | 环境变量配置 |
| `db`      | SQLite + 迁移 + 仓储函数 |
| `ingest`  | `IngestAdapter` 与 mock 实现 |
| `exec`    | `ExecutionAdapter`、`PaperAdapter`、`ExecutionRouter` |
| `strategy`| 策略 trait 与 MVP 规则策略 |
| `api`     | axum 路由 |
| `quantd`  | 二进制 + `pipeline` 库 |

## 配置（MVP）

- `QUANTD_DATABASE_URL`：默认 `sqlite:quantd.db`
- `QUANTD_HTTP_BIND`：默认 `127.0.0.1:8080`

## 非目标（当前）

- 真实 live 券商/交易所接入、gRPC、Qlib 在线路径。
