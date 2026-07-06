# events 技术文档

## 职责

`crates/events` 定义事件 envelope、事件枚举、运行时发布订阅接口和结构化日志写入边界。它为 runtime、API、WebSocket 和审计事件提供统一事件模型。

## 关键实现

- `EventBus` 提供内存广播，用于运行时观察者和 WebSocket。
- `EventEnvelope<T>` 包含 `event_id`、`ts`、`source`、`category` 和 typed payload。
- `EventCategory` 当前覆盖 market、signal、portfolio、risk、execution、order、trade、position、account、system。
- `TraderEvent` 当前包含 `Signal(SignalEvent)` 和 `Runtime(RuntimeEvent)`；`SignalSide` 支持 `Buy`、`Sell`、`CloseLong`、`CloseShort`。
- `RuntimeEvent` 使用 category 加 `payload_json` 承载运行时事件；进入 API response 时应由 API/storage 层转成结构化 JSON。
- `runtime_envelope` 等 helper 负责构造运行事件 envelope。
- `LogWriter`、`SystemLogLayer` 将 tracing 日志批量写入 sink。

## 输入输出与持久化

输入是 typed event 或 tracing event；输出是 event bus envelope 或 log sink batch。事件持久化真源由 `storage` 的 event/log 表承担，内存 bus 不是状态真源。

## 边界与约束

- payload 应由 typed payload 构造后序列化，避免各模块手写 JSON 漂移。
- 事件中不得包含凭证或敏感 secret。
- 发布失败不能让只读观察者破坏核心运行，但关键审计写入失败应由调用方处理。
- `event_store` 是审计真源，`EventBus` 只是运行期广播；WebSocket 订阅需要能回放已持久化事件再接实时广播。
- 新增事件类别或 payload shape 时，要同时考虑 storage projection、API response 和 WebSocket 消费。

## 测试与验证

重点覆盖 envelope 序列化、event bus publish/subscribe、log writer flush、category 过滤和 shutdown。
