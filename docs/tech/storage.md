# storage 技术文档

## 职责

`crates/storage` 是 SQLite 和 repository 边界，负责 migration、交易状态、运行状态、订单、成交、账户、持仓、事件、日志、配置和 read model。

## 关键实现

- `Db` 持有 `SqlitePool` 并执行 migrations。
- migrations 位于仓库根 `migrations/`，从 `0001_init.sql` 到当前 `0011_fee_rule_volume_window.sql`，并包含部分幂等 alter 逻辑。
- SQLite 当前覆盖 `strategy_runs`、`instruments`、`orders`、`fills`、`positions`、`account_balances`、`portfolio_snapshots`、`event_store`、audit projections、market rules、contract accounting、reference snapshots、config lifecycle、system logs 和 fee tier/volume window。
- `repositories.rs` 暴露语义 command/read model 方法，覆盖 backtest、paper、live、order/fill、portfolio、events、logs、ingestion、fee rules、broker status 等。
- `DbSystemLogSink` 支持 events/log writer 将 tracing 日志写库。
- `event_store` 是不可变审计事件真源；`order_events`、`risk_events`、`insights`、`portfolio_targets` 是从事件/运行链路派生出的查询投影。
- `configs`、`config_releases`、`run_config_versions`、`config_audits` 支持 run config snapshot、config lifecycle、diff、rollback 和 run-version 绑定。

## 输入输出与持久化

输入是各 runtime/API 传入的 command；输出是 read model、状态快照和查询结果。SQLite 是交易状态和审计事件真源，Parquet/feature 文件属于 `data` 和 `feature_store` 边界。

## 边界与约束

- `sqlx` 和 SQL 只能出现在 storage 边界内。
- 边界外不能透传 `SqlitePool`、拼 SQL 或构造内部写入 DTO。
- 写入必须走语义 command，读取必须返回明确 read model。
- 金额/数量进入 SQLite 时必须有明确字符串/精度策略。
- 一个表已经存在只表示 storage boundary 可用，不表示所有 runtime 都会自动写入该表；自动写入路径必须在对应 runtime/module 文档中说明。
- 状态恢复以 run、orders、fills、positions/balances/snapshots、event projections、broker open orders/executions 和 system logs 的组合为依据，不能只依赖内存状态。

## 测试与验证

修改 schema 必须同步 migration、repository、本文件和恢复路径。重点覆盖 migration 幂等、command/read model、事件查询和状态恢复。
