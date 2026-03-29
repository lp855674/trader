# trader

量化交易后端（Rust），架构见 [`docs/specs/2026-03-29-quant-backend-architecture-design.md`](docs/specs/2026-03-29-quant-backend-architecture-design.md)，实现计划见 [`docs/superpowers/plans/2026-03-29-quantd-mvp-implementation-plan.md`](docs/superpowers/plans/2026-03-29-quantd-mvp-implementation-plan.md)。

## 构建与测试

```bash
cargo test
cargo run -p quantd
```

默认在启动时迁移数据库、写入 MVP seed，并对四个 `Venue` 各跑一轮 **mock ingest + paper 下单**，然后监听 HTTP。

环境变量：

- `QUANTD_DATABASE_URL` — SQLite 连接串（默认 `sqlite:quantd.db`）
- `QUANTD_HTTP_BIND` — 监听地址（默认 `127.0.0.1:8080`）
- `RUST_LOG` — 如 `info`

## API

- `GET /health`
- `GET /v1/instruments`
- `GET /v1/stream` — WebSocket；连接后先发 `hello`，随后推送 `order_cycle_done` 等事件（含 `event_id`）。
