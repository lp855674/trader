# trader-cli 技术文档

## 职责

`apps/trader-cli` 是本地运维和实验入口，负责初始化、迁移、数据导入、feature 构建、回测、replay、paper、broker smoke、风控操作、报告和查询导出。

## 关键实现

- 使用 `clap` 定义 `trader` 命令和大量 subcommand。
- 主要命令包括 `init`、`migrate`、`import-bars`、`feature-*`、`backtest`、`paper-run`、`replay`、`report`、positions/snapshots/configs/runs/logs 查询。
- broker 相关命令覆盖 Binance testnet、IBKR paper 的 readonly、open orders、executions、reconcile、tiny order、cancel/recover 等操作。
- `risk-kill-switch`、order/risk/reconciliation event 查询和 ingestion/funding 命令支持运维审计。
- CLI 负责把 TOML/env/file 参数装配成 crate 调用，不应承载领域规则。

## 输入输出与持久化

输入是命令行参数、配置文件、数据文件和环境变量；输出是终端报告、文件导出、数据库记录和外部 paper/testnet 操作。持久化必须通过 crate 暴露的 `storage::Db` 方法完成。

## 边界与约束

- 真实外部操作必须要求显式确认参数，例如 tiny order/cancel 类命令。
- CLI 不能绕过 runtime/algorithm/risk/OMS 写成交或下单。
- 凭证只能从环境变量读取，不能打印到日志或报告。

## 测试与验证

重点覆盖命令解析、配置装配、无网络 smoke、危险命令确认参数和报告/导出格式。

