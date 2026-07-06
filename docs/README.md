# Trader Design Docs

本目录保存 Trader 设计与运维文档。当前实现的模块技术事实以 `tech/*.md` 为准，旧版总览/专题文档已归档。

## 文档地图

| 文档 | 内容边界 |
| --- | --- |
| `../tech.md` | 跨模块技术规则、运行链路、边界约束和模块文档索引。 |
| `tech/*.md` | 各 crate/app 的当前实现技术文档，以代码核对后的事实为准。 |
| `tech/api.md` | REST / WebSocket router、AppState、运行控制、查询接口和 response 映射。 |
| `tech/storage.md` | SQLite migration、repository、command/read model、审计与恢复边界。 |
| `tech/data.md` | 行情模型、CSV/Parquet、外部元数据、资金费率和公司行为采集。 |
| `tech/feature_store.md` | Feature record、manifest、Parquet store 和研究特征契约。 |
| `tech/events.md` | Event envelope、事件分类、event bus、typed events 和结构化日志边界。 |
| `tech/strategies.md` | 策略 registry、内置策略、feature gate/rank 和多 alpha 装配。 |
| `tech/alpha.md` | AlphaModel、信号组合、冲突处理和确定性聚合。 |
| `tech/broker.md` | Broker trait、fake/simulated、Binance Spot Testnet、IBKR paper adapter。 |
| `web-admin-api.md` | Web 管理页页面映射、常用接口、轮询与实时更新约定。 |
| `paper-readiness-runbook.md` | 本地 paper-readiness 门禁、IBKR Gateway 验证步骤和 failure_class 排查。 |
| `分析.md` | 当前实现差距分析、生产化差距跟踪表和下一步验证优先级。 |
| `superpowers/plans/2026-07-03-ibkr-paper-gateway-long-run-verification.md` | IBKR paper Gateway ReadOnly / AutoRun / Soak 长跑验证计划。 |
| `ibkr-paper-gateway-long-run-results-paper-readiness-afc967981176.md` | IBKR Gateway 验证结果骨架和本地 readiness evidence。 |
| `roadmap.md` | 阶段目标、MVP 范围、发布计划。 |
| `archive/legacy/*.md` | 已归档旧版设计文档，仅作历史参考，不作为当前实现真源。 |

## 维护规则

- 架构总览只写跨模块原则，不展开表结构、API payload、Cargo.toml 或完整 Rust trait。
- API 端点、请求响应、WebSocket 消息只维护在 `tech/api.md`。
- SQLite 表与 repository 边界只维护在 `tech/storage.md`；Parquet 行情与特征边界只维护在 `tech/data.md` 和 `tech/feature_store.md`。
- Event 类型只维护在 `tech/events.md`；其它文档只引用事件流名称。
- crate 划分、依赖方向和模块职责只维护在 `../tech.md` 与 `tech/*.md`。
- 策略、Alpha 和 Broker 的强约束可以在总览中简述，但完整当前实现只放在对应 `tech/*.md`。
- Roadmap 只描述阶段和优先级，不重复模块接口和表设计。

## 当前整理状态

已将旧版 `architecture.md`、`crates.md`、`database.md`、`api.md`、`events.md`、`strategy.md`、`broker.md` 中仍然正确的内容按代码现状合并进 `tech/*.md`，旧文件归档到 `archive/legacy/`。
