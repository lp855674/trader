# runtime 技术文档

## 职责

`crates/runtime` 负责运行编排、RunSpec、runtime manager、live runtime、worker protocol、取消控制和运行状态快照。它连接 API/CLI 与具体 backtest/paper/live/replay 执行。

## 关键实现

- `CancellationFlag` 为长运行提供协作取消。
- `RuntimeManager` 维护 run info、metadata、snapshot 和状态。
- `RunSpec` 表达从配置派生出的运行规格。
- `LiveRuntime` / `LiveRuntimeSettings` 管理 live 运行、心跳、broker snapshot 和启动恢复策略。
- live process supervisor 和 worker protocol 支持独立 worker 运行与控制。
- `StartupRecoveryUnmatchedOpenOrdersPolicy` 控制启动时未匹配 open order 的处理。
- API 启动的 live run 可以内部进程隔离；公开控制面仍是 start/status/stop/cancel 和 run 状态查询，终态可从 SQLite run record 回读。

## 输入输出与持久化

输入是 RunSpec、配置、DB、broker/executor 和控制命令；输出是 run status、snapshot、runtime events 和 worker messages。持久化通过 `storage` 语义接口完成。

## 边界与约束

- runtime 可以管理取消、pacing、恢复和事件，但不能绕过 AlgorithmEngine 生成交易结果。
- live 不得提供绕过 Risk/Execution/OMS 的手动下单面。
- 启动恢复必须可审计，open order mismatch 不能静默忽略。
- 默认启动恢复策略应对未知远端 open order 保守失败；降级为 warn-only 必须是显式 operator 配置。

## 测试与验证

重点覆盖 run lifecycle、取消、状态快照、live startup recovery、worker command 和 runtime manager 并发安全。
