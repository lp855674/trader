# 量化交易后端 — 架构与设计规格

日期：2026-03-29  
状态：Draft  
仓库：独立仓库 `E:\code\trader`（**不**并入 `bot` monorepo）  
参考：`bot` 仓库中的 **console 独立二进制 + 专用 SQLite + 清晰写入边界** 等架构思想；实现与发布在 `trader` 内独立完成。

## 1. 背景与决策摘要

### 1.1 目标

- 构建面向 **美股、港股、加密货币、Polymarket（预测市场）** 的量化交易 **后端**，支撑：
  - 实时 / 盘前 / 盘后 / 事件 / 新闻 / 技术 等多视角分析（MVP 先 **插件化占位 + 少量落地**）；
  - **多策略并行** 分析；
  - **实盘（live）** 与 **虚拟盘（paper）** 共用同一套领域模型，通过 `AccountMode` 切换执行适配器。
- **数据量大**：采用 **SQLite 作权威状态与元数据**；**冷数据**（长历史 K 线、tick 归档等）采用 **文件（如 Parquet）或外部对象存储**，由库表记录路径、校验和与时间范围，**不把全量高频原始流无限期堆进单一 DB 文件**。
- **引擎**：MVP **主链路全 Rust**；**Qlib 仅离线**（批处理/研究），**不进入**行情/下单的延迟敏感路径。若需「Rust 调用 Qlib」，语义为 **Rust 编排离线子进程或批任务**，而非内联 Python 热路径。

### 1.2 非目标（MVP）

- 机构级组合优化、跨所套利全链路、完备合规报文。
- 引入消息队列（MQ）作为默认基础设施；若日后多实例或吞吐不足，**另起规格**论证后再引入。
- MVP 不要求四类市场在每一种「分析角度」上都达到生产深度；允许 **每类市场最薄数据 + 最薄执行** 先跑通闭环。

### 1.3 与 `bot` 仓库的关系

| 维度 | 说明 |
|------|------|
| **代码归属** | 全部在 `trader` 仓库；**不**作为 `bot` 的 workspace member。 |
| **架构参考** | 借鉴 `bot` 中 **独立二进制**、**按域拆分 SQLite**、**ingest 幂等与信封**、**仅 `db` crate 触 SQL** 等约定；在 `trader` 内用 `rules.md` / `tech.md` 落为本地规范（可与 `bot` 对齐文字，但以本仓库为准）。 |
| **集成** | MVP 以 **独立 HTTP/gRPC API（二选一或并存）** 为主；与 `bot` 的对话/工具链对接属 **后续集成规格**，不在本文交付范围。

---

## 2. 最小闭环（MVP）定义

对 **每一类市场**，MVP 需可演示且可审计的闭环：

1. **标的标识**：统一 `InstrumentId`（含 `venue`、`symbol` 或链上/CLOB 标识），在库内可注册与查询。
2. **最小行情或事件输入**：至少一种 ingest（例如 REST 轮询 K 线、或测试网/WebSocket 的简化 tick），写入 **热数据层**（表或引用外部文件）。
3. **策略运行**：产出结构化 **`Signal`**（方向、数量建议、时间戳、`strategy_id`、置信度可选），落库。
4. **风控闸（可先薄）**：规则如最大单笔、日亏损熔断占位；失败则生成 **`RiskDecision`** 记录，不进入下单。
5. **执行**：通过 **`ExecutionAdapter`**，按 `AccountMode`（`paper` / `live`）路由到 **模拟撮合** 或 **真实网关**（测试网/纸交易视为 live 的一种 **环境配置**，仍走真实协议栈）。
6. **台账**：**订单、成交、持仓、现金/保证金** 写入量化库；可重放核对。

**多策略并行**：多个 `strategy_run` 并发调度，**写库**通过单写者队列、分表或 `BEGIN IMMEDIATE` 等策略控制 SQLite 写竞争；细节在实现计划中展开。

---

## 3. 总体架构（方案 1：Rust 核心 + SQLite 权威）

### 3.1 进程与部署

- **主进程**：`quantd`（名称可调整）— 独立二进制，负责 API、ingest 调度、策略执行器、下单编排、对账任务。
- **可选离线作业**：定时或 CLI 触发的 **Qlib/Python 批处理**（**不在** MVP 关键路径内；部署上可为可选组件）。

### 3.2 建议 Cargo workspace 划分（对齐 `bot` 式边界）

| Crate | 职责 |
|-------|------|
| `trader-db`（或 `db`） | **唯一**依赖 `sqlx` / 内联 SQL；对外暴露 `Db` 与仓储接口。 |
| `trader-config` | 配置加载与校验（路径、密钥引用、环境名）。 |
| `trader-domain` | 类型：`Instrument`、`Signal`、`Order`、`Fill`、`Position`、`AccountMode` 等。 |
| `trader-ingest` | 各 venue 数据采集适配器（美股/港股/币/Poly 各一薄实现）。 |
| `trader-strategy` | 策略 trait、注册表、运行上下文；MVP 可先内置简单规则策略。 |
| `trader-exec` | `ExecutionAdapter`：paper 模拟、live 券商/交易所 API。 |
| `trader-api` | HTTP 或 gRPC 服务层，DTO 与鉴权。 |
| `quantd`（bin） | 组装运行时、信号处理、任务循环。 |

约束：**除 `db` crate 外禁止直接使用 `SqlitePool`**；与 `bot` 的 `rules.md` 精神一致。

### 3.3 逻辑分层与数据流

```
[Venue APIs / Feeds] --> ingest --> [Normalized bars/events] --> feature pipeline (MVP: thin)
                                                      |
                                                      v
                                            [Strategy engine] --> Signal
                                                      |
                                                      v
                                            [Risk gate] --> OrderIntent
                                                      |
                                                      v
                                    ExecutionAdapter(paper|live) --> Broker/Exchange/Sim
                                                      |
                                                      v
                                            [Ledger: orders, fills, positions]
```

- **分析阶段**（实时 / 盘前 / 盘后 / 事件 / 新闻 / 技术）在模型上为 **Pipeline Stage**，共享输入与输出契约（例如统一输出到 `features` 或 `signal_inputs` 表）；MVP 可只实现 **技术/规则** 一类，其余注册空实现或手动导入。

### 3.4 存储分层

| 层 | 内容 | 介质 |
|----|------|------|
| **权威台账** | 订单、成交、持仓、账户、策略运行记录、审计事件 | SQLite |
| **热行情** | 近期 K 线、少量 tick 摘要 | SQLite 表（控制行数与保留策略） |
| **冷归档** | 长历史、全量 tick、研究用大数据 | Parquet 等文件 + DB 元数据（路径、hash、区间） |

---

## 4. 多市场（Venue）抽象

### 4.1 统一概念

- **`Venue`**：`US_EQUITY` | `HK_EQUITY` | `CRYPTO` | `POLYMARKET`（枚举可扩展）。
- **`Instrument`**：必须可序列化、可持久化；Crypto/Poly 需容纳 **链/合约/condition id** 等扩展字段（JSON 列或附属表）。

### 4.2 MVP「最薄实现」预期

- **美股/港股**：一种行情源 + 一种下单路径（纸交易或沙盒 API 优先）。
- **加密货币**：测试网或单一交易所 REST 最薄下单 + 简单行情。
- **Polymarket**：CLOB/链上只读或最小下单（以合规与账号可用性为准）；闭环可先 **模拟成交** + 真实行情 ingest，若 live 受限则在规格中标注 **环境开关**。

---

## 5. 实盘与虚拟盘

- **`AccountMode`**：`paper` | `live`。
- **同一套** `Order`、`Fill` 模型；**适配器**不同：
  - `paper`：**内置撮合器**（可按 last/mid 价简化）或 **回放撮合**。
  - `live`：**真实 API**；密钥由配置文件或 OS secret 引用，**禁止写入日志**。
- **建议**：配置层强制 **环境标签**（`dev` / `paper` / `prod`），防止误连。

---

## 6. Qlib 与替代方案（定位说明）

| 工具/方向 | 适用场景 | 与本文 MVP 关系 |
|-----------|----------|-----------------|
| **Qlib** | A 股/美股因子研究、回测、Alpha 挖掘 | **仅离线**；产出导入 SQLite 或 Parquet 指针 |
| **自研 Rust + polars/ndarray** | 币/Poly 特征、低延迟流水线 | MVP 主路径推荐 |
| **Zipline / vectorbt 等** | Python 回测 | 可选离线，非必选 |
| **交易所 SDK** | Live 执行 | 按 venue 选官方或社区 Rust/crate |

**结论**：不要求「一个引擎吃四类市场」；**统一的是信号与订单契约**，引擎按资产与研究阶段选型。

---

## 7. 错误处理、可观测性与测试

### 7.1 错误处理

- 对外 API：稳定 **错误码** + 结构化字段（对齐 `bot` 的 `error_code` 思路）。
- ingest / 下单：**可重试** 与 **幂等键**（客户端 `idempotency_key` 或交易所 `cl_ord_id`）；失败落 **审计/死信表**，避免静默丢单。

### 7.2 可观测性

- 结构化日志（如 `tracing`）：字段包含 `venue`、`strategy_id`、`trace_id`、`account_mode`。
- 关键指标：ingest 延迟、策略循环耗时、下单往返、SQLite 锁等待（若可测）。

### 7.3 测试

- **单元测试**：domain、风控规则、paper 撮合。
- **集成测试**：嵌入式 SQLite 或临时文件；live 用 **mock adapter**。
- **契约测试**：各 `ExecutionAdapter` 对统一 `OrderIntent` 的输入输出快照。

---

## 8. 与 console 模式的类比（便于团队对齐心智）

| console（`bot` 规格） | trader（本仓库） |
|----------------------|------------------|
| 独立二进制 + 专用 SQLite | `quantd` + 量化库 |
| WebSocket ingest + 幂等信封 | 可选：执行节点向 `quantd` 上报 **分析/执行事件** 时使用同一模式；MVP 可简化为进程内调用 |
| 编排域 vs 执行域 | **研究/批处理（离线）** vs **在线交易路径** |

---

## 9. 待决问题（实现前收口）

1. **对外 API**：MVP 选定 **HTTP**、**gRPC** 或两者中的默认主通道。
2. **券商/交易所清单**：每 venue 各选 **一个** 具体供应商作为 MVP 适配器实现目标。
3. **Polymarket live** 是否在首版必须真实下单，或 **paper + 真实行情** 即算闭环（需结合合规与账号）。
4. **冷数据目录**：本地根路径、备份与清理策略（runbook）。
5. **`trader` 内** `rules.md` / `tech.md` 是否逐条复刻 `bot` 或裁剪（建议复刻 DB 边界与日志规范，其余按体量裁剪）。

---

## 10. 文档真源与后续步骤

- 本文档为 `trader` 仓库内量化后端 **架构真源**；与实现不一致时应先改文档再改代码。
- 下一步：按 `writing-plans` 工作流编写 **实现计划**（迁移脚本、crate 骨架、MVP 四条闭环验收用例）。

---

本规格取代先前「在 `bot` monorepo 内新增量化 crate」的设想；**以独立仓库 `E:\code\trader` 为唯一代码与发布边界**。
