# data 技术文档

## 职责

`crates/data` 负责行情数据模型、历史数据加载、Parquet/CSV 转换、`MarketSlice` 和外部元数据/公司行为/资金费率采集。它不负责策略决策、风控或订单。

## 关键实现

- `Bar`、symbol bar、market slice 表达单标的和同一时间点多标的行情。
- CSV/Parquet loader 支持回测和研究数据输入。
- Binance metadata/funding、Yahoo corporate actions 等 ingestion 模块通过 HTTP 获取外部数据。
- ingestion tracker 将采集状态交给 storage 记录。
- HTTP retry 封装外部请求的重试策略。
- 大规模历史行情和研究数据应留在 Parquet 边界；SQLite 只保存交易状态、引用数据和采集状态等可恢复/可查询状态。

## 输入输出与持久化

输入是文件路径、配置的 data inputs 或外部 HTTP 源；输出是 bars、market slices、metadata、funding/corporate action record。SQLite 写入必须通过 `storage` 暴露的方法完成。

## 边界与约束

- 多标的运行优先使用 `MarketSlice`，单文件 source/path 只是兼容入口。
- 行情价格和数量在 Rust 内使用 `Decimal`。
- 数据采集不能直接影响交易决策，必须先落入可审计数据或由 runtime 明确装配。
- 旧数据库设计中的 tick、order book、open interest、fundamentals 等完整数据湖分区是目标方向；写入当前文档时必须以已实现 loader/ingestion/schema 为准。

## 测试与验证

重点覆盖 CSV/Parquet round-trip、market slice 对齐、HTTP retry、ingestion status 和 schema 稳定性。
