# Trader Crates Design

Version: v1.0  
Status: Draft  
Language: Rust  
Project Type: Server-side Quant Trading System  
Target Markets: A股 / 港股 / 美股 / 数字货币  

---

# 1. 设计目标

Trader 使用 Rust Workspace 组织代码。

目标是将交易系统拆分为多个边界清晰、职责单一、依赖方向明确的 crate。

核心目标：

```text
模块解耦
职责清晰
便于测试
便于替换实现
便于后续扩展 Broker / DataSource / Strategy
避免循环依赖
避免策略直接访问 Broker / Storage / API
```

Trader 是服务端项目，不包含 Dashboard 前端。

应用层只包含：

```text
trader-cli
trader-server
```

---

# 2. Workspace 总体结构

```text
Trader/
├── Cargo.toml
├── apps/
│   ├── trader-cli/
│   └── trader-server/
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

# 3. Workspace Cargo.toml

根目录 `Cargo.toml`：

```toml
[workspace]
resolver = "2"

members = [
    "apps/trader-cli",
    "apps/trader-server",

    "crates/core",
    "crates/events",
    "crates/config",
    "crates/storage",
    "crates/data",
    "crates/market_rules",
    "crates/universe",
    "crates/alpha",
    "crates/portfolio",
    "crates/risk",
    "crates/execution",
    "crates/oms",
    "crates/broker",
    "crates/backtest",
    "crates/replay",
    "crates/accounting",
    "crates/metrics",
    "crates/api",
    "crates/indicators",
    "crates/feature_store",
    "crates/strategies",
]

[workspace.package]
edition = "2021"
license = "MIT"
version = "0.1.0"
authors = ["Trader Team"]
repository = ""
rust-version = "1.78"

[workspace.dependencies]
anyhow = "1"
thiserror = "2"
tokio = { version = "1", features = ["full"] }
async-trait = "0.1"
futures = "0.3"

serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"

uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
time = "0.3"

rust_decimal = { version = "1", features = ["serde"] }
rust_decimal_macros = "1"

tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }

sqlx = { version = "0.8", features = ["sqlite", "runtime-tokio", "chrono", "uuid"] }

polars = { version = "0.50", features = ["lazy", "parquet", "temporal", "dtype-decimal"] }

axum = { version = "0.8", features = ["ws", "macros"] }
tower = "0.5"
tower-http = { version = "0.6", features = ["cors", "trace"] }

clap = { version = "4", features = ["derive"] }

reqwest = { version = "0.12", features = ["json", "rustls-tls"] }
tokio-tungstenite = "0.26"

parking_lot = "0.12"
dashmap = "6"
```

---

# 4. 应用层 Apps

---

# 4.1 trader-cli

路径：

```text
apps/trader-cli/
```

用途：

```text
一次性命令行工具
数据导入
数据库迁移
回测
Replay 启动
配置检查
报告生成
维护任务
```

命令示例：

```bash
trader init

trader migrate

trader import \
  --market crypto \
  --exchange binance \
  --symbol BTCUSDT \
  --timeframe 1m \
  --file datasets/raw/btcusdt_1m.csv

trader backtest \
  --config configs/backtest/ma_cross.toml

trader replay \
  --config configs/replay/btc_replay.toml \
  --speed 10x

trader report \
  --run-id run_001

trader check-config \
  --config configs/server.toml
```

职责：

```text
解析 CLI 参数
加载配置
调用 Runtime
调用 Storage Migration
调用 Importer
输出报告
```

不负责：

```text
长期运行
WebSocket 连接维护
REST API 服务
Broker 长连接
实时行情订阅
```

依赖：

```text
core
config
storage
data
backtest
replay
metrics
strategies
```

---

## 4.1.1 trader-cli Cargo.toml

```toml
[package]
name = "trader-cli"
edition.workspace = true
version.workspace = true

[[bin]]
name = "trader"
path = "src/main.rs"

[dependencies]
trader-core = { path = "../../crates/core" }
trader-config = { path = "../../crates/config" }
trader-storage = { path = "../../crates/storage" }
trader-data = { path = "../../crates/data" }
trader-backtest = { path = "../../crates/backtest" }
trader-replay = { path = "../../crates/replay" }
trader-metrics = { path = "../../crates/metrics" }
trader-strategies = { path = "../../crates/strategies" }

anyhow.workspace = true
tokio.workspace = true
clap.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
```

---

# 4.2 trader-server

路径：

```text
apps/trader-server/
```

用途：

```text
常驻服务进程
REST API
WebSocket API
Paper Trading
Live Trading
Replay 控制
策略启动 / 停止
实时状态推送
订单事件监听
Broker 长连接
行情长连接
```

启动示例：

```bash
trader-server --config configs/server.toml
```

职责：

```text
加载服务端配置
启动 API Server
启动 Runtime Manager
维护 Event Bus
维护 Strategy Runtime
维护 Broker 连接
维护 Market Data 连接
推送 WebSocket 事件
处理控制命令
```

不负责：

```text
前端 Dashboard
离线报告展示
复杂交互 UI
```

依赖：

```text
core
events
config
storage
data
market_rules
universe
alpha
portfolio
risk
execution
oms
broker
backtest
replay
accounting
metrics
api
strategies
```

---

## 4.2.1 trader-server Cargo.toml

```toml
[package]
name = "trader-server"
edition.workspace = true
version.workspace = true

[[bin]]
name = "trader-server"
path = "src/main.rs"

[dependencies]
trader-core = { path = "../../crates/core" }
trader-events = { path = "../../crates/events" }
trader-config = { path = "../../crates/config" }
trader-storage = { path = "../../crates/storage" }
trader-data = { path = "../../crates/data" }
trader-market-rules = { path = "../../crates/market_rules" }
trader-universe = { path = "../../crates/universe" }
trader-alpha = { path = "../../crates/alpha" }
trader-portfolio = { path = "../../crates/portfolio" }
trader-risk = { path = "../../crates/risk" }
trader-execution = { path = "../../crates/execution" }
trader-oms = { path = "../../crates/oms" }
trader-broker = { path = "../../crates/broker" }
trader-backtest = { path = "../../crates/backtest" }
trader-replay = { path = "../../crates/replay" }
trader-accounting = { path = "../../crates/accounting" }
trader-metrics = { path = "../../crates/metrics" }
trader-api = { path = "../../crates/api" }
trader-strategies = { path = "../../crates/strategies" }

anyhow.workspace = true
tokio.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
```

---

# 5. Crate 分层

Trader 的 crate 分为 6 层。

```text
Layer 0: Foundation
  core

Layer 1: Infrastructure
  events
  config
  storage

Layer 2: Data & Market
  data
  market_rules
  indicators
  feature_store

Layer 3: Algorithm Framework
  universe
  alpha
  portfolio
  risk
  execution

Layer 4: Trading Engine
  oms
  broker
  accounting
  metrics
  backtest
  replay

Layer 5: Interface
  api
  strategies
  trader-cli
  trader-server
```

---

# 6. 依赖方向

依赖必须单向。

```text
core
  ↑
events / config / storage
  ↑
data / market_rules / indicators / feature_store
  ↑
universe / alpha / portfolio / risk / execution
  ↑
oms / broker / accounting / metrics
  ↑
backtest / replay / api / strategies
  ↑
apps
```

禁止循环依赖。

禁止：

```text
strategy -> broker
strategy -> storage
strategy -> api
broker -> strategy
storage -> strategy
api -> strategy internals
core -> any crate
```

允许：

```text
strategy -> alpha
strategy -> indicators
strategy -> core
strategy -> data types
```

---

# 7. core crate

路径：

```text
crates/core/
```

包名：

```text
trader-core
```

职责：

```text
系统核心领域类型
不依赖任何业务 crate
所有 crate 可以依赖 core
```

包含：

```text
Market
AssetClass
Symbol
Security
Currency
Money
Quantity
Price
Order
OrderRequest
OrderStatus
OrderType
Side
Fill
Position
CryptoPosition
AccountBalance
Portfolio
Insight
PortfolioTarget
TimeRange
Error
Result
```

目录结构：

```text
crates/core/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── market.rs
    ├── symbol.rs
    ├── security.rs
    ├── asset.rs
    ├── money.rs
    ├── order.rs
    ├── fill.rs
    ├── position.rs
    ├── account.rs
    ├── portfolio.rs
    ├── signal.rs
    ├── target.rs
    ├── time.rs
    └── error.rs
```

---

## 7.1 core Cargo.toml

```toml
[package]
name = "trader-core"
edition.workspace = true
version.workspace = true

[dependencies]
serde.workspace = true
serde_json.workspace = true
uuid.workspace = true
chrono.workspace = true
rust_decimal.workspace = true
thiserror.workspace = true
```

---

## 7.2 core 依赖规则

core 只能依赖：

```text
serde
uuid
chrono/time
rust_decimal
thiserror
```

core 不能依赖：

```text
tokio
sqlx
axum
polars
reqwest
broker
storage
api
```

---

# 8. events crate

路径：

```text
crates/events/
```

包名：

```text
trader-events
```

职责：

```text
Event 定义
EventEnvelope
EventBus
EventPublisher
EventSubscriber
CommandEvent
SystemEvent
```

目录结构：

```text
crates/events/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── envelope.rs
    ├── event.rs
    ├── bus.rs
    ├── publisher.rs
    ├── subscriber.rs
    ├── command.rs
    └── system.rs
```

---

## 8.1 events Cargo.toml

```toml
[package]
name = "trader-events"
edition.workspace = true
version.workspace = true

[dependencies]
trader-core = { path = "../core" }

anyhow.workspace = true
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
uuid.workspace = true
chrono.workspace = true
tracing.workspace = true
```

---

## 8.2 events 依赖规则

events 可以依赖：

```text
core
tokio
serde
tracing
```

events 不能依赖：

```text
storage
broker
api
strategies
```

---

# 9. config crate

路径：

```text
crates/config/
```

包名：

```text
trader-config
```

职责：

```text
配置加载
配置校验
TOML / JSON 配置解析
环境变量覆盖
RuntimeConfig
StrategyConfig
BrokerConfig
DataConfig
RiskConfig
ServerConfig
```

目录结构：

```text
crates/config/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── loader.rs
    ├── validator.rs
    ├── runtime.rs
    ├── strategy.rs
    ├── broker.rs
    ├── data.rs
    ├── risk.rs
    ├── server.rs
    └── error.rs
```

---

## 9.1 config Cargo.toml

```toml
[package]
name = "trader-config"
edition.workspace = true
version.workspace = true

[dependencies]
trader-core = { path = "../core" }

anyhow.workspace = true
serde.workspace = true
serde_json.workspace = true
toml.workspace = true
thiserror.workspace = true
```

---

# 10. storage crate

路径：

```text
crates/storage/
```

包名：

```text
trader-storage
```

职责：

```text
SQLite 连接池
Migration
Repository
Parquet Reader
Parquet Writer
数据导入
状态恢复查询
```

目录结构：

```text
crates/storage/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── sqlite/
    │   ├── mod.rs
    │   ├── pool.rs
    │   ├── migration.rs
    │   ├── strategy_run_repo.rs
    │   ├── order_repo.rs
    │   ├── fill_repo.rs
    │   ├── position_repo.rs
    │   ├── account_repo.rs
    │   ├── portfolio_repo.rs
    │   ├── risk_repo.rs
    │   └── config_repo.rs
    ├── parquet/
    │   ├── mod.rs
    │   ├── candle_reader.rs
    │   ├── candle_writer.rs
    │   ├── tick_reader.rs
    │   ├── orderbook_reader.rs
    │   ├── funding_reader.rs
    │   └── feature_reader.rs
    └── error.rs
```

---

## 10.1 storage Cargo.toml

```toml
[package]
name = "trader-storage"
edition.workspace = true
version.workspace = true

[dependencies]
trader-core = { path = "../core" }
trader-config = { path = "../config" }

anyhow.workspace = true
async-trait.workspace = true
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
sqlx.workspace = true
polars.workspace = true
rust_decimal.workspace = true
tracing.workspace = true
thiserror.workspace = true
```

---

## 10.2 storage 依赖规则

storage 可以依赖：

```text
core
config
sqlx
polars
```

storage 不能依赖：

```text
strategy
broker
api
runtime
oms
```

---

# 11. data crate

路径：

```text
crates/data/
```

包名：

```text
trader-data
```

职责：

```text
历史行情接口
实时行情接口
MarketSlice
Candle
Tick
Trade
OrderBook
FundingRate
OpenInterest
MarketDataProvider
RealtimeDataProvider
```

目录结构：

```text
crates/data/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── candle.rs
    ├── tick.rs
    ├── trade.rs
    ├── orderbook.rs
    ├── funding.rs
    ├── open_interest.rs
    ├── slice.rs
    ├── history.rs
    ├── provider.rs
    ├── realtime.rs
    ├── csv.rs
    ├── parquet.rs
    └── error.rs
```

---

## 11.1 data Cargo.toml

```toml
[package]
name = "trader-data"
edition.workspace = true
version.workspace = true

[dependencies]
trader-core = { path = "../core" }
trader-storage = { path = "../storage" }

anyhow.workspace = true
async-trait.workspace = true
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
polars.workspace = true
rust_decimal.workspace = true
tracing.workspace = true
thiserror.workspace = true
```

---

# 12. market_rules crate

路径：

```text
crates/market_rules/
```

包名：

```text
trader-market-rules
```

职责：

```text
A股交易规则
港股交易规则
美股交易规则
数字货币交易规则
交易日历
交易时段
手续费模型
税费模型
最小交易单位
涨跌停
精度校验
杠杆校验
保证金校验
资金费率规则
```

目录结构：

```text
crates/market_rules/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── validator.rs
    ├── context.rs
    ├── calendar.rs
    ├── session.rs
    ├── fee.rs
    ├── settlement.rs
    ├── lot_size.rs
    ├── price_limit.rs
    ├── margin.rs
    ├── funding.rs
    ├── cn/
    │   ├── mod.rs
    │   ├── validator.rs
    │   ├── trading_time.rs
    │   ├── lot_size.rs
    │   ├── t1.rs
    │   ├── price_limit.rs
    │   └── fee.rs
    ├── hk/
    │   ├── mod.rs
    │   ├── validator.rs
    │   ├── trading_time.rs
    │   ├── lot_size.rs
    │   └── fee.rs
    ├── us/
    │   ├── mod.rs
    │   ├── validator.rs
    │   ├── trading_time.rs
    │   ├── fractional.rs
    │   ├── luld.rs
    │   └── fee.rs
    └── crypto/
        ├── mod.rs
        ├── validator.rs
        ├── precision.rs
        ├── min_notional.rs
        ├── leverage.rs
        ├── margin.rs
        ├── reduce_only.rs
        ├── post_only.rs
        ├── rate_limit.rs
        └── funding.rs
```

---

## 12.1 market_rules Cargo.toml

```toml
[package]
name = "trader-market-rules"
edition.workspace = true
version.workspace = true

[dependencies]
trader-core = { path = "../core" }
trader-data = { path = "../data" }

anyhow.workspace = true
async-trait.workspace = true
serde.workspace = true
serde_json.workspace = true
rust_decimal.workspace = true
chrono.workspace = true
thiserror.workspace = true
```

---

# 13. indicators crate

路径：

```text
crates/indicators/
```

包名：

```text
trader-indicators
```

职责：

```text
技术指标
MA
EMA
RSI
MACD
Bollinger Bands
ATR
VWAP
OrderBook Imbalance
Funding Rate Indicators
```

目录结构：

```text
crates/indicators/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── ma.rs
    ├── ema.rs
    ├── rsi.rs
    ├── macd.rs
    ├── bollinger.rs
    ├── atr.rs
    ├── vwap.rs
    ├── orderbook.rs
    └── funding.rs
```

---

## 13.1 indicators Cargo.toml

```toml
[package]
name = "trader-indicators"
edition.workspace = true
version.workspace = true

[dependencies]
trader-core = { path = "../core" }
trader-data = { path = "../data" }

rust_decimal.workspace = true
serde.workspace = true
thiserror.workspace = true
```

---

# 14. feature_store crate

路径：

```text
crates/feature_store/
```

包名：

```text
trader-feature-store
```

职责：

```text
因子数据读取
因子数据写入
FeatureFrame
FeatureQuery
未来对接 Qlib / Python Research Service
```

目录结构：

```text
crates/feature_store/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── feature.rs
    ├── query.rs
    ├── reader.rs
    ├── writer.rs
    └── error.rs
```

---

## 14.1 feature_store Cargo.toml

```toml
[package]
name = "trader-feature-store"
edition.workspace = true
version.workspace = true

[dependencies]
trader-core = { path = "../core" }
trader-storage = { path = "../storage" }

anyhow.workspace = true
async-trait.workspace = true
serde.workspace = true
serde_json.workspace = true
polars.workspace = true
thiserror.workspace = true
```

---

# 15. universe crate

路径：

```text
crates/universe/
```

包名：

```text
trader-universe
```

职责：

```text
股票池选择
交易对池选择
固定股票池
指数成分股
成交额过滤
流动性过滤
数字货币 Top Volume Universe
```

目录结构：

```text
crates/universe/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── model.rs
    ├── static_universe.rs
    ├── index_universe.rs
    ├── filter_universe.rs
    ├── crypto_volume_universe.rs
    └── error.rs
```

---

## 15.1 universe Cargo.toml

```toml
[package]
name = "trader-universe"
edition.workspace = true
version.workspace = true

[dependencies]
trader-core = { path = "../core" }
trader-data = { path = "../data" }

anyhow.workspace = true
async-trait.workspace = true
serde.workspace = true
thiserror.workspace = true
```

---

# 16. alpha crate

路径：

```text
crates/alpha/
```

包名：

```text
trader-alpha
```

职责：

```text
AlphaModel trait
Insight 生成
趋势信号
均值回归信号
动量信号
资金费率信号
订单簿信号
外部研究信号接入
```

目录结构：

```text
crates/alpha/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── model.rs
    ├── context.rs
    ├── ma_cross.rs
    ├── momentum.rs
    ├── mean_reversion.rs
    ├── funding_rate.rs
    ├── orderbook_imbalance.rs
    ├── external_signal.rs
    └── error.rs
```

---

## 16.1 alpha Cargo.toml

```toml
[package]
name = "trader-alpha"
edition.workspace = true
version.workspace = true

[dependencies]
trader-core = { path = "../core" }
trader-data = { path = "../data" }
trader-indicators = { path = "../indicators" }

anyhow.workspace = true
async-trait.workspace = true
serde.workspace = true
rust_decimal.workspace = true
thiserror.workspace = true
```

---

# 17. portfolio crate

路径：

```text
crates/portfolio/
```

包名：

```text
trader-portfolio
```

职责：

```text
PortfolioConstructionModel
Insight -> PortfolioTarget
等权组合
固定权重
风险平价
目标仓位
数字货币资金分配
```

目录结构：

```text
crates/portfolio/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── model.rs
    ├── context.rs
    ├── equal_weight.rs
    ├── fixed_weight.rs
    ├── target_weight.rs
    ├── risk_parity.rs
    ├── crypto_allocation.rs
    └── error.rs
```

---

## 17.1 portfolio Cargo.toml

```toml
[package]
name = "trader-portfolio"
edition.workspace = true
version.workspace = true

[dependencies]
trader-core = { path = "../core" }

anyhow.workspace = true
async-trait.workspace = true
serde.workspace = true
rust_decimal.workspace = true
thiserror.workspace = true
```

---

# 18. risk crate

路径：

```text
crates/risk/
```

包名：

```text
trader-risk
```

职责：

```text
RiskManagementModel
最大仓位
最大回撤
最大日亏损
价格偏离保护
交易时间保护
A股 T+1 风险
数字货币杠杆风险
数字货币强平风险
资金费率风险
```

目录结构：

```text
crates/risk/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── model.rs
    ├── context.rs
    ├── composite.rs
    ├── max_position.rs
    ├── max_drawdown.rs
    ├── daily_loss.rs
    ├── price_deviation.rs
    ├── leverage.rs
    ├── liquidation.rs
    ├── funding_rate.rs
    └── error.rs
```

---

## 18.1 risk Cargo.toml

```toml
[package]
name = "trader-risk"
edition.workspace = true
version.workspace = true

[dependencies]
trader-core = { path = "../core" }
trader-market-rules = { path = "../market_rules" }

anyhow.workspace = true
async-trait.workspace = true
serde.workspace = true
rust_decimal.workspace = true
thiserror.workspace = true
```

---

# 19. execution crate

路径：

```text
crates/execution/
```

包名：

```text
trader-execution
```

职责：

```text
ExecutionModel
PortfolioTarget -> OrderRequest
ImmediateExecution
TWAP
VWAP
PostOnlyExecution
ReduceOnlyExecution
```

目录结构：

```text
crates/execution/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── model.rs
    ├── context.rs
    ├── immediate.rs
    ├── twap.rs
    ├── vwap.rs
    ├── post_only.rs
    ├── reduce_only.rs
    └── error.rs
```

---

## 19.1 execution Cargo.toml

```toml
[package]
name = "trader-execution"
edition.workspace = true
version.workspace = true

[dependencies]
trader-core = { path = "../core" }

anyhow.workspace = true
async-trait.workspace = true
serde.workspace = true
rust_decimal.workspace = true
thiserror.workspace = true
```

---

# 20. oms crate

路径：

```text
crates/oms/
```

包名：

```text
trader-oms
```

职责：

```text
OrderManager
OrderStateMachine
OrderIdMapping
OrderRecovery
OrderEventStore
订单幂等
订单状态同步
```

目录结构：

```text
crates/oms/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── manager.rs
    ├── state_machine.rs
    ├── id.rs
    ├── recovery.rs
    ├── event_handler.rs
    ├── repository.rs
    └── error.rs
```

---

## 20.1 oms Cargo.toml

```toml
[package]
name = "trader-oms"
edition.workspace = true
version.workspace = true

[dependencies]
trader-core = { path = "../core" }
trader-events = { path = "../events" }
trader-storage = { path = "../storage" }

anyhow.workspace = true
async-trait.workspace = true
tokio.workspace = true
serde.workspace = true
uuid.workspace = true
rust_decimal.workspace = true
tracing.workspace = true
thiserror.workspace = true
```

---

# 21. broker crate

路径：

```text
crates/broker/
```

包名：

```text
trader-broker
```

职责：

```text
BrokerAdapter trait
BacktestBroker
ReplayBroker
PaperBroker
LiveBroker 抽象
股票 Broker 抽象
数字货币交易所 Broker 抽象
订单提交
撤单
账户同步
持仓同步
订单事件流
```

目录结构：

```text
crates/broker/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── adapter.rs
    ├── account.rs
    ├── backtest.rs
    ├── replay.rs
    ├── paper.rs
    ├── live.rs
    ├── stock/
    │   ├── mod.rs
    │   ├── futu.rs
    │   ├── ibkr.rs
    │   ├── longport.rs
    │   └── alpaca.rs
    ├── crypto/
    │   ├── mod.rs
    │   ├── binance.rs
    │   ├── okx.rs
    │   ├── bybit.rs
    │   └── bitget.rs
    └── error.rs
```

---

## 21.1 broker Cargo.toml

```toml
[package]
name = "trader-broker"
edition.workspace = true
version.workspace = true

[features]
default = []
stock = []
crypto = []
binance = ["crypto"]
okx = ["crypto"]
bybit = ["crypto"]
futu = ["stock"]
ibkr = ["stock"]
alpaca = ["stock"]

[dependencies]
trader-core = { path = "../core" }
trader-events = { path = "../events" }
trader-storage = { path = "../storage" }
trader-market-rules = { path = "../market_rules" }

anyhow.workspace = true
async-trait.workspace = true
tokio.workspace = true
futures.workspace = true
serde.workspace = true
serde_json.workspace = true
reqwest.workspace = true
tokio-tungstenite.workspace = true
rust_decimal.workspace = true
tracing.workspace = true
thiserror.workspace = true
```

---

# 22. accounting crate

路径：

```text
crates/accounting/
```

包名：

```text
trader-accounting
```

职责：

```text
CashBook
BalanceBook
PositionBook
PortfolioBook
PnL
Fee
Tax
FundingFee
Margin
A股 T+1 可卖数量更新
数字货币保证金和强平风险计算
```

目录结构：

```text
crates/accounting/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── cash_book.rs
    ├── balance_book.rs
    ├── position_book.rs
    ├── portfolio_book.rs
    ├── pnl.rs
    ├── fee.rs
    ├── tax.rs
    ├── funding.rs
    ├── margin.rs
    └── error.rs
```

---

## 22.1 accounting Cargo.toml

```toml
[package]
name = "trader-accounting"
edition.workspace = true
version.workspace = true

[dependencies]
trader-core = { path = "../core" }
trader-storage = { path = "../storage" }
trader-market-rules = { path = "../market_rules" }

anyhow.workspace = true
async-trait.workspace = true
serde.workspace = true
rust_decimal.workspace = true
tracing.workspace = true
thiserror.workspace = true
```

---

# 23. metrics crate

路径：

```text
crates/metrics/
```

包名：

```text
trader-metrics
```

职责：

```text
收益率
年化收益
最大回撤
Sharpe
Sortino
胜率
盈亏比
换手率
订单成交率
撤单率
资金费率统计
杠杆使用率
强平距离
```

目录结构：

```text
crates/metrics/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── returns.rs
    ├── drawdown.rs
    ├── sharpe.rs
    ├── sortino.rs
    ├── win_rate.rs
    ├── turnover.rs
    ├── order_stats.rs
    ├── funding.rs
    ├── leverage.rs
    └── report.rs
```

---

## 23.1 metrics Cargo.toml

```toml
[package]
name = "trader-metrics"
edition.workspace = true
version.workspace = true

[dependencies]
trader-core = { path = "../core" }
trader-storage = { path = "../storage" }

anyhow.workspace = true
serde.workspace = true
rust_decimal.workspace = true
polars.workspace = true
thiserror.workspace = true
```

---

# 24. backtest crate

路径：

```text
crates/backtest/
```

包名：

```text
trader-backtest
```

职责：

```text
BacktestRuntime
BacktestClock
FillModel
SlippageModel
CommissionModel
历史数据驱动策略
回测报告生成
```

目录结构：

```text
crates/backtest/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── runtime.rs
    ├── clock.rs
    ├── fill.rs
    ├── slippage.rs
    ├── commission.rs
    ├── report.rs
    └── error.rs
```

---

## 24.1 backtest Cargo.toml

```toml
[package]
name = "trader-backtest"
edition.workspace = true
version.workspace = true

[dependencies]
trader-core = { path = "../core" }
trader-events = { path = "../events" }
trader-config = { path = "../config" }
trader-storage = { path = "../storage" }
trader-data = { path = "../data" }
trader-market-rules = { path = "../market_rules" }
trader-universe = { path = "../universe" }
trader-alpha = { path = "../alpha" }
trader-portfolio = { path = "../portfolio" }
trader-risk = { path = "../risk" }
trader-execution = { path = "../execution" }
trader-oms = { path = "../oms" }
trader-broker = { path = "../broker" }
trader-accounting = { path = "../accounting" }
trader-metrics = { path = "../metrics" }

anyhow.workspace = true
async-trait.workspace = true
tokio.workspace = true
serde.workspace = true
rust_decimal.workspace = true
tracing.workspace = true
thiserror.workspace = true
```

---

# 25. replay crate

路径：

```text
crates/replay/
```

包名：

```text
trader-replay
```

职责：

```text
ReplayRuntime
ReplayClock
ReplayReader
ReplayController
历史行情按时间回放
暂停 / 恢复 / 跳转 / 倍速
WebSocket 控制接口支撑
```

目录结构：

```text
crates/replay/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── runtime.rs
    ├── clock.rs
    ├── reader.rs
    ├── controller.rs
    ├── command.rs
    ├── speed.rs
    └── error.rs
```

---

## 25.1 replay Cargo.toml

```toml
[package]
name = "trader-replay"
edition.workspace = true
version.workspace = true

[dependencies]
trader-core = { path = "../core" }
trader-events = { path = "../events" }
trader-config = { path = "../config" }
trader-storage = { path = "../storage" }
trader-data = { path = "../data" }
trader-market-rules = { path = "../market_rules" }
trader-universe = { path = "../universe" }
trader-alpha = { path = "../alpha" }
trader-portfolio = { path = "../portfolio" }
trader-risk = { path = "../risk" }
trader-execution = { path = "../execution" }
trader-oms = { path = "../oms" }
trader-broker = { path = "../broker" }
trader-accounting = { path = "../accounting" }
trader-metrics = { path = "../metrics" }

anyhow.workspace = true
async-trait.workspace = true
tokio.workspace = true
serde.workspace = true
rust_decimal.workspace = true
tracing.workspace = true
thiserror.workspace = true
```

---

# 26. api crate

路径：

```text
crates/api/
```

包名：

```text
trader-api
```

职责：

```text
REST API
WebSocket API
Command Handler
Server State
Run 查询
Order 查询
Position 查询
Portfolio 查询
Replay 控制
Strategy 控制
事件推送
```

目录结构：

```text
crates/api/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── server.rs
    ├── state.rs
    ├── error.rs
    ├── rest/
    │   ├── mod.rs
    │   ├── routes.rs
    │   ├── strategy.rs
    │   ├── replay.rs
    │   ├── orders.rs
    │   ├── positions.rs
    │   ├── portfolio.rs
    │   ├── metrics.rs
    │   └── health.rs
    └── ws/
        ├── mod.rs
        ├── handler.rs
        ├── session.rs
        ├── message.rs
        ├── channels.rs
        └── broadcaster.rs
```

---

## 26.1 api Cargo.toml

```toml
[package]
name = "trader-api"
edition.workspace = true
version.workspace = true

[dependencies]
trader-core = { path = "../core" }
trader-events = { path = "../events" }
trader-config = { path = "../config" }
trader-storage = { path = "../storage" }
trader-replay = { path = "../replay" }

anyhow.workspace = true
async-trait.workspace = true
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
axum.workspace = true
tower.workspace = true
tower-http.workspace = true
tracing.workspace = true
thiserror.workspace = true
```

---

# 27. strategies crate

路径：

```text
crates/strategies/
```

包名：

```text
trader-strategies
```

职责：

```text
示例策略
内置策略
策略注册
策略工厂
MA Cross
RSI
Momentum
Grid
Funding Arbitrage
Market Making
```

目录结构：

```text
crates/strategies/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── registry.rs
    ├── factory.rs
    ├── ma_cross.rs
    ├── rsi.rs
    ├── momentum.rs
    ├── grid.rs
    ├── funding_arbitrage.rs
    └── market_making.rs
```

---

## 27.1 strategies Cargo.toml

```toml
[package]
name = "trader-strategies"
edition.workspace = true
version.workspace = true

[dependencies]
trader-core = { path = "../core" }
trader-data = { path = "../data" }
trader-alpha = { path = "../alpha" }
trader-portfolio = { path = "../portfolio" }
trader-risk = { path = "../risk" }
trader-execution = { path = "../execution" }
trader-indicators = { path = "../indicators" }

anyhow.workspace = true
async-trait.workspace = true
serde.workspace = true
rust_decimal.workspace = true
thiserror.workspace = true
```

---

# 28. Runtime 装配关系

Runtime 不单独作为 crate，第一版分别放在：

```text
backtest::runtime
replay::runtime
server runtime manager
```

后期如果复杂度上升，可以新增：

```text
crates/runtime/
```

V1 暂不新增 runtime crate。

---

# 29. 模块调用链

标准策略运行链路：

```text
MarketDataProvider
  ↓
EventBus
  ↓
UniverseSelectionModel
  ↓
AlphaModel
  ↓
PortfolioConstructionModel
  ↓
MarketRuleValidator
  ↓
RiskManagementModel
  ↓
ExecutionModel
  ↓
OMS
  ↓
BrokerAdapter
  ↓
Accounting
  ↓
Metrics
  ↓
Storage
```

---

# 30. 禁止依赖关系

以下依赖禁止出现：

```text
trader-core -> any trader-* crate

trader-storage -> trader-broker
trader-storage -> trader-api
trader-storage -> trader-strategies

trader-broker -> trader-strategies
trader-broker -> trader-api

trader-strategies -> trader-broker
trader-strategies -> trader-storage
trader-strategies -> trader-api

trader-api -> trader-strategies internal implementation

trader-oms -> trader-strategies

trader-market-rules -> trader-oms
```

---

# 31. Feature Flags

broker crate 支持 feature flags：

```toml
[features]
default = []
stock = []
crypto = []
binance = ["crypto"]
okx = ["crypto"]
bybit = ["crypto"]
futu = ["stock"]
ibkr = ["stock"]
alpaca = ["stock"]
```

用途：

```text
默认不编译所有 Broker
需要哪个 Broker 才开启哪个 feature
减少依赖体积
减少编译时间
隔离第三方 SDK
```

示例：

```bash
cargo build -p trader-server --features "binance okx"

cargo build -p trader-server --features "futu ibkr"
```

---

# 32. Testing Strategy

每个 crate 必须包含单元测试。

测试目录：

```text
crates/*/tests/
```

推荐测试类型：

```text
core:
  类型序列化
  Decimal 精度
  Order 状态枚举

market_rules:
  A股 T+1
  港股 lot size
  美股碎股
  Crypto min_notional
  Crypto precision

oms:
  状态机流转
  部分成交
  撤单
  重复回报
  状态恢复

backtest:
  MA 策略回测
  成交模型
  滑点模型

replay:
  pause
  resume
  seek
  speed

storage:
  migration
  order repository
  position repository
```

---

# 33. 开发顺序

建议按依赖顺序开发：

```text
1. core
2. config
3. events
4. storage
5. data
6. market_rules
7. indicators
8. universe
9. alpha
10. portfolio
11. risk
12. execution
13. oms
14. broker
15. accounting
16. metrics
17. backtest
18. replay
19. api
20. strategies
21. trader-cli
22. trader-server
```

---

# 34. V1 最小可运行组合

V1 最小可运行组合：

```text
core
config
events
storage
data
market_rules
indicators
alpha
portfolio
risk
execution
oms
broker
accounting
metrics
backtest
strategies
trader-cli
```

完成后可以实现：

```text
读取 Parquet 日线
运行 MA Cross 策略
生成 Insight
生成 PortfolioTarget
经过 MarketRule
经过 Risk
生成 OrderRequest
通过 OMS
由 BacktestBroker 成交
更新 Accounting
写入 SQLite
生成 Metrics
```

---

# 35. Replay 可运行组合

Replay 需要：

```text
core
config
events
storage
data
market_rules
alpha
portfolio
risk
execution
oms
broker
accounting
metrics
replay
api
strategies
trader-server
```

完成后可以实现：

```text
历史行情按倍速播放
策略实时响应
订单状态实时推送
持仓和 PnL 实时推送
WebSocket 控制 pause / resume / seek
```

---

# 36. Paper Trading 可运行组合

Paper Trading 需要：

```text
core
config
events
storage
data
market_rules
alpha
portfolio
risk
execution
oms
broker
accounting
metrics
api
strategies
trader-server
```

完成后可以实现：

```text
接入实时行情
模拟订单成交
维护模拟账户
维护模拟持仓
WebSocket 实时推送状态
```

---

# 37. V1 结论

Trader 的 Rust Workspace 设计原则：

```text
core 最底层
events 负责事件
storage 负责持久化
data 负责行情
market_rules 负责市场规则
algorithm crates 负责策略框架
oms 负责订单生命周期
broker 负责交易接口
accounting 负责账户与持仓
metrics 负责绩效
api 负责服务接口
apps 负责最终启动程序
```

Trader 不包含前端 Dashboard。

最终应用只包含：

```text
apps/trader-cli
apps/trader-server
```

所有策略必须通过统一模型进入交易链路：

```text
Insight
  ↓
PortfolioTarget
  ↓
MarketRule
  ↓
Risk
  ↓
Execution
  ↓
OMS
  ↓
Broker
```
