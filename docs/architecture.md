# Trader Architecture

Version: v1.0  
Status: Draft  
Language: Rust  
Target Markets: A股 / 港股 / 美股 / 数字货币  
Storage: SQLite + Parquet  

---

## 1. 项目目标

Trader 是一个使用 Rust 开发的量化交易系统，目标是构建支持多市场、多运行模式、可扩展、可回测、可回放、可模拟交易、可实盘交易的统一交易平台。

第一阶段支持：

| 维度 | 范围 |
| --- | --- |
| 市场 | A股、港股、美股、数字货币 |
| 资产 | 股票、数字货币现货、永续合约、交割合约 |
| 运行模式 | Backtest、Replay、Paper、Live |
| 存储 | SQLite 交易状态与运行状态；Parquet 历史行情与研究数据 |
| 控制方式 | CLI、REST API、WebSocket API |

---

## 2. 文档分工

本文只描述总体架构与跨模块原则。具体设计分散到以下文档，避免同一内容在多处维护：

| 文档 | 负责内容 |
| --- | --- |
| `crates.md` | Rust workspace、crate 划分、依赖方向、feature flags、测试策略。 |
| `database.md` | SQLite 表设计、Parquet schema、repository、migration、状态恢复。 |
| `api.md` | REST / WebSocket 端点、消息格式、错误码、安全设计。 |
| `events.md` | Event envelope、事件分类、事件流、事件持久化。 |
| `strategy.md` | 策略接口、策略上下文、信号模型、策略边界。 |
| `broker.md` | Broker 抽象、路由、回报、重连、限流、故障处理。 |
| `roadmap.md` | 阶段目标、交付物、MVP 范围、发布计划。 |

---

## 3. 核心设计原则

### 3.1 策略不直接下单

策略只负责产生信号，不直接访问 Broker、OMS、SQLite、WebSocket 或 Exchange API。

合法路径：

```text
Strategy
  -> Signal / Insight
  -> Portfolio Construction
  -> Market Rule Validation
  -> Risk
  -> Execution
  -> OMS
  -> Broker
```

### 3.2 多运行模式共用核心逻辑

同一个策略应该可以运行在 Backtest、Replay、Paper、Live 中。不同模式替换 Clock、MarketDataProvider、BrokerAdapter、FillModel、SlippageModel、AccountProvider，不为每种模式写一套策略。

### 3.3 事件驱动

Trader 内部通过 Event Bus 解耦。核心事件类型由 `events.md` 维护，架构层只约束事件流方向：

```text
MarketData
  -> Strategy
  -> Portfolio
  -> Risk
  -> Execution
  -> OMS
  -> Broker
  -> Accounting
  -> Metrics / API
```

### 3.4 市场规则插件化

A股、港股、美股、数字货币规则差异很大。交易日历、lot size、涨跌停、T+1、碎股、精度、保证金、资金费率等规则必须独立封装，不能写死在策略、OMS、Broker 或 Execution 中。

### 3.5 OMS 是订单核心

OMS 负责订单生命周期与本地状态真源，包括 client order id、broker order id 映射、状态机、部分成交、撤单、拒单、超时、重复回报、乱序回报、恢复与同步。

所有订单必须经过：

```text
Execution
  -> OMS
  -> BrokerAdapter
```

### 3.6 存储分层

SQLite 存储交易状态、订单、成交、持仓、账户、运行记录、风控事件与系统配置。

Parquet 存储历史行情、分钟线、日线、tick、order book、资金费率、因子数据与研究数据。

详细 schema 只在 `database.md` 中维护。

---

## 4. 总体架构

```text
User / Operator
  -> CLI / REST API / WebSocket API
  -> Runtime Manager
  -> BacktestRuntime / ReplayRuntime / PaperRuntime / LiveRuntime
  -> Event Bus
  -> Algorithm Framework
  -> OMS
  -> Broker Adapter
  -> SQLite / Parquet
```

Algorithm Framework 的核心顺序：

```text
Universe Selection
  -> Alpha / Strategy
  -> Portfolio Construction
  -> Market Rule Validation
  -> Risk Management
  -> Execution Model
```

---

## 5. 分层说明

### 5.1 User Layer

提供 CLI、REST API、WebSocket API 三类入口：

- CLI：本地运维、数据导入、回测、Replay、报告。
- REST API：查询、启动、停止、配置、控制类操作。
- WebSocket API：实时行情、订单、成交、持仓、账户、风险与 Replay 状态推送。

API 细节见 `api.md`。

### 5.2 Runtime Manager

Runtime Manager 负责加载配置、选择运行模式、初始化上下文、启动或停止运行实例，并管理 Backtest / Replay / Paper / Live 的生命周期。

### 5.3 BacktestRuntime

BacktestRuntime 使用历史数据和模拟时钟驱动事件流，主要用于策略验证和结果分析。

### 5.4 ReplayRuntime

ReplayRuntime 将历史行情按时间流重放，支持 pause、resume、seek、speed，用于观察策略在历史市场中的实时反应。

### 5.5 PaperRuntime

PaperRuntime 使用实时行情与模拟成交，不发送真实订单，用于实盘前验证。

生产前 paper 验证必须显式加载风控、broker 和 paper pacing 配置；不能依赖隐藏在代码里的默认风控阈值或硬编码 broker。

### 5.6 LiveRuntime

LiveRuntime 连接真实券商或交易所，必须具备风控、恢复、同步、审计、监控和紧急停止能力。

当前 V1 live surface 仍只使用本地 fake broker adapter 进行生命周期验证，不连接真实券商网络。

---

## 6. 核心领域模型

### 6.1 Market

```text
CN
HK
US
CRYPTO
```

### 6.2 AssetClass

```text
EQUITY
CRYPTO_SPOT
CRYPTO_PERP
CRYPTO_FUTURE
```

### 6.3 Symbol

Symbol 必须能唯一表示市场、交易所、代码与资产类型：

```text
market
exchange
symbol
asset_class
```

示例：

```text
CN:SSE:600000:EQUITY
HK:HKEX:00700:EQUITY
US:NASDAQ:AAPL:EQUITY
CRYPTO:BINANCE:BTCUSDT:CRYPTO_SPOT
CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP
```

### 6.4 Security

Security 是系统内统一可交易标的模型，至少包含 symbol、market、exchange、asset_class、currency、lot_size、tick_size、是否可交易等基础字段。

---

## 7. 跨模块边界

### 7.1 Strategy

Strategy 只输出 Signal / Insight，不决定最终仓位，不创建订单，不连接 Broker。详细接口见 `strategy.md`。

### 7.2 Portfolio

Portfolio 根据信号、资金、目标风险与配置生成目标仓位。

### 7.3 Risk

Risk 对目标仓位和订单意图做最终检查，包括最大仓位、最大敞口、最大回撤、杠杆、保证金、交易时间、市场状态等。

### 7.4 Execution

Execution 将目标仓位转换为订单意图，可以实现立即执行、TWAP、VWAP、PostOnly、ReduceOnly 等模型。

### 7.5 OMS

OMS 管理订单状态机和本地订单真源。它不做策略判断，也不绕过 Risk。

### 7.6 Broker

Broker 只负责连接交易通道、发送订单、撤单、查询状态和接收回报。详细设计见 `broker.md`。

### 7.7 Accounting

Accounting 根据成交和账户快照更新现金、持仓、PnL、费用、保证金与组合状态。

---

## 8. 参考项目

| 领域 | 参考 | 借鉴点 |
| --- | --- | --- |
| 算法框架 | Lean / QuantConnect | Universe、Alpha、Portfolio、Risk、Execution 分层 |
| 事件驱动 | vn.py | Event engine、gateway、strategy app |
| 数字货币 | Hummingbot | connector、order tracking、market making |
| 研究平台 | Qlib | 因子工程、机器学习、模型训练 |
| 存储 | DuckDB / Parquet 生态 | 离线行情与研究数据 |

---

## 9. V1 范围

V1 优先完成：

- Rust workspace 与核心领域类型。
- Event Bus 与事件持久化。
- SQLite 状态存储与 Parquet 历史数据。
- Backtest、Replay、Paper 三条可运行路径。
- 市场规则基础抽象。
- OMS、Risk、Execution、Accounting、Metrics。
- REST API、WebSocket API。
- 示例策略。

V1 不包含：

- Qlib 在线集成。
- 复杂实盘券商矩阵。
- 高频 order book 完整撮合。
- 分布式部署。
- 多用户权限系统。
- 期权、传统期货、外汇。

---

## 10. 架构结论

Trader 的核心价值在于统一策略接口、统一事件流、统一订单管理、统一风控链路、统一账户与持仓模型，以及统一的回测 / 回放 / 模拟 / 实盘架构。

架构文档只保留跨模块约束；模块级细节必须维护在对应专题文档中，避免 API、数据库、事件、crate 职责在多个文件重复漂移。
