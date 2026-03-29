# 量化交易后端 — 架构与设计规格

日期：2026-03-29  
状态：Draft  
仓库：独立仓库 `E:\code\trader`（**不**并入 `bot` monorepo）  
参考：`bot` **整体**由多 crate、多入口（HTTP/WebSocket/CLI/TUI 等）与多进程形态共同构成；借鉴其 **强模块边界**、**仅 `db` 触 SQL**、**统一请求/响应契约**、**可审计** 与 **结构化可观测**（`error_code`、`trace_id`、`channel` 等）、**按域拆分持久化** 等 **跨组件约定**。**对照物是 `bot` 全系统**，而非某一子系统（如单独对标 console）；`trader` 在独立仓库内 **独立实现**，不复用 `bot` 源码。

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
| **架构参考** | 借鉴 `bot` **整套系统**已沉淀的约定：`agent`/`tools`/`channels`/`db` 式 **职责分离**、**敏感数据不进日志**、**关键链路可审计**、**多通道对外** 等；在 `trader` 内用 `rules.md` / `tech.md` 落为本地规范（可与 `bot` 对齐文字，但以本仓库为准）。 |
| **集成** | MVP 以 **独立对外接口** 为主：**HTTP**（REST/JSON）、**gRPC**（强类型/内部工具）、**WebSocket**（推送行情/订单与成交流、订阅）；可按场景 **并存**；与 `bot` 的对话/工具链对接属 **后续集成规格**，不在本文交付范围。 |

---

## 2. 最小闭环（MVP）定义

对 **每一类市场**，MVP 需可演示且可审计的闭环：

1. **标的标识**：统一 `InstrumentId`（含 `venue`、`symbol` 或链上/CLOB 标识），在库内可注册与查询。
2. **最小行情或事件输入**：至少 **一个** `IngestAdapter`（例如 REST 轮询 K 线、或测试网/WebSocket 的简化 tick），写入 **热数据层**（表或引用外部文件）；架构上支持多源，MVP 可先挂一条见 §3.5。
3. **策略运行**：产出结构化 **`Signal`**（方向、数量建议、时间戳、`strategy_id`、置信度可选），落库。
4. **风控闸（可先薄）**：规则如最大单笔、日亏损熔断占位；失败则生成 **`RiskDecision`** 记录，不进入下单。
5. **执行**：通过 **`ExecutionRouter`** 选择具体 **`ExecutionAdapter`**，按 `AccountMode`（`paper` / `live`）与账户绑定路由到 **模拟撮合** 或 **某一真实网关**（测试网/纸交易视为 live 的一种 **环境配置**，仍走真实协议栈）；多网关并存见 §3.6。
6. **台账**：**订单、成交、持仓、现金/保证金** 写入量化库；可重放核对。

**多策略并行**：多个 `strategy_run` 并发调度，**写库**通过单写者队列、分表或 `BEGIN IMMEDIATE` 等策略控制 SQLite 写竞争；细节在实现计划中展开。

---

## 3. 总体架构（方案 1：Rust 核心 + SQLite 权威）

### 3.1 进程与部署

- **主进程**：`quantd`（名称可调整）— 独立二进制，负责 API、ingest 调度、策略执行器、下单编排、对账任务。
- **可选离线作业**：定时或 CLI 触发的 **Qlib/Python 批处理**（**不在** MVP 关键路径内；部署上可为可选组件）。

### 3.2 建议 Cargo workspace 划分（对齐 `bot` 式边界）

**命名约定**：library crate **一律短名、不带 `trader-` 前缀**（仓库根目录名已是 `trader`，workspace 内无需重复前缀）。

| Crate | 职责 |
|-------|------|
| **`db`** | **唯一**依赖 `sqlx` / 内联 SQL；对外暴露 `Db` 与仓储接口。 |
| **`config`** | 配置加载与校验（路径、密钥引用、环境名）。 |
| **`domain`** | 类型：`Instrument`、`Signal`、`Order`、`Fill`、`Position`、`AccountMode` 等。 |
| **`ingest`** | **多数据源**：按 **`DataSource` / `IngestAdapter` trait** 注册若干实现；MVP 每 venue 先接 **一条** 最薄源，架构上允许多源并存（见 §3.5）。 |
| **`strategy`** | 策略 trait、注册表、运行上下文；MVP 可先内置简单规则策略。 |
| **`exec`** | **多下单接口**：多个 **`ExecutionAdapter`** 实现（paper、各券商/交易所）；由路由按账户/配置选择（见 §3.6）。 |
| **`api`** | 对外服务层：**HTTP**（管理、查询、部分命令）、**gRPC**（可选）、**WebSocket**（实时推送：行情摘要、订单/成交、策略事件、订阅协议）；统一 DTO、鉴权与 **错误码**；WS 需 **鉴权、心跳、背压** 与 **幂等/游标**（对齐 `bot` 在全系统中对长连接与可靠推送的工程习惯）；方向以 **服务端 → 客户端** 推送为主，必要时客户端上行命令另议。 |
| **`quantd`**（`[[bin]]`） | 组装运行时、信号处理、任务循环；可放在 `crates/quantd` 或根 `src/bin`。 |

约束：**除 `db` crate 外禁止直接使用 `SqlitePool`**；与 `bot` 的 `rules.md` 精神一致。

### 3.3 逻辑分层与数据流

```
[DataSource A,B,… / Feeds] --> ingest adapters --> [Normalized bars/events] --> feature pipeline (MVP: thin)
                                                      |
                                                      v
                                            [Strategy engine] --> Signal
                                                      |
                                                      v
                                            [Risk gate] --> OrderIntent
                                                      |
                                                      v
                          ExecutionRouter --> Adapter₁…Adapterₙ (paper | broker A | exchange B | …)
                                                      |
                                                      v
                                            [Ledger: orders, fills, positions]
```

- **分析阶段**（实时 / 盘前 / 盘后 / 事件 / 新闻 / 技术）在模型上为 **Pipeline Stage**，共享输入与输出契约（例如统一输出到 `features` 或 `signal_inputs` 表）；MVP 可只实现 **技术/规则** 一类，其余注册空实现或手动导入。

### 3.5 多数据源（Market Data）

- **目标**：同一 `Instrument` 可同时存在 **多个上游**（例如：主行情 + 备用源、付费级与免费级、REST 与 WebSocket）；后续可增源而 **不改** 策略侧统一契约。
- **适配模型**：每个源实现同一 **`IngestAdapter`**（或分层：`connect` / `poll` / `normalize`），带稳定 **`data_source_id`**（字符串或枚举扩展），配置（鉴权、限频、endpoint）与 **venue 能力声明**（是否 tick、是否盘前等）由配置或 DB 元数据驱动。
- **归一化**：落地数据必须进入 **统一时间序列/事件模型**（bar、quote、trade、news_ref 等），并在记录上 **携带 `data_source_id`**，便于对账与回放。
- **冲突与主源**：策略消费层使用 **`MarketDataView` 抽象** — 默认按配置 **主源（primary）** 解析；可选 **合并规则**（例如更优延迟、显式优先级表）；MVP 可只实现 **单源 + 预留多源字段**，但 **注册表与 trait 边界** 按多源设计，避免二次大改。
- **持久化**：`db` 中建议为数据源与订阅关系预留元数据（如 `data_sources`、`instrument_subscriptions` 或等价结构），具体表结构在迁移设计中定稿。

### 3.6 多下单接口（Execution）

- **目标**：同一套 **`OrderIntent` / `Order` / `Fill`** 可路由到 **不同券商、交易所、链上/CLOB、纸交易撮合**；账户维度决定走哪条适配器。
- **适配模型**：每个通道实现 **`ExecutionAdapter`** trait（下单、撤单、查单、可选持仓/余额同步），带 **`execution_profile_id`**（配置内唯一）；能力通过 trait 或侧表声明（是否支持市价/限价、是否支持融券、是否链上确认等）。
- **路由**：**`ExecutionRouter`**（或等价）根据 **`account_id` + `venue` + `AccountMode`**（及环境标签）解析为 **唯一适配器**；禁止在策略内硬编码供应商类型。
- **纸 vs 实**：`paper` 对应 **内置模拟适配器**（可配置滑点/手续费）；`live` 对应 **零个或多个** 真实适配器注册实例，由账户绑定。
- **凭证**：每 `execution_profile_id` 独立密钥引用与限频；**禁止**将密钥写入日志或订单审计明文。

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
- **同一套** `Order`、`Fill` 模型；经 **`ExecutionRouter`** 选择具体 **适配器**（可多实例注册，见 §3.6）：
  - `paper`：**内置撮合器**（可按 last/mid 价简化）或 **回放撮合**。
  - `live`：**真实 API**（券商/交易所/CLOB 等可多路）；密钥由配置文件或 OS secret 引用，**禁止写入日志**。
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

- 对外接口：**HTTP/gRPC** 用稳定 **错误码** + 结构化字段（对齐 `bot` 的 `error_code` 思路）；**WebSocket** 用 **带 `error_code` 的 JSON 信封**（或二进制帧 + 元数据），区分 **业务错误** 与 **连接级错误**；关键推送带 **`event_id` 幂等** 或游标，便于客户端断线重连后补齐。
- ingest / 下单：**可重试** 与 **幂等键**（客户端 `idempotency_key` 或交易所 `cl_ord_id`）；失败落 **审计/死信表**，避免静默丢单。

### 7.2 可观测性

- 结构化日志（如 `tracing`）：字段包含 `venue`、`strategy_id`、`trace_id`、`account_mode`；ingest/下单路径建议带 `data_source_id`、`execution_profile_id`（若适用）。
- 关键指标：ingest 延迟（可按 `data_source_id` 分面）、策略循环耗时、下单往返（可按 `execution_profile_id` 分面）、SQLite 锁等待（若可测）。

### 7.3 测试

- **单元测试**：domain、风控规则、paper 撮合。
- **集成测试**：嵌入式 SQLite 或临时文件；live 用 **mock adapter**。
- **契约测试**：各 `ExecutionAdapter` 对统一 `OrderIntent` 的输入输出快照。

---

## 8. 与 `bot` 整个系统的对照（便于团队对齐心智）

下列为 **架构原则与职责切分** 的对照，**不是** crate 名称一一映射；`trader` 不复用 `bot` 代码，仅对齐工程与产品哲学。

| `bot` 中体现的原则或形态 | `trader` 中的对应落点 |
|--------------------------|------------------------|
| **local-first、可管控、可扩展（个人/自托管语境）** | 本地或自托管部署为主；配置与密钥可控；环境与 `paper`/`live` 显式隔离。 |
| **仅 `db` 模块执行 SQL**（`sqlx` 隔离） | 唯 **`db`** crate 使用 `sqlx`/内联 SQL；其余经仓储接口访问。 |
| **跨模块边界清晰**（`agent` 编排、`tools` 执行、`channels` 传输、`memory` 等） | **`strategy`**：决策与策略循环；**`exec`** / **`ingest`**：对外 I/O（下单、拉行情）；**`api`**：HTTP/WS/gRPC 对外；**`domain`**：统一类型。 |
| **多渠道**（CLI/TUI/HTTP/WebSocket 等并存） | **HTTP + WebSocket + 可选 gRPC**；统一鉴权、DTO 与错误契约（§7.1）。 |
| **关键操作可审计**（对话、工具调用、结果可追溯） | 信号、风控决策、订单/成交、ingest 事件可审计落库；密钥与隐私字段 **禁止** 写入日志或对外追溯明文。 |
| **结构化日志与稳定错误码**（`trace_id`、`error_code`、`channel` 等） | §7.1 / §7.2：对外与 WS 信封对齐同一套 **可聚合** 字段习惯。 |
| **多进程/多二进制**（`bot` 整体由 gateway、agent 路径、console、daemon、CLI/TUI 等共同构成「入口、执行、运维」分层） | **`quantd`** 独立进程 + **专用 SQLite**；`ingest`/`exec` 类似「外向工具与通道」；观测与运维面经 **`api`**（及后续可选独立运维二进制，另文规定）。 |
| **长连接与异步通道上的可靠语义**（鉴权、重连、背压、幂等等，散见于 `bot` 的 HTTP/WS/channel 实现与规格，而非单一子系统专利） | WS **出站**：`event_id`/游标、断线可补齐；若存在 **入站**（如外部节点向 `quantd` 上报），复用 **信封 + 幂等** 模型（与 §7.1 一致）。 |
| **扩展点**（`hooks`、`skills`、Wasm 工具等） | **适配器注册**（`IngestAdapter` / `ExecutionAdapter`）、**Pipeline Stage**、策略插件；边界清晰、可单测，与 `bot`「能力可插拔」同一哲学。 |
| **失败与降级**（`bot` `tech.md`：LLM/工具/DB/channel 分层失败策略） | ingest 失败可重试/死信；下单失败可审计回滚意图；DB 失败快速失败；**单通道** 故障不拖垮全局进程（与 `bot` 的分层隔离一致）。 |
| **配置与真源**（`bot`：`tech.md` 约定业务配置以 DB 为真源等） | `trader` 是否在 MVP 完全照搬 **由 DB 承载业务配置**，见 §9 第 6 条与本地 `tech.md` 定稿；**密钥**仍不得入日志。 |

---

## 9. 待决问题（实现前收口）

1. **对外接口组合**：MVP 至少落地 **HTTP**（必备，用于管理与同步查询）；**WebSocket** 至少覆盖 **一类** 高价值推送（如订单/成交 或 订阅行情摘要）；**gRPC** 是否首版交付由工具链需求决定。三者可并存，**不**互斥。
2. **券商/交易所清单**：MVP 每 venue 各选 **至少一个** 具体供应商作为首条 **ExecutionAdapter**；架构上保留 **同 venue 多 profile**（例如主备券商、测试网与主网分 profile）的扩展位。
3. **行情供应商清单**：MVP 每 venue 各选 **至少一个** `IngestAdapter`；后续新增数据源仅增实现与配置，不改核心契约。
4. **Polymarket live** 是否在首版必须真实下单，或 **paper + 真实行情** 即算闭环（需结合合规与账号）。
5. **冷数据目录**：本地根路径、备份与清理策略（runbook）。
6. **`trader` 内** `rules.md` / `tech.md` 是否逐条复刻 `bot` 或裁剪（建议复刻 DB 边界与日志规范，其余按体量裁剪）。

---

## 10. 文档真源与后续步骤

- 本文档为 `trader` 仓库内量化后端 **架构真源**；与实现不一致时应先改文档再改代码。
- **实现计划（已编写）：** [`docs/superpowers/plans/2026-03-29-quantd-mvp-implementation-plan.md`](../superpowers/plans/2026-03-29-quantd-mvp-implementation-plan.md) — Cargo workspace、迁移、`ingest`/`exec` trait、四 venue 集成测试、HTTP/WS。

---

本规格取代先前「在 `bot` monorepo 内新增量化 crate」的设想；**以独立仓库 `E:\code\trader` 为唯一代码与发布边界**。
