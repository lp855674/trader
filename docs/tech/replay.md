# replay 技术文档

## 职责

`crates/replay` 负责历史行情回放、暂停、恢复、跳转和倍速控制。它用于观察和事件回放，不负责策略下单链路。

## 关键实现

- `ReplayController` 维护 `ReplayState { run_id, status, speed, offset }`。
- `ReplayRuntime` 可绑定 `EventBus` 和共享 controller。
- `replay_bars_with_events` 逐条发布 `market.bar` runtime event。
- pause/resume/seek/speed 通过 controller 状态影响回放循环。

## 输入输出与持久化

输入是 `Vec<Bar>` 和控制命令；输出是 `ReplaySummary` / `ReplayEventSummary` 和 event bus 事件。模块自身不写 storage。

## 边界与约束

- replay event bus 是广播通道，不是审计真源。
- 回放节奏用 async sleep 和 speed 控制，不能用于真实交易时钟。
- publish 是 best-effort，观察者滞后不应中断回放。

## 测试与验证

重点覆盖暂停/恢复/seek、speed 下限、事件 payload、offset 推进和无 event bus 模式。

