# trader-server 技术文档

## 职责

`apps/trader-server` 是 HTTP/WebSocket 服务进程入口，负责加载 server config、连接 SQLite、执行 migration、装配 API state 并启动 Axum server。

## 关键实现

- Tokio main 初始化 tracing。
- 配置路径优先级是 `TRADER_SERVER_CONFIG` -> `TRADER_CONFIG` -> `configs/deploy/trader-server.example.toml`。
- 数据库 URL 可由 `TRADER_DATABASE_URL` 覆盖 server config。
- SQLite 文件路径会先确保父目录存在。
- 启动时执行 `Db::connect`、`db.migrate()`，并创建日志保留 scheduler。
- bind 地址可由 `TRADER_SERVER_BIND` 覆盖。
- 最终使用 `api::router_with_state(state)` 服务请求。

## 输入输出与持久化

输入是环境变量和 server TOML；输出是监听中的 Axum 服务、SQLite migration 和后台日志保留任务。

## 边界与约束

- server main 只做进程装配，不写业务规则。
- server config 是 deployment 配置，不等同于单次 run identity。
- API state 必须持有明确 DB URL/config，不应依赖隐藏全局状态。

## 测试与验证

重点覆盖配置优先级、SQLite path 解析、migration 启动、bind 解析和 router health。

