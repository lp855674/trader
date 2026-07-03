# Trader Design Docs

本目录保存 Trader 新设计稿。文档按主题分工维护，避免同一设计在多个文件重复描述。

## 文档地图

| 文档 | 内容边界 |
| --- | --- |
| `architecture.md` | 总体目标、核心原则、分层架构、跨模块边界、V1 范围。 |
| `crates.md` | Rust workspace、crate 职责、依赖方向、feature flags、测试策略。 |
| `database.md` | SQLite 表、Parquet schema、repository、migration、状态恢复。 |
| `api.md` | REST / WebSocket 端点、消息格式、错误码、安全设计。 |
| `web-admin-api.md` | Web 管理页页面映射、常用接口、轮询与实时更新约定。 |
| `events.md` | Event envelope、事件分类、事件流、事件持久化。 |
| `strategy.md` | Strategy trait、StrategyContext、信号模型、策略边界。 |
| `broker.md` | Broker trait、路由、订单/成交/持仓映射、回报、重连、限流。 |
| `paper-readiness-runbook.md` | 本地 paper-readiness 门禁、IBKR Gateway 验证步骤和 failure_class 排查。 |
| `分析.md` | 当前实现差距分析、生产化差距跟踪表和下一步验证优先级。 |
| `superpowers/plans/2026-07-03-ibkr-paper-gateway-long-run-verification.md` | IBKR paper Gateway ReadOnly / AutoRun / Soak 长跑验证计划。 |
| `ibkr-paper-gateway-long-run-results-paper-readiness-afc967981176.md` | IBKR Gateway 验证结果骨架和本地 readiness evidence。 |
| `roadmap.md` | 阶段目标、MVP 范围、发布计划。 |

## 维护规则

- 架构总览只写跨模块原则，不展开表结构、API payload、Cargo.toml 或完整 Rust trait。
- API 端点、请求响应、WebSocket 消息只维护在 `api.md`。
- SQLite 表与 Parquet schema 只维护在 `database.md`。
- Event 类型只维护在 `events.md`；其它文档只引用事件流名称。
- crate 划分和依赖方向只维护在 `crates.md`。
- 策略和 Broker 的强约束可以在总览中简述，但完整接口只放在各自专题文档。
- Roadmap 只描述阶段和优先级，不重复模块接口和表设计。

## 当前整理状态

已将 `architecture.md` 收敛为总览，删除其中与 `api.md`、`database.md`、`events.md`、`crates.md`、`broker.md` 重复的大段内容。
