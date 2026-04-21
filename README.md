# trader

量化交易后端（Rust）。当前主运行时是 `quantd`，终端入口是 `trader`，模型侧目前仍位于 `services/lstm-service`，后续会收敛到 `services/model`。

架构与边界设计：

- [`docs/superpowers/specs/2026-04-21-quantd-paper-and-model-boundary-design.md`](docs/superpowers/specs/2026-04-21-quantd-paper-and-model-boundary-design.md)

当前实现计划：

- [`docs/superpowers/plans/2026-04-21-quantd-paper-boundary-plan.md`](docs/superpowers/plans/2026-04-21-quantd-paper-boundary-plan.md)

## 构建与测试

```bash
cargo test
cargo run -p quantd
```

## Terminal CLI

```bash
cargo run -p trader -- tui
cargo run -p trader -- quote AAPL.US
cargo run -p trader -- order submit --account-id acc_mvp_paper --symbol AAPL.US --side buy --qty 10 --limit-price 123.45
cargo run -p trader -- order cancel --account-id acc_mvp_paper --order-id <order-id>
cargo run -p trader -- order amend --account-id acc_mvp_paper --order-id <order-id> --qty 12 --limit-price 124
```

## Operator Runbook

操作与联调真源见 [`docs/runbook.md`](docs/runbook.md)。其中包含：

- `Paper Smoke`：手工 submit / amend / cancel、overview、execution-state、WebSocket、TUI
- `Model Workflow / Service`：模型服务启动、`/health`、模型存在性检查
- `LSTM Cycle Paper`：allowlist、runtime mode、cycle、history、execution-state、execution guard

## API Overview

主要接口：

- `GET /health`
- `GET /v1/instruments`
- `GET /v1/orders?account_id=<id>`
- `POST /v1/orders`
- `POST /v1/orders/:order_id/cancel`
- `POST /v1/orders/:order_id/amend`
- `GET /v1/runtime/mode`
- `PUT /v1/runtime/mode`
- `GET /v1/runtime/allowlist`
- `PUT /v1/runtime/allowlist`
- `POST /v1/runtime/cycle`
- `GET /v1/runtime/cycle/latest`
- `GET /v1/runtime/cycle/history?limit=10`
- `GET /v1/runtime/execution-state?account_id=<id>`
- `GET /v1/runtime/reconciliation/latest?account_id=<id>`
- `GET /v1/terminal/overview?account_id=<id>`
- `GET /v1/quotes/<symbol>`
- `POST /v1/tick`
- `GET /v1/stream`

接口行为与联调步骤以 [`docs/runbook.md`](docs/runbook.md) 为准。
