# config 技术文档

## 职责

`crates/config` 负责 TOML 配置解析、server/run 配置模型和有效配置派生。它是行为配置的入口，不执行运行逻辑、不访问存储。

## 关键实现

- `AppConfig` 描述单次 run template：runtime、database、data、strategy、portfolio、risk、broker、paper、live、ingestion、logging。
- `ServerConfig` 描述服务进程 deployment：database、server bind、logging、run defaults。
- `RuntimeMode` 支持 backtest、replay、paper、live。
- `BrokerKind` / `BrokerMode` 规范化 broker 类型和 paper/live 模式，`ibkr` 可映射到 `interactive_brokers`。
- `RiskConfig::effective_allow_short` 根据显式配置或 symbol asset class 保守派生 short 权限。

## 输入输出与持久化

输入是 TOML 文件或字符串；输出是强类型配置结构。模块不持久化，也不读取环境变量；环境变量覆盖由 app/server 边界处理。

## 边界与约束

- 风控阈值、broker kind/mode、送单闸门、paper/live 参数必须在配置中显式表达。
- 配置层只做解析和派生，不做 preflight 外部连接。
- 金额阈值在配置中以字符串保存，进入运行装配时再解析为 `Decimal`。

## 测试与验证

重点覆盖 TOML 解析、默认值、broker alias、server config、short 权限派生和非法配置错误。

