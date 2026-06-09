我已基于 `docs/architecture.md` 和当前代码重新核对项目进度，结论如下。

## 总体结论
项目并不是“架构没做”，而是**已经实现了大部分基础设施与分层骨架，但还没有完全把这些能力串成 architecture.md 设想的统一事件驱动主链路**。换句话说，很多关键模块已经存在，但主流程仍然偏编排式，抽象边界还需要继续收敛。

## 已完成 / 已较成熟

### 1. 策略边界
`crates/strategies/src/strategies.rs` 中已经有 `Strategy` 和 `AlphaModel` 两套接口，策略本身只负责产生信号，不直接依赖 Broker、OMS 或 SQLite。

### 2. 多运行模式
`StrategyRuntimeMode` 已支持 Backtest、Replay、Paper、Live。`backtest`、`paper`、`replay`、`api` 等模块也已经在使用这些模式。

### 3. 市场规则
市场规则已按市场 / 资产类型进行封装，能够承载 CN / HK / US / CRYPTO 这类差异化规则。

### 4. OMS
`crates/oms/src/oms.rs` 中的 `OrderStateMachine` 已经实现了订单生命周期、重复回报去重、部分成交以及乱序 / 终态处理。

### 5. EventBus 基础设施
`crates/events/src/bus.rs` 和 `crates/events/src/event.rs` 已经具备事件结构、事件封装与广播总线能力。

### 6. Universe / Alpha 基础
`crates/universe/src/universe.rs` 已经定义了 `UniverseSelector` 和 `StaticUniverseSelector`；`crates/alpha/src/alpha.rs` 也已经有 `AlphaModel` 与 `CompositeAlphaModel`。

## 部分完成 / 仍需推进

### 1. 主链路事件化
`crates/algorithm/src/algorithm.rs` 已经按 Universe → Alpha → Portfolio → Risk → Execution → OMS 的顺序编排，并把关键节点转成事件；但目前它仍然是一个引擎内部的顺序编排，而不是完全独立的模块订阅 / 发布解耦链路。

### 2. Backtest 的存储边界
`crates/backtest/src/backtest.rs` 已经通过 `AlgorithmEngine` 产生事件并落库，但仍直接依赖 `storage` 中的具体 record 类型，存储抽象还不够薄。

### 3. Paper / Replay 集成
`crates/paper/src/paper.rs` 与 `crates/replay/src/replay.rs` 已接入 EventBus 和控制能力，但完整的状态同步、回放审计和统一事件流还没有完全闭环。

## 仍明显不足

### 1. 动态 Universe Selection
当前实现偏最小可用，离真正的动态选股 / 选币框架还有差距。

### 2. Alpha 生态
已有接口与组合器，但还缺少更完整的多 Alpha 组合、配置化装配与分层治理。

### 3. 存储抽象
`crates/storage/src/repositories.rs` 仍集中暴露大量具体 `Db` 写入方法，上层业务对 SQLite record 类型依赖较深。

### 4. Replay 的完整体验
pause / resume / seek / speed 已有，但事件驱动的完整回放链路、API / WebSocket 状态同步仍需补齐。

## 对原有问题分析的修正
`problem.md` 里有几处说法需要修正：

- **“事件总线未使用”**：不准确。事件总线已经接入，但主要还承担广播 / 记录作用，主业务链路仍是编排式。
- **“Universe Selection 未实现”**：不准确。Universe 已有最小实现，但还没有发展成动态筛选框架。
- **“Alpha 只定义空 trait”**：不准确。Alpha 已有接口和组合器，也已经被引擎与策略接入，只是生态还早期。
- **“Replay 只是简单睡眠，没有发布事件”**：不准确。Replay 已有控制器、速度控制、事件发布和 WebSocket 控制接入，但还没形成完整统一的回放闭环。
- **“Backtest 完全过程式，没有 EventBus”**：不准确。Backtest 已经围绕 `AlgorithmEngine` 产生事件并写入数据库，但持久化和编排边界仍需继续收敛。

## 建议的修复优先级

1. **收敛 Backtest / Paper 的持久化边界**，减少对 `storage::Db` 和具体 record 的直接依赖。
2. **继续统一 AlgorithmEngine 的职责边界**，让主运行链路保持一致，避免 runtime 侧重复编排。
3. **补强 Replay 的状态 / 事件同步**，让控制动作和回放状态都能被 API / WebSocket 稳定观察。
4. **再推进 Universe / Alpha 的动态能力**，在基础链路稳定后再扩展更复杂的策略装配与筛选逻辑。

## 结论
当前项目的状态可以概括为：**基础设施和分层骨架已经到位，但还没有完全把这些能力串成 architecture.md 设想的统一事件驱动主链路。**

如果要继续推进，建议优先修的是：**Backtest / Paper 的存储抽象、AlgorithmEngine 的职责收敛、Replay 的状态同步**。