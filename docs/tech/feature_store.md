# feature_store 技术文档

## 职责

`crates/feature_store` 负责研究特征记录、manifest、内存/Parquet feature store 和构建契约校验。它服务于 feature ranked universe 和 alpha gate。

## 关键实现

- `FeatureKey` 标识 run_id、symbol、feature name。
- `FeatureRecord` 保存 key、ts_ms、value、version 等特征值。
- `FeatureManifest` 描述 parquet path、schema、run id、symbols、feature name、version 和 build contract。
- `FeatureStore` trait 有 in-memory 和 Parquet 实现。
- manifest 校验用于防止输入漂移和 schema 漂移。
- feature ranked universe 和 alpha gate 使用装配阶段加载的 feature records；策略运行时只消费内存记录。

## 输入输出与持久化

输入是 feature record、manifest 和 Parquet 文件；输出是按 key/time range 查询的 feature records。运行时策略只接收内存 records，不直接访问 SQLite 或 Parquet。

## 边界与约束

- `value` 使用 `Decimal`，不得隐式转浮点破坏研究可复现性。
- Parquet schema 变更必须同步 manifest 校验和文档。
- feature store 不负责交易风控或订单。
- 文件路径/manifest 是研究数据契约的一部分，变更时要校验 run id、symbols、feature name、version、schema 和可选 build contract。

## 测试与验证

重点覆盖 Parquet round-trip、manifest 校验、range 查询、version 过滤和 build contract。
