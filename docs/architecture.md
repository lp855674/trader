````markdown
# Trader Architecture

Version: v1.0  
Status: Draft  
Language: Rust  
Target Markets: A股 / 港股 / 美股 / 数字货币  
Storage: SQLite + Parquet  

---

# 1. 项目目标

Trader 是一个使用 Rust 开发的量化交易系统，目标是构建一个支持多市场、多运行模式、可扩展、可回测、可回放、可模拟交易、可实盘交易的统一交易平台。

Trader 第一阶段支持的市场：

```text
CN      A股
HK      港股
US      美股
CRYPTO  数字货币
````

Trader 第一阶段支持的资产类型：

```text
EQUITY          股票
CRYPTO_SPOT     数字货币现货
CRYPTO_PERP     数字货币永续合约
CRYPTO_FUTURE   数字货币交割合约
```

Trader 第一阶段支持的运行模式：

```text
Backtest    历史回测
Replay      历史行情实时回放
Paper       模拟交易
Live        实盘交易，后期实现
```

Trader 第一阶段支持的数据存储：

```text
SQLite
  交易状态、订单、成交、持仓、账户、运行记录、风控事件

Parquet
  历史行情、分钟线、日线、Tick、OrderBook、资金费率、因子数据
```

Trader 第一阶段支持的控制方式：

```text
CLI
REST API
WebSocket API
```

---

# 2. 核心设计原则

## 2.1 策略不直接下单

策略只负责产生信号，不直接访问 Broker，不直接访问交易接口，不直接写数据库。

正确的数据流：

```text
Strategy
  ↓
Insight
  ↓
Portfolio Construction
  ↓
PortfolioTarget
  ↓
MarketRuleValidator
  ↓
RiskManager
  ↓
ExecutionModel
  ↓
OrderRequest
  ↓
OMS
  ↓
BrokerAdapter
```

禁止：

```text
Strategy -> Broker
Strategy -> SQLite
Strategy -> WebSocket
Strategy -> Exchange API
```

允许：

```text
Strategy -> Context
Strategy -> Indicator
Strategy -> HistoricalData
Strategy -> Insight
```

---

## 2.2 回测、回放、模拟、实盘共用核心逻辑

同一个策略应该可以运行在：

```text
Backtest
Replay
Paper
Live
```

不同模式只替换：

```text
Clock
MarketDataProvider
BrokerAdapter
FillModel
SlippageModel
AccountProvider
```

不能为每种运行模式写一套策略。

---

## 2.3 事件驱动

Trader 内部使用 Event Bus 解耦。

模块之间不直接互相调用，而是通过事件通信。

核心事件：

```text
MarketEvent
InsightEvent
PortfolioTargetEvent
OrderRequestEvent
OrderEvent
FillEvent
PositionEvent
PortfolioEvent
RiskEvent
AccountEvent
SystemEvent
CommandEvent
```

---

## 2.4 市场规则插件化

A股、港股、美股、数字货币规则完全不同。

市场规则必须独立封装，不能写死在策略、OMS、Broker 或 Execution 中。

```text
CNMarketRuleValidator
HKMarketRuleValidator
USMarketRuleValidator
CryptoMarketRuleValidator
```

---

## 2.5 OMS 是核心模块

OMS 是 Order Management System。

实盘交易里，OMS 比策略更重要。

OMS 必须负责：

```text
订单状态机
client_order_id 生成
broker_order_id 映射
部分成交处理
撤单处理
拒单处理
订单超时处理
重复回报处理
乱序回报处理
本地状态恢复
Broker 状态同步
订单事件落库
```

所有订单必须经过：

```text
ExecutionModel
  ↓
OMS
  ↓
BrokerAdapter
```

---

## 2.6 存储分层

Trader 使用 SQLite + Parquet。

```text
SQLite
  状态数据
  订单数据
  成交数据
  持仓数据
  账户数据
  策略运行记录
  风控事件

Parquet
  大规模历史行情
  K线
  Tick
  OrderBook
  因子
  资金费率
  Open Interest
```

原则：

```text
SQLite 管交易状态
Parquet 管历史行情
```

---

## 2.7 股票和数字货币统一抽象，但规则分离

Trader 使用统一的 Symbol / Security / Order / Fill / Position 抽象。

但不同市场的规则由不同模型处理。

```text
股票:
  CN / HK / US

数字货币:
  Spot / Perp / Future
```

统一：

```text
Symbol
Security
OrderRequest
OrderEvent
Fill
Portfolio
Risk
BrokerAdapter
```

分离：

```text
MarketRule
FeeModel
SettlementModel
MarginModel
TradingCalendar
PositionModel
```

---

# 3. 参考项目

Trader 不直接复制单个开源项目，而是综合参考多个成熟系统。

| 模块           | 参考项目                   | 参考内容                                                  |
| ------------ | ---------------------- | ----------------------------------------------------- |
| 总体架构         | QuantConnect Lean      | Algorithm Framework、Security、Portfolio、Risk、Execution |
| 事件驱动         | Barter-rs              | Rust 事件驱动交易系统                                         |
| 高频回测         | hftbacktest            | Tick Replay、OrderBook Replay、延迟、队列位置                  |
| OMS          | Lean + MMB             | 订单生命周期、订单状态机、交易所适配                                    |
| 数据处理         | Polars                 | Rust DataFrame、Lazy Query、Parquet                     |
| 数据格式         | Apache Arrow / Parquet | 列式存储、历史行情存储                                           |
| 研究平台         | Qlib                   | 因子工程、机器学习、模型训练                                        |
| WebSocket 协议 | Binance / OKX          | channel-based 实时推送协议                                  |
| Web API      | Axum                   | Rust HTTP / WebSocket 服务                              |

---

# 4. 总体架构

```text
┌────────────────────────────────────────────────────────────┐
│                        User Layer                          │
│                                                            │
│       CLI              REST API             WebSocket API  │
└──────────────────────────────┬─────────────────────────────┘
                               │
                               ▼
┌────────────────────────────────────────────────────────────┐
│                     Runtime Manager                        │
│                                                            │
│  BacktestRuntime   ReplayRuntime   PaperRuntime   LiveRuntime│
└──────────────────────────────┬─────────────────────────────┘
                               │
                               ▼
┌────────────────────────────────────────────────────────────┐
│                        Event Bus                           │
│                                                            │
│  MarketEvent        InsightEvent       TargetEvent          │
│  OrderEvent         FillEvent          PortfolioEvent       │
│  RiskEvent          AccountEvent       SystemEvent          │
└──────────────────────────────┬─────────────────────────────┘
                               │
                               ▼
┌────────────────────────────────────────────────────────────┐
│                  Algorithm Framework                       │
│                                                            │
│  Universe Selection                                        │
│  Alpha Model                                               │
│  Portfolio Construction                                    │
│  Market Rule Validation                                    │
│  Risk Management                                           │
│  Execution Model                                           │
└──────────────────────────────┬─────────────────────────────┘
                               │
                               ▼
┌────────────────────────────────────────────────────────────┐
│                            OMS                             │
│                                                            │
│  Order State Machine                                       │
│  Client Order ID Mapping                                   │
│  Broker Order ID Mapping                                   │
│  Retry / Cancel / Recovery                                 │
│  Order Event Store                                         │
└──────────────────────────────┬─────────────────────────────┘
                               │
                               ▼
┌────────────────────────────────────────────────────────────┐
│                      Broker Adapter                        │
│                                                            │
│  BacktestBroker                                            │
│  ReplayBroker                                              │
│  PaperBroker                                               │
│  LiveBroker                                                │
│  CryptoExchangeBroker                                      │
│  StockBroker                                               │
└──────────────────────────────┬─────────────────────────────┘
                               │
                               ▼
┌────────────────────────────────────────────────────────────┐
│                  Accounting / Metrics                      │
│                                                            │
│  CashBook           PositionBook       PortfolioBook        │
│  PnL                Drawdown           Performance Metrics  │
│  Funding Fee        Margin             Liquidation Risk     │
└──────────────────────────────┬─────────────────────────────┘
                               │
                               ▼
┌────────────────────────────────────────────────────────────┐
│                         Storage                            │
│                                                            │
│  SQLite                                                    │
│  Parquet                                                   │
└────────────────────────────────────────────────────────────┘
```

---

# 5. 分层架构说明

## 5.1 User Layer

用户通过三种方式操作 Trader：

```text
CLI
REST API
WebSocket API
```

CLI 用于：

```text
初始化项目
导入数据
启动回测
启动 Replay
启动服务
生成报告
执行维护任务
```

REST API 用于：

```text
查询运行记录
查询订单
查询成交
查询持仓
查询账户
查询绩效
启动策略
停止策略
更新参数
```

WebSocket API 用于：

```text
实时行情推送
实时订单推送
实时成交推送
实时持仓推送
实时 PnL 推送
实时风控事件推送
Replay 控制
策略动态调参
```

---

## 5.2 Runtime Manager

Runtime Manager 负责管理不同运行模式。

支持：

```text
BacktestRuntime
ReplayRuntime
PaperRuntime
LiveRuntime
```

每种 Runtime 负责装配：

```text
Clock
MarketDataProvider
BrokerAdapter
Storage
EventBus
Strategy
Accounting
Metrics
RiskManager
OMS
```

---

## 5.3 BacktestRuntime

BacktestRuntime 用于一次性历史回测。

特点：

```text
使用历史行情
使用 BacktestClock
使用 BacktestBroker
使用模拟成交模型
使用 SQLite 保存回测结果
从 Parquet 读取历史行情
回测结束后生成完整绩效报告
```

数据流：

```text
Parquet Historical Data
  ↓
BacktestClock
  ↓
MarketDataProvider
  ↓
MarketEvent
  ↓
Strategy
  ↓
Insight
  ↓
PortfolioConstruction
  ↓
MarketRuleValidator
  ↓
RiskManager
  ↓
ExecutionModel
  ↓
OMS
  ↓
BacktestBroker
  ↓
FillModel
  ↓
Accounting
  ↓
Metrics
  ↓
SQLite Result
```

---

## 5.4 ReplayRuntime

ReplayRuntime 用于历史行情实时回放。

它不是普通回测，而是把历史行情按时间顺序播放，让系统像实时交易一样运行。

特点：

```text
历史行情按真实时间顺序播放
支持倍速
支持暂停
支持恢复
支持跳转
支持 WebSocket 实时观察
适合前端联调
适合策略行为观察
适合模拟实时环境
```

数据流：

```text
Parquet Historical Data
  ↓
ReplayReader
  ↓
ReplayClock
  ↓
MarketEvent
  ↓
EventBus
  ↓
Strategy Runtime
  ↓
Portfolio / Risk / OMS
  ↓
ReplayBroker
  ↓
Accounting
  ↓
WebSocket Push
```

支持速度：

```text
1x
2x
5x
10x
50x
100x
1000x
```

---

## 5.5 PaperRuntime

PaperRuntime 用于模拟交易。

特点：

```text
使用实时行情
不发送真实订单
使用 PaperBroker
维护模拟账户
维护模拟持仓
适合实盘前测试
```

数据流：

```text
Realtime Market Data
  ↓
MarketDataAdapter
  ↓
MarketEvent
  ↓
Strategy
  ↓
PortfolioConstruction
  ↓
MarketRuleValidator
  ↓
RiskManager
  ↓
ExecutionModel
  ↓
OMS
  ↓
PaperBroker
  ↓
Accounting
  ↓
WebSocket Push
```

---

## 5.6 LiveRuntime

LiveRuntime 用于实盘交易。

LiveRuntime 后期实现。

特点：

```text
使用实时行情
使用真实 BrokerAdapter
发送真实订单
开启完整风控
支持 emergency stop
支持订单恢复
支持账户同步
支持持仓同步
支持断线重连
```

数据流：

```text
Realtime Market Data
  ↓
Strategy
  ↓
PortfolioConstruction
  ↓
MarketRuleValidator
  ↓
RiskManager
  ↓
ExecutionModel
  ↓
OMS
  ↓
LiveBroker
  ↓
Broker API / Exchange API
  ↓
OrderEvent / FillEvent
  ↓
Accounting
  ↓
Storage
```

---

# 6. Event Bus 架构

Event Bus 是 Trader 的内部通信核心。

第一版推荐使用：

```rust
tokio::sync::broadcast
tokio::sync::mpsc
```

后期可以替换为：

```text
Kafka
Redpanda
NATS
Redis Streams
```

---

## 6.1 Event Envelope

所有事件使用统一 envelope。

```rust
pub struct EventEnvelope<T> {
    pub id: EventId,
    pub run_id: RunId,
    pub ts: i64,
    pub source: String,
    pub payload: T,
}
```

字段说明：

```text
id:
  事件 ID

run_id:
  策略运行 ID

ts:
  事件时间戳，毫秒

source:
  事件来源模块

payload:
  事件内容
```

---

## 6.2 Event 类型

```rust
pub enum Event {
    Market(MarketEvent),
    Insight(InsightEvent),
    PortfolioTarget(PortfolioTargetEvent),
    OrderRequest(OrderRequestEvent),
    Order(OrderEvent),
    Fill(FillEvent),
    Position(PositionEvent),
    Portfolio(PortfolioEvent),
    Account(AccountEvent),
    Risk(RiskEvent),
    System(SystemEvent),
    Command(CommandEvent),
}
```

---

## 6.3 事件分类

### MarketEvent

行情事件。

来源：

```text
HistoricalDataProvider
ReplayReader
RealtimeMarketDataAdapter
ExchangeWebSocket
```

用途：

```text
驱动策略
更新持仓估值
更新风控状态
推送前端行情
```

---

### InsightEvent

策略信号事件。

来源：

```text
AlphaModel
Strategy
QlibResearchService
```

用途：

```text
进入 PortfolioConstruction
记录策略信号
调试策略行为
```

---

### PortfolioTargetEvent

目标仓位事件。

来源：

```text
PortfolioConstructionModel
```

用途：

```text
进入 MarketRuleValidator
进入 RiskManager
生成订单需求
```

---

### OrderRequestEvent

订单请求事件。

来源：

```text
ExecutionModel
```

用途：

```text
进入 OMS
进行订单状态管理
```

---

### OrderEvent

订单状态事件。

来源：

```text
OMS
BrokerAdapter
Exchange API
```

用途：

```text
更新订单状态
推送前端
落库
触发恢复逻辑
```

---

### FillEvent

成交事件。

来源：

```text
BrokerAdapter
BacktestBroker
PaperBroker
Exchange API
```

用途：

```text
更新持仓
更新现金
计算手续费
计算税费
计算 PnL
```

---

### RiskEvent

风控事件。

来源：

```text
MarketRuleValidator
RiskManager
OMS
BrokerAdapter
```

用途：

```text
拒单
减仓
停止策略
停止交易
推送前端
落库
```

---

# 7. Algorithm Framework 架构

Trader 的策略框架参考 Lean 的 Algorithm Framework。

核心流程：

```text
Universe Selection
  ↓
Alpha Model
  ↓
Portfolio Construction
  ↓
Market Rule Validation
  ↓
Risk Management
  ↓
Execution Model
  ↓
OMS
```

---

## 7.1 Universe Selection

Universe Selection 负责选择当前策略关注的标的池。

输入：

```text
MarketContext
HistoricalData
FundamentalData
IndexMembers
Config
```

输出：

```text
Vec<Symbol>
```

示例：

```text
沪深300
中证500
恒生科技
恒生指数
纳斯达克100
标普500
自定义股票列表
Binance USDT 交易对
OKX 永续合约列表
成交额 Top N 加密货币
```

---

## 7.2 Alpha Model

Alpha Model 负责产生预测信号。

输入：

```text
MarketSlice
HistoricalData
IndicatorState
Context
```

输出：

```text
Vec<Insight>
```

Insight 示例：

```text
symbol: US:NASDAQ:AAPL
direction: Long
confidence: 0.82
horizon: 5d
```

数字货币 Insight 示例：

```text
symbol: CRYPTO:BINANCE:BTCUSDT
direction: Long
confidence: 0.76
horizon: 4h
```

---

## 7.3 Portfolio Construction

Portfolio Construction 负责把 Insight 转成目标仓位。

输入：

```text
Vec<Insight>
PortfolioState
RiskBudget
Config
```

输出：

```text
Vec<PortfolioTarget>
```

PortfolioTarget 示例：

```text
AAPL: 20%
MSFT: 15%
NVDA: 10%
Cash: 55%
```

数字货币示例：

```text
BTCUSDT: 30%
ETHUSDT: 20%
USDT Cash: 50%
```

---

## 7.4 Market Rule Validation

Market Rule Validation 负责检查订单或目标仓位是否符合市场规则。

股票市场校验：

```text
是否交易日
是否交易时段
是否停牌
是否满足 lot size
是否满足 T+1
是否超过涨跌停
是否允许盘前盘后
是否支持碎股
```

数字货币校验：

```text
是否满足最小下单金额
是否满足价格精度
是否满足数量精度
是否满足最小数量
是否超过最大数量
是否支持 post only
是否支持 reduce only
是否超过杠杆限制
是否满足保证金要求
是否触发交易所限频
```

---

## 7.5 Risk Management

Risk Management 负责风险控制。

通用风控：

```text
最大单笔订单金额
最大单标的仓位
最大组合仓位
最大日亏损
最大回撤
最大下单频率
价格偏离保护
可用资金保护
禁止无持仓卖出
```

股票风控：

```text
A股 T+1 风险
A股涨跌停风险
港股 lot size 风险
美股盘前盘后流动性风险
```

数字货币风控：

```text
杠杆风险
保证金率风险
强平风险
资金费率风险
爆仓距离风险
交易所限频风险
滑点风险
深度不足风险
```

---

## 7.6 Execution Model

Execution Model 负责把目标仓位转成具体订单。

V1 支持：

```text
ImmediateExecution
```

后续支持：

```text
TWAP
VWAP
POV
SmartExecution
MakerOnlyExecution
IcebergExecution
```

数字货币后续可支持：

```text
PostOnlyExecution
ReduceOnlyExecution
FundingAwareExecution
MarketMakingExecution
GridExecution
```

---

# 8. 核心领域模型

## 8.1 Market

```rust
pub enum Market {
    CN,
    HK,
    US,
    Crypto,
}
```

---

## 8.2 AssetClass

```rust
pub enum AssetClass {
    Equity,
    CryptoSpot,
    CryptoPerp,
    CryptoFuture,
}
```

---

## 8.3 Symbol

```rust
pub struct Symbol {
    pub market: Market,
    pub exchange: String,
    pub code: String,
}
```

示例：

```text
CN:SSE:600519
CN:SZSE:000001
HK:HKEX:00700
US:NASDAQ:AAPL
US:NYSE:IBM
CRYPTO:BINANCE:BTCUSDT
CRYPTO:OKX:BTC-USDT-SWAP
```

---

## 8.4 Security

Security 是市场规则、费用、交易时间、合约规格、保证金模型等信息的聚合对象。

```rust
pub struct Security {
    pub symbol: Symbol,
    pub asset_class: AssetClass,
    pub currency: Currency,

    pub lot_size: Decimal,
    pub tick_size: Decimal,
    pub multiplier: Decimal,

    pub base_asset: Option<String>,
    pub quote_asset: Option<String>,
    pub settlement_asset: Option<String>,

    pub trading_calendar: Box<dyn TradingCalendar>,
    pub fee_model: Box<dyn FeeModel>,
    pub settlement_model: Box<dyn SettlementModel>,
    pub price_limit_model: Box<dyn PriceLimitModel>,
    pub margin_model: Option<Box<dyn MarginModel>>,
    pub corporate_action_model: Option<Box<dyn CorporateActionModel>>,
}
```

Security 职责：

```text
判断是否可交易
提供最小价格单位
提供最小交易单位
计算手续费
计算税费
计算资金费率
判断涨跌停
判断交易时段
判断保证金要求
处理复权、拆股、分红
提供合约规格
```

---

# 9. 市场规则架构

## 9.1 统一接口

```rust
pub trait MarketRuleValidator {
    fn validate_order(
        &self,
        ctx: &MarketContext,
        order: &OrderRequest,
    ) -> Result<(), MarketRuleError>;
}
```

---

## 9.2 A股规则

A股需要支持：

```text
T+1
100股整数手
涨跌停
ST 5% 涨跌停
普通股票 10% 涨跌停
创业板 / 科创板 20% 涨跌停
北交所规则
停牌
午休
集合竞价
复权
分红
印花税
过户费
佣金
```

A股校验器：

```text
CNMarketRuleValidator
  ├── CNTradingTimeValidator
  ├── CNLotSizeValidator
  ├── CNT1Validator
  ├── CNPriceLimitValidator
  ├── CNSuspensionValidator
  └── CNFeeValidator
```

A股订单校验流程：

```text
OrderRequest
  ↓
是否交易日
  ↓
是否交易时段
  ↓
是否停牌
  ↓
是否满足 100 股整数手
  ↓
是否满足 T+1 可卖数量
  ↓
是否超过涨跌停价格
  ↓
是否满足资金要求
```

---

## 9.3 港股规则

港股需要支持：

```text
不同股票不同每手股数
T+2 结算
午休
无固定涨跌停
印花税
交易征费
交易费
财汇局交易征费
可选港股通规则
```

港股校验器：

```text
HKMarketRuleValidator
  ├── HKTradingTimeValidator
  ├── HKLotSizeValidator
  ├── HKFeeValidator
  ├── HKSettlementValidator
  └── HKCorporateActionValidator
```

港股订单校验流程：

```text
OrderRequest
  ↓
是否交易日
  ↓
是否交易时段
  ↓
是否满足该股票 lot size
  ↓
是否满足资金要求
  ↓
是否满足价格档位
```

---

## 9.4 美股规则

美股需要支持：

```text
T+1 结算
盘前交易
盘后交易
整股
碎股
拆股
分红
LULD 波动限制
做空规则
SEC Fee
TAF Fee
佣金
```

美股校验器：

```text
USMarketRuleValidator
  ├── USTradingTimeValidator
  ├── USFractionalShareValidator
  ├── USLuldValidator
  ├── USShortSellValidator
  ├── USFeeValidator
  └── USCorporateActionValidator
```

美股订单校验流程：

```text
OrderRequest
  ↓
是否交易日
  ↓
是否允许盘前/盘后
  ↓
是否支持碎股
  ↓
是否满足资金要求
  ↓
是否触发 LULD 限制
```

---

## 9.5 数字货币规则

数字货币需要支持：

```text
7x24 交易
交易所维护期
最小下单金额
最小下单数量
价格精度
数量精度
maker / taker 手续费
post only
reduce only
杠杆
逐仓 / 全仓
资金费率
保证金率
强平价格
交易所限频
```

数字货币校验器：

```text
CryptoMarketRuleValidator
  ├── CryptoTradingStatusValidator
  ├── CryptoMinNotionalValidator
  ├── CryptoPricePrecisionValidator
  ├── CryptoQtyPrecisionValidator
  ├── CryptoLeverageValidator
  ├── CryptoMarginValidator
  ├── CryptoReduceOnlyValidator
  ├── CryptoPostOnlyValidator
  └── CryptoRateLimitValidator
```

数字货币订单校验流程：

```text
OrderRequest
  ↓
交易所是否可用
  ↓
交易对是否可交易
  ↓
是否满足最小名义金额
  ↓
是否满足数量精度
  ↓
是否满足价格精度
  ↓
是否满足杠杆限制
  ↓
是否满足保证金要求
  ↓
是否符合 reduce only
  ↓
是否符合 post only
  ↓
是否触发交易所限频
```

---

# 10. OMS 架构

OMS 是 Trader 的订单管理核心。

策略不能直接下单到 Broker。

所有订单必须经过：

```text
ExecutionModel
  ↓
OMS
  ↓
BrokerAdapter
```

---

## 10.1 OMS 职责

OMS 负责：

```text
生成 client_order_id
维护 broker_order_id
维护订单状态机
处理部分成交
处理撤单
处理拒单
处理订单超时
处理重复回报
处理乱序回报
处理本地恢复
同步 Broker 状态
落库订单事件
```

---

## 10.2 订单状态机

标准路径：

```text
New
  ↓
PendingSubmit
  ↓
Submitted
  ↓
PartiallyFilled
  ↓
Filled
```

撤单路径：

```text
Submitted
  ↓
PendingCancel
  ↓
Cancelled
```

拒单路径：

```text
PendingSubmit
  ↓
Rejected
```

过期路径：

```text
Submitted
  ↓
Expired
```

异常恢复路径：

```text
Submitted
  ↓
Unknown
  ↓
Syncing
  ↓
Submitted / PartiallyFilled / Filled / Cancelled / Rejected
```

---

## 10.3 Order ID

Trader 使用两类订单 ID：

```text
client_order_id
broker_order_id
```

client_order_id：

```text
Trader 本地生成
全局唯一
用于幂等
用于恢复
用于重试
```

broker_order_id：

```text
Broker 返回
可能延迟返回
可能为空
需要和 client_order_id 映射
```

---

## 10.4 数字货币订单扩展

数字货币订单需要支持：

```text
reduce_only
post_only
leverage
margin_mode
position_side
client_order_id
exchange_order_id
```

position_side：

```text
LONG
SHORT
NET
```

margin_mode：

```text
CROSS
ISOLATED
```

---

# 11. Broker Adapter 架构

BrokerAdapter 是 Trader 连接撮合引擎、模拟账户或真实交易接口的统一接口。

---

## 11.1 统一接口

```rust
#[async_trait::async_trait]
pub trait BrokerAdapter {
    async fn connect(&self) -> Result<()>;

    async fn disconnect(&self) -> Result<()>;

    async fn place_order(&self, order: OrderRequest) -> Result<OrderAck>;

    async fn cancel_order(&self, order_id: OrderId) -> Result<CancelAck>;

    async fn account_snapshot(&self) -> Result<AccountSnapshot>;

    async fn positions(&self) -> Result<Vec<Position>>;

    async fn open_orders(&self) -> Result<Vec<Order>>;

    async fn stream_order_events(&self) -> Result<OrderEventStream>;
}
```

---

## 11.2 Broker 实现

V1 实现：

```text
BacktestBroker
ReplayBroker
PaperBroker
```

V2 实现：

```text
LiveBroker
```

股票 Broker 适配方向：

```text
FutuBroker
InteractiveBrokersBroker
LongPortBroker
TigerBroker
AlpacaBroker
```

数字货币 Broker 适配方向：

```text
BinanceBroker
OKXBroker
BybitBroker
BitgetBroker
GateBroker
```

---

# 12. Market Data 架构

Market Data 负责历史行情和实时行情。

---

## 12.1 数据源类型

支持：

```text
CSV
Parquet
REST API
WebSocket API
Broker API
Exchange API
```

---

## 12.2 数据类型

股票数据：

```text
Daily Candle
Minute Candle
Tick
Corporate Action
Fundamental
Index Member
Trading Calendar
```

数字货币数据：

```text
Spot Candle
Perp Candle
Tick
OrderBook
Trade
Funding Rate
Open Interest
Liquidation
Index Price
Mark Price
```

---

## 12.3 MarketDataProvider

统一接口：

```rust
pub trait MarketDataProvider {
    async fn history(
        &self,
        request: HistoryRequest,
    ) -> Result<MarketDataFrame>;

    async fn subscribe(
        &self,
        symbols: Vec<Symbol>,
    ) -> Result<MarketDataStream>;
}
```

---

# 13. Backtest 架构

Backtest 用于历史回测。

---

## 13.1 Backtest 数据流

```text
Parquet Historical Data
  ↓
BacktestClock
  ↓
MarketDataProvider
  ↓
MarketEvent
  ↓
Strategy
  ↓
Insight
  ↓
PortfolioConstruction
  ↓
MarketRuleValidator
  ↓
RiskManager
  ↓
ExecutionModel
  ↓
OMS
  ↓
BacktestBroker
  ↓
FillModel
  ↓
Accounting
  ↓
Metrics
  ↓
SQLite Result
```

---

## 13.2 FillModel

V1 支持：

```text
ClosePriceFillModel
OpenPriceFillModel
NextBarOpenFillModel
```

后续支持：

```text
BidAskFillModel
OrderBookFillModel
QueuePositionFillModel
LatencyAwareFillModel
```

数字货币后续支持：

```text
OrderBookDepthFillModel
MakerTakerFillModel
FundingAwareFillModel
LatencyAwarePerpFillModel
```

---

## 13.3 SlippageModel

V1 支持：

```text
FixedBpsSlippageModel
```

后续支持：

```text
VolumeShareSlippageModel
SpreadBasedSlippageModel
MarketImpactModel
OrderBookSlippageModel
```

---

# 14. Replay 架构

Replay 是 Trader V1 的重点功能。

Replay 让历史行情像实时行情一样流动，用于观察策略在历史市场中的实时反应。

---

## 14.1 Replay 数据流

```text
Parquet Historical Data
  ↓
ReplayReader
  ↓
ReplayClock
  ↓
MarketEvent
  ↓
EventBus
  ↓
Strategy Runtime
  ↓
OMS / Risk / Portfolio
  ↓
ReplayBroker
  ↓
Accounting
  ↓
WebSocket Push
```

---

## 14.2 Replay 功能

Replay 支持：

```text
start
pause
resume
stop
seek
speed
```

速度支持：

```text
1x
2x
5x
10x
50x
100x
1000x
```

Replay 控制消息：

```json
{
  "type": "ReplayControl",
  "action": "pause"
}
```

```json
{
  "type": "ReplayControl",
  "action": "seek",
  "ts": 1700000000000
}
```

---

# 15. Paper Trading 架构

Paper Trading 用实时行情模拟交易。

```text
Realtime Market Data
  ↓
MarketDataAdapter
  ↓
MarketEvent
  ↓
Strategy
  ↓
PortfolioConstruction
  ↓
MarketRuleValidator
  ↓
RiskManager
  ↓
ExecutionModel
  ↓
OMS
  ↓
PaperBroker
  ↓
Accounting
  ↓
WebSocket Push
```

PaperBroker 不发送真实订单，只在本地模拟成交。

---

# 16. Live Trading 架构

Live Trading 后期实现。

Live Trading 必须满足：

```text
完整风控
订单恢复
断线重连
账户同步
持仓同步
订单同步
Emergency Stop
操作审计
交易所限频
API Key 权限控制
```

Live Trading 数据流：

```text
Realtime Market Data
  ↓
Strategy
  ↓
PortfolioConstruction
  ↓
MarketRuleValidator
  ↓
RiskManager
  ↓
ExecutionModel
  ↓
OMS
  ↓
LiveBroker
  ↓
Broker API / Exchange API
  ↓
OrderEvent / FillEvent
  ↓
Accounting
  ↓
Storage
```

---

# 17. Accounting 架构

Accounting 负责账户、现金、持仓、PnL。

---

## 17.1 股票 Accounting

股票账户需要维护：

```text
Cash
AvailableCash
FrozenCash
PositionQty
AvailableQty
AvgPrice
MarketValue
RealizedPnL
UnrealizedPnL
Fee
Tax
```

A股特殊点：

```text
qty
available_qty
```

买入当天：

```text
qty 增加
available_qty 不增加
```

下一个交易日：

```text
available_qty 增加
```

---

## 17.2 数字货币 Accounting

数字货币账户需要维护：

```text
Asset Balance
Available Balance
Frozen Balance
Borrowed
Interest
Position Side
Entry Price
Mark Price
Liquidation Price
Margin
Leverage
Funding Fee
RealizedPnL
UnrealizedPnL
```

现货：

```text
BTC
ETH
USDT
USDC
```

合约：

```text
Long Position
Short Position
Margin
Funding Fee
Liquidation Price
```

---

# 18. Metrics 架构

Metrics 负责绩效分析。

通用指标：

```text
Total Return
Annual Return
Max Drawdown
Sharpe
Sortino
Win Rate
Profit Factor
Turnover
Fee
Tax
Slippage
Order Fill Rate
Cancel Rate
```

股票补充指标：

```text
Benchmark Return
Alpha
Beta
Tracking Error
Information Ratio
```

数字货币补充指标：

```text
Funding Fee
Maker Ratio
Taker Ratio
Liquidation Distance
Margin Ratio
Leverage Usage
Open Interest Exposure
```

---

# 19. Storage 架构

Trader 使用 SQLite + Parquet。

---

## 19.1 SQLite

SQLite 存储：

```text
strategy_runs
instruments
orders
order_events
fills
positions
crypto_positions
account_balances
portfolio_snapshots
cash_snapshots
risk_events
configs
system_logs
```

SQLite 适合：

```text
状态数据
订单数据
成交数据
账户数据
结果数据
配置数据
```

---

## 19.2 Parquet

Parquet 存储：

```text
daily candles
minute candles
ticks
orderbook
trades
fundamentals
corporate_actions
features
funding_rate
open_interest
mark_price
index_price
```

目录建议：

```text
datasets/
├── cn/
│   ├── daily/
│   ├── 1m/
│   ├── tick/
│   ├── fundamentals/
│   └── corporate_actions/
├── hk/
│   ├── daily/
│   ├── 1m/
│   ├── tick/
│   ├── fundamentals/
│   └── corporate_actions/
├── us/
│   ├── daily/
│   ├── 1m/
│   ├── tick/
│   ├── fundamentals/
│   └── corporate_actions/
└── crypto/
    ├── spot/
    │   ├── 1m/
    │   ├── tick/
    │   ├── trade/
    │   └── orderbook/
    ├── perp/
    │   ├── 1m/
    │   ├── tick/
    │   ├── trade/
    │   ├── orderbook/
    │   ├── funding_rate/
    │   ├── open_interest/
    │   ├── mark_price/
    │   └── index_price/
    └── features/
```

---

# 20. API 架构

API 分两类：

```text
REST API
WebSocket API
```

REST 用于查询和控制。

WebSocket 用于实时推送和实时控制。

---

## 20.1 REST API

REST 主要提供：

```text
POST /strategies/start
POST /strategies/stop
POST /strategies/update

POST /replay/start
POST /replay/pause
POST /replay/resume
POST /replay/seek
POST /replay/stop

GET /runs
GET /orders
GET /fills
GET /positions
GET /accounts
GET /portfolio
GET /metrics
GET /risk-events
```

---

## 20.2 WebSocket API

WebSocket channel：

```text
market
orders
fills
positions
portfolio
accounts
risk
system
replay
```

统一消息格式：

```json
{
  "channel": "portfolio",
  "type": "PortfolioSnapshot",
  "ts": 1700000000000,
  "data": {}
}
```

---

# 21. Dashboard 架构

Dashboard 建议使用：

```text
Next.js
TypeScript
Tailwind CSS
shadcn/ui
TanStack Query
ECharts
```

页面：

```text
Overview
Strategies
Replay
Orders
Fills
Positions
Accounts
Portfolio
Risk
Metrics
Settings
```

数字货币额外页面：

```text
Funding
Margin
Liquidation Risk
Exchange Status
OrderBook
```

Dashboard 只通过 API 访问 Trader，不直接访问数据库。

---

# 22. Qlib 集成架构

V1 不直接集成 Qlib。

V2 增加 Python Research Service。

```text
Qlib / Python Research Service
  ↓
Alpha Signal API
  ↓
Trader Engine
```

Research Service 负责：

```text
因子工程
模型训练
模型评估
预测信号生成
```

Trader Engine 负责：

```text
组合构建
风控
订单
执行
OMS
实盘
监控
```

Qlib 输出示例：

```json
{
  "symbol": "US:NASDAQ:AAPL",
  "direction": "Long",
  "confidence": 0.83,
  "target_weight": 0.12,
  "horizon": "5d"
}
```

---

# 23. Rust Workspace 架构

建议目录：

```text
Trader/
├── apps/
│   ├── trader-cli/
│   ├── trader-server/
├── crates/
│   ├── core/
│   ├── events/
│   ├── config/
│   ├── storage/
│   ├── data/
│   ├── market_rules/
│   ├── universe/
│   ├── alpha/
│   ├── portfolio/
│   ├── risk/
│   ├── execution/
│   ├── oms/
│   ├── broker/
│   ├── backtest/
│   ├── replay/
│   ├── accounting/
│   ├── metrics/
│   ├── api/
│   ├── indicators/
│   ├── feature_store/
│   └── strategies/
├── configs/
├── migrations/
├── datasets/
├── docs/
└── scripts/
```

---

# 24. Crate 职责

## 24.1 core

负责：

```text
Symbol
Market
AssetClass
Security
Order
Fill
Position
Portfolio
Money
Decimal
Time
Error
```

---

## 24.2 events

负责：

```text
Event
EventBus
EventEnvelope
EventStore
EventPublisher
EventSubscriber
```

---

## 24.3 config

负责：

```text
TOML config
Environment config
Runtime config
Strategy config
Broker config
Market config
```

---

## 24.4 storage

负责：

```text
SQLite
Parquet
Repository
Migration
DataWriter
DataReader
```

---

## 24.5 data

负责：

```text
HistoricalDataProvider
RealtimeDataProvider
MarketSlice
Candle
Tick
OrderBook
FundingRate
OpenInterest
```

---

## 24.6 market_rules

负责：

```text
CNMarketRuleValidator
HKMarketRuleValidator
USMarketRuleValidator
CryptoMarketRuleValidator
TradingCalendar
FeeModel
SettlementModel
LotSizeModel
PriceLimitModel
MarginModel
FundingModel
```

---

## 24.7 universe

负责：

```text
UniverseSelectionModel
StaticUniverse
IndexUniverse
FilterUniverse
CryptoTopVolumeUniverse
```

---

## 24.8 alpha

负责：

```text
AlphaModel
Insight
MA Alpha
Momentum Alpha
Mean Reversion Alpha
FundingRate Alpha
OrderBookImbalance Alpha
```

---

## 24.9 portfolio

负责：

```text
PortfolioConstructionModel
EqualWeight
FixedWeight
RiskParity
TargetWeight
CryptoAllocation
```

---

## 24.10 risk

负责：

```text
RiskManagementModel
MaxPositionRisk
MaxDrawdownRisk
DailyLossRisk
PriceDeviationRisk
LeverageRisk
LiquidationRisk
FundingRateRisk
```

---

## 24.11 execution

负责：

```text
ExecutionModel
ImmediateExecution
TWAP
VWAP
PostOnlyExecution
ReduceOnlyExecution
```

---

## 24.12 oms

负责：

```text
OrderManager
OrderStateMachine
OrderRepository
OrderRecovery
OrderIdMapping
```

---

## 24.13 broker

负责：

```text
BrokerAdapter
BacktestBroker
ReplayBroker
PaperBroker
LiveBroker
StockBroker
CryptoExchangeBroker
```

---

## 24.14 backtest

负责：

```text
BacktestRuntime
BacktestClock
FillModel
SlippageModel
BacktestReport
```

---

## 24.15 replay

负责：

```text
ReplayRuntime
ReplayClock
ReplayReader
ReplayController
ReplaySpeed
```

---

## 24.16 accounting

负责：

```text
CashBook
BalanceBook
PositionBook
PortfolioBook
PnL
Fees
Taxes
FundingFee
Margin
```

---

## 24.17 metrics

负责：

```text
Return
Drawdown
Sharpe
Sortino
WinRate
Turnover
FundingFeeMetrics
LeverageMetrics
```

---

## 24.18 api

负责：

```text
REST API
WebSocket API
Server State
Auth
Command Handler
```

---

## 24.19 strategies

负责：

```text
Example Strategies
MA Cross
RSI
Momentum
Grid
Funding Arbitrage
Market Making
```

---

# 25. 依赖方向

依赖必须单向。

```text
core
  ↑
events
  ↑
data / storage / market_rules
  ↑
alpha / portfolio / risk / execution
  ↑
oms
  ↑
broker
  ↑
runtime
  ↑
api / app
```

禁止：

```text
strategy -> broker
strategy -> storage
strategy -> api
broker -> strategy
storage -> strategy
api -> strategy internals
```

---

# 26. V1 交付范围

V1 必须完成：

```text
Rust Workspace
Core Types
Event Bus
SQLite Storage
Parquet Historical Data
Backtest Runtime
Replay Runtime
A股 Market Rules
港股 Market Rules
美股 Market Rules
Crypto Market Rules
OMS
Risk Manager
Execution Model
Accounting
Metrics
REST API
WebSocket API
Example Strategies
```

V1 不包含：

```text
Qlib
真实 Live Broker
高频 OrderBook 完整撮合
分布式部署
多用户系统
多账户权限系统
期权
传统期货
外汇
```

---

# 27. V1 成功标准

Trader V1 成功标准：

```text
同一个策略可以跑 A股 / 港股 / 美股 / 数字货币
同一个策略可以跑 Backtest / Replay / Paper
所有订单经过 MarketRule / Risk / OMS
所有交易状态可以落库
历史行情可以从 Parquet 扫描
数字货币支持现货和永续的基础抽象
A股支持 T+1 和 100 股整数手
港股支持 lot size
美股支持盘前盘后和碎股配置
Crypto 支持最小名义金额、精度、杠杆、保证金校验
前端可以通过 WebSocket 实时观察策略运行
Replay 可以按倍速模拟实时行情
```

---

# 28. 架构结论

Trader 的核心架构是：

```text
Lean Algorithm Framework
  +
Rust Event Bus
  +
SQLite State Storage
  +
Parquet Historical Data
  +
OMS Order State Machine
  +
CN / HK / US / CRYPTO Market Rules
  +
Replay Runtime
  +
WebSocket Dashboard
```

Trader 第一阶段的目标不是直接做复杂实盘，而是先完成一个稳定的多市场量化交易内核。

系统的核心价值在于：

```text
统一策略接口
统一事件流
统一订单管理
统一风控链路
统一账户与持仓模型
统一回测 / 回放 / 模拟 / 实盘架构
```

```
```
