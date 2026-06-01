# strategy.md

## 1. Overview

Strategy 是 Trader 中的策略逻辑模块。

Strategy 只负责：

```text
接收行情

读取特征

计算信号

发布 SignalEvent
```

Strategy 不负责：

```text
下单

风控

仓位管理

订单管理

Broker 连接

SQLite 读写
```

---

## 2. Core Rule

策略只能产生信号。

```text
Strategy
  ↓
SignalEvent
  ↓
Portfolio
  ↓
Risk
  ↓
Execution
  ↓
OMS
  ↓
Broker
```

禁止：

```text
Strategy → Broker

Strategy → OMS

Strategy → SQLite

Strategy → Order
```

---

## 3. Strategy Trait

```rust
pub trait Strategy: Send + Sync {

    fn id(&self) -> &str;

    fn name(&self) -> &str;

    fn on_start(
        &mut self,
        ctx: &mut StrategyContext,
    ) -> Result<()>;

    fn on_stop(
        &mut self,
        ctx: &mut StrategyContext,
    ) -> Result<()>;

    fn on_tick(
        &mut self,
        ctx: &mut StrategyContext,
        tick: &TickEvent,
    ) -> Result<Vec<SignalEvent>>;

    fn on_bar(
        &mut self,
        ctx: &mut StrategyContext,
        bar: &BarEvent,
    ) -> Result<Vec<SignalEvent>>;

    fn on_timer(
        &mut self,
        ctx: &mut StrategyContext,
        ts: DateTime<Utc>,
    ) -> Result<Vec<SignalEvent>>;
}
```

---

## 4. Strategy Context

StrategyContext 是策略唯一可访问的上下文。

```rust
pub struct StrategyContext {
    pub run_id: String,

    pub mode: RunMode,

    pub universe: Arc<dyn UniverseProvider>,

    pub feature_store: Arc<dyn FeatureStore>,

    pub indicators: Arc<IndicatorRegistry>,

    pub event_sink: Arc<dyn EventSink>,
}
```

StrategyContext 不暴露：

```text
Broker

OMS

SQLite Connection

Risk Engine

Portfolio Engine
```

---

## 5. Run Mode

同一个策略必须支持四种运行模式。

```rust
pub enum RunMode {
    Backtest,
    Replay,
    Paper,
    Live,
}
```

策略代码不应根据模式写不同逻辑。

允许：

```text
读取 mode 做日志标记

读取 mode 做指标统计
```

不允许：

```text
Backtest 一套逻辑

Live 一套逻辑
```

---

## 6. Signal Model

Strategy 的唯一业务输出是 `SignalEvent`。事件字段和事件持久化规则统一维护在 `events.md`，本文只约束策略如何产生信号。

---

## 7. Signal Side

`SignalSide` 的枚举定义见 `events.md`。策略只能表达方向和信心，不表达最终数量、订单类型、账户路由或券商参数。

---

## 8. Signal Strength

confidence 范围：

```text
0.0 ~ 1.0
```

含义：

```text
0.0 无效信号

0.5 普通信号

1.0 强信号
```

Portfolio 可以根据 confidence 调整目标仓位。

---

## 9. Strategy State

策略可以维护内部状态。

例如：

```rust
pub struct MovingAverageCrossStrategy {
    pub id: String,

    pub fast_window: usize,

    pub slow_window: usize,

    pub last_signal: Option<SignalSide>,
}
```

允许保存：

```text
最近指标值

最近信号

临时缓存

窗口数据
```

不允许保存：

```text
账户真实资金

Broker订单ID

数据库连接

未经过OMS的订单状态
```

---

## 10. Strategy Config

```yaml
strategies:

  ma_cross:
    enabled: true

    symbols:
      - AAPL
      - TSLA

    timeframe: 1m

    params:
      fast_window: 20
      slow_window: 60
```

---

## 11. Strategy Loading

支持静态注册。

```rust
pub trait StrategyFactory: Send + Sync {

    fn create(
        &self,
        config: StrategyConfig,
    ) -> Result<Box<dyn Strategy>>;
}
```

注册表：

```rust
pub struct StrategyRegistry {
    factories: HashMap<String, Box<dyn StrategyFactory>>,
}
```

---

## 12. Strategy Lifecycle

```text
Created

 ↓

Initialized

 ↓

Started

 ↓

Running

 ↓

Stopping

 ↓

Stopped
```

---

## 13. on_start

用于初始化策略。

允许：

```text
读取配置

初始化指标

加载特征定义

订阅行情
```

不允许：

```text
下单

访问Broker

写SQLite
```

---

## 14. on_tick

用于处理 Tick 行情。

```rust
fn on_tick(
    &mut self,
    ctx: &mut StrategyContext,
    tick: &TickEvent,
) -> Result<Vec<SignalEvent>>;
```

适合：

```text
高频策略

盘口策略

成交驱动策略
```

---

## 15. on_bar

用于处理 K线。

```rust
fn on_bar(
    &mut self,
    ctx: &mut StrategyContext,
    bar: &BarEvent,
) -> Result<Vec<SignalEvent>>;
```

适合：

```text
分钟策略

日线策略

趋势策略

均值回归策略
```

---

## 16. on_timer

用于定时任务。

```rust
fn on_timer(
    &mut self,
    ctx: &mut StrategyContext,
    ts: DateTime<Utc>,
) -> Result<Vec<SignalEvent>>;
```

适合：

```text
开盘检查

收盘清理

定时再平衡

风格切换
```

---

## 17. Universe Access

策略通过 Universe 读取可交易标的。

```rust
let symbols = ctx.universe.symbols();
```

Universe 负责：

```text
市场

资产类型

交易日历

停牌状态

可交易状态
```

---

## 18. Feature Store Access

策略通过 FeatureStore 读取特征。

```rust
let factor = ctx
    .feature_store
    .get("momentum_20d", symbol, ts)?;
```

FeatureStore 负责：

```text
因子

技术指标

历史特征

横截面数据
```

---

## 19. Indicator Access

策略通过 IndicatorRegistry 使用指标。

```rust
let ma = ctx
    .indicators
    .sma(symbol, "close", 20)?;
```

常用指标：

```text
SMA

EMA

RSI

MACD

ATR

VWAP

Bollinger Bands
```

---

## 20. Example Strategy

```rust
pub struct MovingAverageCrossStrategy {
    pub strategy_id: String,

    pub fast_window: usize,

    pub slow_window: usize,

    pub last_side: Option<SignalSide>,
}
```

```rust
impl Strategy for MovingAverageCrossStrategy {

    fn id(&self) -> &str {
        &self.strategy_id
    }

    fn name(&self) -> &str {
        "moving_average_cross"
    }

    fn on_start(
        &mut self,
        _ctx: &mut StrategyContext,
    ) -> Result<()> {
        Ok(())
    }

    fn on_stop(
        &mut self,
        _ctx: &mut StrategyContext,
    ) -> Result<()> {
        Ok(())
    }

    fn on_tick(
        &mut self,
        _ctx: &mut StrategyContext,
        _tick: &TickEvent,
    ) -> Result<Vec<SignalEvent>> {
        Ok(vec![])
    }

    fn on_bar(
        &mut self,
        ctx: &mut StrategyContext,
        bar: &BarEvent,
    ) -> Result<Vec<SignalEvent>> {

        let fast = ctx.indicators.sma(
            &bar.symbol,
            "close",
            self.fast_window,
        )?;

        let slow = ctx.indicators.sma(
            &bar.symbol,
            "close",
            self.slow_window,
        )?;

        let side = if fast > slow {
            Some(SignalSide::Buy)
        } else if fast < slow {
            Some(SignalSide::Sell)
        } else {
            None
        };

        if side.is_some() && side != self.last_side {
            self.last_side = side.clone();

            return Ok(vec![
                SignalEvent {
                    strategy_id: self.strategy_id.clone(),
                    symbol: bar.symbol.clone(),
                    side: side.unwrap(),
                    confidence: 0.8,
                    reason: Some("ma_cross".to_string()),
                    ts: bar.ts,
                }
            ]);
        }

        Ok(vec![])
    }

    fn on_timer(
        &mut self,
        _ctx: &mut StrategyContext,
        _ts: DateTime<Utc>,
    ) -> Result<Vec<SignalEvent>> {
        Ok(vec![])
    }
}
```

---

## 21. Multi Asset Strategy

策略可以同时支持：

```text
A股

港股

美股

数字货币
```

但不能直接处理市场交易规则。

市场规则由：

```text
MarketRule
```

处理。

例如：

```text
A股最小100股

港股每只股票不同Lot

美股支持碎股

币圈支持不同tick size
```

Strategy 只输出方向和信号强度。

---

## 22. Multi Timeframe Strategy

支持多周期数据。

```yaml
strategies:

  trend_following:
    enabled: true

    symbols:
      - BTCUSDT
      - ETHUSDT

    timeframes:
      - 1m
      - 15m
      - 1h
      - 1d
```

策略可读取不同周期特征：

```rust
let trend_1h = ctx
    .feature_store
    .get("trend_1h", symbol, ts)?;
```

---

## 23. Portfolio Separation

Strategy 不决定最终仓位。

Strategy 只表达：

```text
我要买

我要卖

我要平多

我要平空
```

Portfolio 决定：

```text
买多少

卖多少

目标仓位是多少

资金如何分配
```

---

## 24. Risk Separation

Strategy 不做最终风控。

Strategy 可以做轻量过滤：

```text
波动率过滤

时间过滤

信号过滤
```

但最终检查由 Risk 执行：

```text
最大仓位

最大亏损

最大回撤

杠杆限制

市场禁买

账户余额
```

---

## 25. Data Separation

Strategy 不直接读取 SQLite。

允许读取：

```text
FeatureStore

IndicatorRegistry

UniverseProvider

MarketDataSnapshot
```

禁止读取：

```text
SQLite Connection

Parquet File

Broker API

OMS State
```

---

## 26. Strategy Metrics

每个策略记录：

```text
signal_count

buy_signal_count

sell_signal_count

close_signal_count

last_signal_ts

signal_latency

error_count
```

---

## 27. Strategy Error Handling

策略错误不能导致系统崩溃。

```text
Strategy Error
   ↓
StrategyErrorEvent
   ↓
Metrics
   ↓
API / WebSocket
```

---

## 28. Strategy Hot Reload

支持配置级热更新。

允许热更新：

```text
enabled

params

symbols

timeframes
```

不允许热更新：

```text
strategy type

run mode

account binding
```

---

## 29. Testing

策略测试必须覆盖：

```text
单标的

多标的

无行情

缺失特征

重复信号

异常行情

Backtest

Replay

Paper

Live
```

---

## 30. Mandatory Rules

本节不再重复事件与链路定义；强制规则以本文第 2 节为准，事件字段以 `events.md` 为准。
