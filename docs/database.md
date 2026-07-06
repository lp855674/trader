# Trader Database Design

Version: v1.0  
Status: Draft  
Database: SQLite  
Historical Data Format: Parquet  
Target Markets: A股 / 港股 / 美股 / 数字货币  

---

# Current Implementation Status

The current implemented SQLite schema is split across migrations:

`migrations/0001_init.sql` contains the MVP trading-state schema:

- `strategy_runs`
- `instruments`
- `orders`
- `fills`
- `positions`
- `account_balances`
- `portfolio_snapshots`
- `event_store`

`migrations/0002_audit_projections.sql` adds query projections derived from `event_store`:

- `order_events`
- `risk_events`
- `insights`
- `portfolio_targets`

`migrations/0003_market_rules.sql` adds versionable market-reference and rule tables:

- `market_calendars`
- `trading_sessions`
- `fee_rules`
- `lot_size_rules`
- `price_limit_rules`

`migrations/0004_contract_accounting.sql` adds the storage boundary for contract positions and funding rates:

- `crypto_positions`
- `funding_rates`

`migrations/0005_reference_snapshots_and_ops.sql` adds the remaining target storage-boundary tables:

- `crypto_market_meta`
- `corporate_actions_meta`
- `cash_snapshots`
- `position_snapshots`
- `configs`
- `system_logs`

The current migrations cover the 24 target SQLite tables below, plus `event_store` as the immutable audit truth. A table existing in migration means the storage boundary exists; it does not mean every runtime path already writes it automatically.

Paper runtime writes `portfolio_snapshots`, `cash_snapshots`, `position_snapshots`, and simulated contract `crypto_positions` during local paper runs. Simulated funding settlement updates contract funding and realized PnL fields. API-launched Backtest, Paper, Replay, and Live runs plus CLI-launched Backtest, Paper, and Replay runs write a `RUN` config snapshot to `configs`; API runs also store the parsed config in `strategy_runs.config_json` and index run lifecycle messages in `system_logs`. Config snapshots and run-version bindings are exposed through API/CLI query routes. `crypto_positions`, `funding_rates`, `crypto_market_meta`, and `corporate_actions_meta` have read-only API query routes; contract positions and funding rates also have CLI readback commands. Binance market metadata/funding-rate and Yahoo corporate-actions ingestion can populate the reference-data tables through CLI/scheduled ingestion, live runtime can write baseline plus fake-broker reconciliation snapshots for local verification, and the live runtime snapshot path now supports injecting the IBKR paper adapter from API config for broker account/position snapshots. Remaining production-hardening tasks include human config approval/rollout policy, real Gateway long-run snapshot/reconciliation verification, external production log collectors/alert routing, and reference-data rate-limit/stale-data alerting.

---

# 1. 设计目标

Trader 的数据库设计分为两层：

```text
SQLite
  ↓
交易状态、订单、成交、持仓、账户、策略运行记录、风控事件、系统配置

Parquet
  ↓
历史行情、K线、Tick、OrderBook、基本面、复权数据、因子数据、资金费率、Open Interest
```

SQLite 不负责存储大规模历史行情。

Parquet 不负责存储交易状态。

---

# 2. 支持市场与资产类型

## 2.1 Market

```text
CN       A股
HK       港股
US       美股
CRYPTO   数字货币
```

---

## 2.2 AssetClass

```text
EQUITY          股票
CRYPTO_SPOT     数字货币现货
CRYPTO_PERP     数字货币永续合约
CRYPTO_FUTURE   数字货币交割合约
```

---

## 2.3 Currency

```text
CNY
HKD
USD
USDT
USDC
BTC
ETH
```

---

# 3. 存储分层原则

## 3.1 SQLite 存储内容

SQLite 用于存储系统状态和交易结果。

包括：

```text
strategy_runs
instruments
market_calendars
trading_sessions
fee_rules
lot_size_rules
price_limit_rules
crypto_market_meta
funding_rates
orders
order_events
fills
positions
crypto_positions
account_balances
cash_snapshots
portfolio_snapshots
position_snapshots
risk_events
insights
portfolio_targets
configs
system_logs
```

SQLite 适合：

```text
事务写入
状态恢复
订单查询
成交查询
持仓查询
账户查询
回测结果查询
小规模配置管理
```

SQLite 不适合：

```text
海量 Tick 行情
海量 OrderBook
海量分钟线
大规模因子矩阵
机器学习训练数据
```

---

## 3.2 Parquet 存储内容

Parquet 用于存储大规模历史数据。

包括：

```text
daily candles
minute candles
ticks
trades
orderbook
fundamentals
corporate_actions
index_members
features
funding_rate
open_interest
mark_price
index_price
liquidations
```

Parquet 适合：

```text
大规模扫描
列式压缩
回测
因子计算
机器学习训练
历史行情归档
```

Parquet 不适合：

```text
频繁小事务更新
订单状态管理
实盘状态恢复
```

---

# 4. 数据目录结构

```text
Trader/
├── data/
│   ├── trader.sqlite
│   └── parquet/
│       ├── cn/
│       │   ├── daily/
│       │   ├── 1m/
│       │   ├── tick/
│       │   ├── fundamentals/
│       │   ├── corporate_actions/
│       │   ├── index_members/
│       │   └── features/
│       ├── hk/
│       │   ├── daily/
│       │   ├── 1m/
│       │   ├── tick/
│       │   ├── fundamentals/
│       │   ├── corporate_actions/
│       │   ├── index_members/
│       │   └── features/
│       ├── us/
│       │   ├── daily/
│       │   ├── 1m/
│       │   ├── tick/
│       │   ├── fundamentals/
│       │   ├── corporate_actions/
│       │   ├── index_members/
│       │   └── features/
│       └── crypto/
│           ├── spot/
│           │   ├── 1m/
│           │   ├── tick/
│           │   ├── trade/
│           │   └── orderbook/
│           ├── perp/
│           │   ├── 1m/
│           │   ├── tick/
│           │   ├── trade/
│           │   ├── orderbook/
│           │   ├── funding_rate/
│           │   ├── open_interest/
│           │   ├── mark_price/
│           │   └── index_price/
│           ├── future/
│           │   ├── 1m/
│           │   ├── tick/
│           │   ├── trade/
│           │   ├── orderbook/
│           │   └── open_interest/
│           └── features/
```

---

# 5. 命名规范

## 5.1 表名

使用小写复数形式：

```text
strategy_runs
orders
fills
positions
risk_events
account_balances
```

---

## 5.2 时间字段

所有时间字段使用 Unix timestamp milliseconds。

```text
created_at INTEGER
updated_at INTEGER
ts INTEGER
started_at INTEGER
ended_at INTEGER
```

单位：

```text
milliseconds
```

---

## 5.3 金额、价格、数量字段

SQLite 中使用 `TEXT` 存储 Decimal 字符串，避免浮点误差。

示例：

```sql
price TEXT NOT NULL
qty TEXT NOT NULL
cash TEXT NOT NULL
```

Rust 中使用：

```rust
rust_decimal::Decimal
```

---

## 5.4 枚举字段

SQLite 中使用 `TEXT`。

示例：

```text
market = 'CN' | 'HK' | 'US' | 'CRYPTO'
side = 'BUY' | 'SELL'
status = 'NEW' | 'SUBMITTED' | 'FILLED'
asset_class = 'EQUITY' | 'CRYPTO_SPOT' | 'CRYPTO_PERP' | 'CRYPTO_FUTURE'
```

---

# 6. SQLite 初始化设置

```sql
PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA temp_store = MEMORY;
PRAGMA busy_timeout = 5000;
```

说明：

```text
foreign_keys = ON
  启用外键约束

journal_mode = WAL
  提升并发读写能力

synchronous = NORMAL
  在性能和安全之间折中

temp_store = MEMORY
  临时表使用内存

busy_timeout = 5000
  数据库锁等待 5 秒
```

---

# 7. ER 关系

```text
strategy_runs
    │
    ├── orders
    │       │
    │       ├── order_events
    │       └── fills
    │
    ├── positions
    ├── crypto_positions
    ├── account_balances
    ├── cash_snapshots
    ├── position_snapshots
    ├── portfolio_snapshots
    ├── risk_events
    ├── insights
    ├── portfolio_targets
    └── system_logs


instruments
    │
    ├── market_calendars
    ├── trading_sessions
    ├── fee_rules
    ├── lot_size_rules
    ├── price_limit_rules
    ├── crypto_market_meta
    ├── funding_rates
    └── corporate_actions_meta
```

---

# 8. SQLite 表设计

---

# 8.1 strategy_runs

记录每次策略运行。

包括：

```text
backtest
replay
paper
live
```

```sql
CREATE TABLE IF NOT EXISTS strategy_runs (
    id TEXT PRIMARY KEY,

    name TEXT NOT NULL,
    strategy_name TEXT NOT NULL,

    mode TEXT NOT NULL,
    market TEXT NOT NULL,

    status TEXT NOT NULL,

    started_at INTEGER NOT NULL,
    ended_at INTEGER,

    initial_cash TEXT NOT NULL,
    final_cash TEXT,
    final_equity TEXT,

    base_currency TEXT NOT NULL,

    config_json TEXT NOT NULL,
    params_json TEXT,

    git_commit TEXT,
    engine_version TEXT,

    note TEXT,

    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
```

字段说明：

| 字段             | 说明                                              |
| -------------- | ----------------------------------------------- |
| id             | 运行 ID                                           |
| name           | 本次运行名称                                          |
| strategy_name  | 策略名称                                            |
| mode           | BACKTEST / REPLAY / PAPER / LIVE                |
| market         | CN / HK / US / CRYPTO / MIXED                   |
| status         | CREATED / RUNNING / FINISHED / FAILED / STOPPED |
| initial_cash   | 初始资金                                            |
| final_cash     | 结束现金                                            |
| final_equity   | 结束权益                                            |
| base_currency  | 基准币种                                            |
| config_json    | 完整配置快照                                          |
| params_json    | 策略参数                                            |
| git_commit     | 代码版本                                            |
| engine_version | 引擎版本                                            |

```sql
CREATE INDEX IF NOT EXISTS idx_strategy_runs_mode
ON strategy_runs(mode);

CREATE INDEX IF NOT EXISTS idx_strategy_runs_status
ON strategy_runs(status);

CREATE INDEX IF NOT EXISTS idx_strategy_runs_started_at
ON strategy_runs(started_at);
```

---

# 8.2 instruments

记录证券、交易对、合约基础信息。

```sql
CREATE TABLE IF NOT EXISTS instruments (
    id INTEGER PRIMARY KEY AUTOINCREMENT,

    market TEXT NOT NULL,
    exchange TEXT NOT NULL,
    symbol TEXT NOT NULL,

    name TEXT,
    asset_class TEXT NOT NULL,

    currency TEXT,

    base_asset TEXT,
    quote_asset TEXT,
    settlement_asset TEXT,

    contract_type TEXT,
    contract_size TEXT,
    is_inverse INTEGER NOT NULL DEFAULT 0,

    lot_size TEXT,
    tick_size TEXT,
    multiplier TEXT,

    min_qty TEXT,
    max_qty TEXT,
    min_notional TEXT,

    price_precision INTEGER,
    qty_precision INTEGER,

    board TEXT,
    industry TEXT,
    sector TEXT,

    is_active INTEGER NOT NULL DEFAULT 1,
    is_tradable INTEGER NOT NULL DEFAULT 1,

    list_date INTEGER,
    delist_date INTEGER,

    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,

    UNIQUE(market, exchange, symbol)
);
```

示例：

```text
CN / SSE / 600519 / EQUITY
HK / HKEX / 00700 / EQUITY
US / NASDAQ / AAPL / EQUITY
CRYPTO / BINANCE / BTCUSDT / CRYPTO_SPOT
CRYPTO / OKX / BTC-USDT-SWAP / CRYPTO_PERP
```

```sql
CREATE INDEX IF NOT EXISTS idx_instruments_market
ON instruments(market);

CREATE INDEX IF NOT EXISTS idx_instruments_symbol
ON instruments(symbol);

CREATE INDEX IF NOT EXISTS idx_instruments_market_symbol
ON instruments(market, symbol);

CREATE INDEX IF NOT EXISTS idx_instruments_asset_class
ON instruments(asset_class);

CREATE INDEX IF NOT EXISTS idx_instruments_exchange_symbol
ON instruments(exchange, symbol);
```

---

# 8.3 market_calendars

记录股票市场交易日历。

数字货币通常 7x24，不依赖该表，但可用于记录交易所维护日。

```sql
CREATE TABLE IF NOT EXISTS market_calendars (
    id INTEGER PRIMARY KEY AUTOINCREMENT,

    market TEXT NOT NULL,
    exchange TEXT NOT NULL,

    trade_date TEXT NOT NULL,

    is_trading_day INTEGER NOT NULL,
    is_half_day INTEGER NOT NULL DEFAULT 0,

    note TEXT,

    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,

    UNIQUE(market, exchange, trade_date)
);
```

```sql
CREATE INDEX IF NOT EXISTS idx_market_calendars_market_date
ON market_calendars(market, trade_date);

CREATE INDEX IF NOT EXISTS idx_market_calendars_exchange_date
ON market_calendars(exchange, trade_date);
```

---

# 8.4 trading_sessions

记录市场交易时段。

```sql
CREATE TABLE IF NOT EXISTS trading_sessions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,

    market TEXT NOT NULL,
    exchange TEXT NOT NULL,

    session_name TEXT NOT NULL,

    start_time TEXT NOT NULL,
    end_time TEXT NOT NULL,

    timezone TEXT NOT NULL,

    is_auction INTEGER NOT NULL DEFAULT 0,
    is_regular INTEGER NOT NULL DEFAULT 1,
    is_extended INTEGER NOT NULL DEFAULT 0,

    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
```

示例：

A股：

```text
open_auction: 09:15 - 09:25
morning:      09:30 - 11:30
afternoon:    13:00 - 15:00
```

港股：

```text
morning:      09:30 - 12:00
afternoon:    13:00 - 16:00
```

美股：

```text
pre_market:   04:00 - 09:30
regular:      09:30 - 16:00
after_hours:  16:00 - 20:00
```

数字货币：

```text
regular:      00:00 - 23:59
```

```sql
CREATE INDEX IF NOT EXISTS idx_trading_sessions_market
ON trading_sessions(market, exchange);
```

---

# 8.5 fee_rules

记录 maker/taker 手续费、税费、交易所附加费和最低费用 floor。`symbol` 为空时表示
asset class 规则；`asset_class='*'` 且 `symbol` 为空时表示 exchange default。

```sql
CREATE TABLE IF NOT EXISTS fee_rules (
    id TEXT PRIMARY KEY,
    market TEXT NOT NULL,
    exchange TEXT NOT NULL,
    asset_class TEXT NOT NULL,
    symbol TEXT,
    volume_window TEXT NOT NULL DEFAULT 'run',
    maker_bps TEXT NOT NULL,
    taker_bps TEXT NOT NULL,
    minimum_fee TEXT,
    tax_bps TEXT,
    exchange_fee_bps TEXT,
    effective_from_ms INTEGER NOT NULL,
    effective_to_ms INTEGER
);
```

```sql
CREATE INDEX IF NOT EXISTS idx_fee_rules_lookup
ON fee_rules(market, exchange, asset_class, symbol, effective_from_ms);
```

运行时查询顺序：

1. `symbol` 精确匹配。
2. `symbol IS NULL AND asset_class = <requested>`。
3. `symbol IS NULL AND asset_class = '*'`。

阶梯费率归属到父 `fee_rules`。tier 只覆盖 maker/taker bps；`minimum_fee`、
`tax_bps`、`exchange_fee_bps` 仍来自父规则。

`volume_window` 控制阶梯费率的成交额窗口：

| 值 | 说明 |
| --- | --- |
| `run` | 默认值；不读取历史成交 seed，只按当前运行内成交额累计。 |
| `rolling_30d` | 启动时读取同账户、同规则作用域最近 30 天已持久化成交额作为 seed；运行中每次成交计费前剔除滑出 30 天窗口的历史和运行内成交。 |
| `calendar_month` | 启动时读取同账户、同规则作用域从 UTC 月初到启动时的已持久化成交额作为 seed；运行中每次成交计费前按 UTC 月初剔除上月成交，跨月后只累计当前 UTC 月成交。 |

```sql
CREATE TABLE IF NOT EXISTS fee_rule_tiers (
    id TEXT PRIMARY KEY,
    fee_rule_id TEXT NOT NULL,
    volume_from TEXT NOT NULL,
    volume_to TEXT,
    maker_bps TEXT NOT NULL,
    taker_bps TEXT NOT NULL,
    FOREIGN KEY(fee_rule_id) REFERENCES fee_rules(id)
);

CREATE INDEX IF NOT EXISTS idx_fee_rule_tiers_lookup
ON fee_rule_tiers(fee_rule_id, volume_from);
```

---

# 8.6 lot_size_rules

记录最小交易单位。

```sql
CREATE TABLE IF NOT EXISTS lot_size_rules (
    id INTEGER PRIMARY KEY AUTOINCREMENT,

    market TEXT NOT NULL,
    exchange TEXT NOT NULL,
    symbol TEXT,

    asset_class TEXT,

    lot_size TEXT NOT NULL,
    min_qty TEXT,
    max_qty TEXT,
    min_notional TEXT,

    allow_fractional INTEGER NOT NULL DEFAULT 0,

    effective_from INTEGER NOT NULL,
    effective_to INTEGER,

    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
```

规则说明：

```text
A股：lot_size = 100
港股：不同股票不同 lot_size
美股：可配置是否支持 fractional share
数字货币：按交易所 min_qty / min_notional / step_size 校验
```

```sql
CREATE INDEX IF NOT EXISTS idx_lot_size_rules_market_symbol
ON lot_size_rules(market, symbol);

CREATE INDEX IF NOT EXISTS idx_lot_size_rules_asset_class
ON lot_size_rules(asset_class);
```

---

# 8.7 price_limit_rules

记录涨跌停、价格限制、LULD 或交易所价格限制。

```sql
CREATE TABLE IF NOT EXISTS price_limit_rules (
    id INTEGER PRIMARY KEY AUTOINCREMENT,

    market TEXT NOT NULL,
    exchange TEXT NOT NULL,
    symbol TEXT,

    asset_class TEXT,
    board TEXT,

    limit_type TEXT NOT NULL,

    up_limit_rate TEXT,
    down_limit_rate TEXT,

    effective_from INTEGER NOT NULL,
    effective_to INTEGER,

    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
```

limit_type 示例：

```text
PERCENT_LIMIT
LULD
EXCHANGE_PRICE_BAND
NONE
```

```sql
CREATE INDEX IF NOT EXISTS idx_price_limit_rules_market_symbol
ON price_limit_rules(market, symbol);

CREATE INDEX IF NOT EXISTS idx_price_limit_rules_board
ON price_limit_rules(market, board);

CREATE INDEX IF NOT EXISTS idx_price_limit_rules_asset_class
ON price_limit_rules(asset_class);
```

---

# 8.8 crypto_market_meta

记录数字货币交易对、永续合约、交割合约的交易规则。

```sql
CREATE TABLE IF NOT EXISTS crypto_market_meta (
    id INTEGER PRIMARY KEY AUTOINCREMENT,

    exchange TEXT NOT NULL,
    symbol TEXT NOT NULL,

    base_asset TEXT NOT NULL,
    quote_asset TEXT NOT NULL,

    instrument_type TEXT NOT NULL,

    contract_type TEXT,
    contract_size TEXT,
    settlement_asset TEXT,

    min_notional TEXT,
    min_qty TEXT,
    max_qty TEXT,

    price_precision INTEGER,
    qty_precision INTEGER,

    price_tick TEXT,
    qty_step TEXT,

    maker_fee_rate TEXT,
    taker_fee_rate TEXT,

    funding_interval_hours INTEGER,

    max_leverage TEXT,

    margin_modes TEXT,

    is_inverse INTEGER NOT NULL DEFAULT 0,
    is_active INTEGER NOT NULL DEFAULT 1,

    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,

    UNIQUE(exchange, symbol)
);
```

instrument_type 示例：

```text
SPOT
PERP
FUTURE
```

contract_type 示例：

```text
LINEAR
INVERSE
DELIVERY
```

margin_modes 示例：

```json
["CROSS","ISOLATED"]
```

```sql
CREATE INDEX IF NOT EXISTS idx_crypto_market_meta_exchange
ON crypto_market_meta(exchange);

CREATE INDEX IF NOT EXISTS idx_crypto_market_meta_symbol
ON crypto_market_meta(symbol);

CREATE INDEX IF NOT EXISTS idx_crypto_market_meta_type
ON crypto_market_meta(instrument_type);
```

---

# 8.9 funding_rates

记录数字货币永续合约资金费率。

```sql
CREATE TABLE IF NOT EXISTS funding_rates (
    id TEXT PRIMARY KEY,
    exchange TEXT NOT NULL,
    symbol TEXT NOT NULL,
    funding_time_ms INTEGER NOT NULL,
    funding_rate TEXT NOT NULL,
    mark_price TEXT,
    source TEXT NOT NULL,

    UNIQUE(exchange, symbol, funding_time_ms)
);
```

```sql
CREATE INDEX IF NOT EXISTS idx_funding_rates_symbol_time
ON funding_rates(exchange, symbol, funding_time_ms);
```

---

# 8.10 corporate_actions_meta

记录复权、分红、拆股等元数据索引。

详细数据可以存 Parquet，这里只保留索引和摘要。

```sql
CREATE TABLE IF NOT EXISTS corporate_actions_meta (
    id INTEGER PRIMARY KEY AUTOINCREMENT,

    market TEXT NOT NULL,
    exchange TEXT NOT NULL,
    symbol TEXT NOT NULL,

    action_type TEXT NOT NULL,

    ex_date INTEGER NOT NULL,
    record_date INTEGER,
    payable_date INTEGER,

    ratio TEXT,
    cash_amount TEXT,
    currency TEXT,

    source TEXT,

    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
```

action_type 示例：

```text
DIVIDEND
SPLIT
BONUS
RIGHTS
MERGER
SPINOFF
```

```sql
CREATE INDEX IF NOT EXISTS idx_corporate_actions_symbol_date
ON corporate_actions_meta(market, symbol, ex_date);
```

---

# 8.11 orders

订单主表。

股票、数字货币现货、永续合约、交割合约订单统一进入 orders。

```sql
CREATE TABLE IF NOT EXISTS orders (
    id TEXT PRIMARY KEY,

    run_id TEXT NOT NULL,

    client_order_id TEXT NOT NULL UNIQUE,
    broker_order_id TEXT,

    market TEXT NOT NULL,
    exchange TEXT NOT NULL,
    symbol TEXT NOT NULL,
    asset_class TEXT NOT NULL,

    side TEXT NOT NULL,
    order_type TEXT NOT NULL,
    time_in_force TEXT,

    price TEXT,
    stop_price TEXT,

    qty TEXT NOT NULL,
    filled_qty TEXT NOT NULL DEFAULT '0',
    remaining_qty TEXT NOT NULL DEFAULT '0',

    avg_fill_price TEXT,

    status TEXT NOT NULL,

    strategy_name TEXT,
    source TEXT,

    reduce_only INTEGER NOT NULL DEFAULT 0,
    post_only INTEGER NOT NULL DEFAULT 0,

    leverage TEXT,
    margin_mode TEXT,
    position_side TEXT,

    submitted_at INTEGER,
    accepted_at INTEGER,
    completed_at INTEGER,

    error_code TEXT,
    error_message TEXT,

    raw_json TEXT,

    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,

    FOREIGN KEY(run_id) REFERENCES strategy_runs(id)
);
```

side：

```text
BUY
SELL
```

order_type：

```text
MARKET
LIMIT
STOP
STOP_LIMIT
POST_ONLY
```

time_in_force：

```text
DAY
GTC
IOC
FOK
```

status：

```text
NEW
PENDING_SUBMIT
SUBMITTED
PARTIALLY_FILLED
FILLED
PENDING_CANCEL
CANCELLED
REJECTED
EXPIRED
UNKNOWN
SYNCING
```

margin_mode：

```text
CROSS
ISOLATED
```

position_side：

```text
LONG
SHORT
NET
```

```sql
CREATE INDEX IF NOT EXISTS idx_orders_run_id
ON orders(run_id);

CREATE INDEX IF NOT EXISTS idx_orders_symbol
ON orders(market, symbol);

CREATE INDEX IF NOT EXISTS idx_orders_asset_class
ON orders(asset_class);

CREATE INDEX IF NOT EXISTS idx_orders_status
ON orders(status);

CREATE INDEX IF NOT EXISTS idx_orders_broker_order_id
ON orders(broker_order_id);

CREATE INDEX IF NOT EXISTS idx_orders_created_at
ON orders(created_at);
```

---

# 8.12 order_events

订单事件结构化审计投影。

真实不可变事件仍写入 `event_store`；`order_events` 是从 `broker.order.*` 与 `algorithm.oms.*` 事件派生出来的只读查询面，用于按 run、订单标识、状态和时间范围直接排查下单/恢复路径。

```sql
CREATE TABLE IF NOT EXISTS order_events (
    id TEXT PRIMARY KEY,
    event_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    order_id TEXT,
    client_order_id TEXT,
    broker_order_id TEXT,
    account_id TEXT,
    symbol TEXT,
    status TEXT NOT NULL,
    event_type TEXT NOT NULL,
    message TEXT,
    ts_ms INTEGER NOT NULL,
    payload_json TEXT NOT NULL,
    FOREIGN KEY(event_id) REFERENCES event_store(event_id)
);
```

event_type 示例：

```text
broker.order.submitted
broker.order.partially_filled
broker.order.filled
broker.order.failed
broker.order.recovered
algorithm.oms.cancel_requested
```

```sql
CREATE INDEX IF NOT EXISTS idx_order_events_run_id
ON order_events(run_id);

CREATE INDEX IF NOT EXISTS idx_order_events_order_id
ON order_events(order_id);

CREATE INDEX IF NOT EXISTS idx_order_events_ts
ON order_events(ts_ms);
```

---

# 8.13 fills

成交表。

```sql
CREATE TABLE IF NOT EXISTS fills (
    id TEXT PRIMARY KEY,

    run_id TEXT NOT NULL,
    order_id TEXT NOT NULL,

    broker_fill_id TEXT,

    market TEXT NOT NULL,
    exchange TEXT NOT NULL,
    symbol TEXT NOT NULL,
    asset_class TEXT NOT NULL,

    side TEXT NOT NULL,

    price TEXT NOT NULL,
    qty TEXT NOT NULL,

    gross_amount TEXT NOT NULL,

    fee TEXT NOT NULL DEFAULT '0',
    tax TEXT NOT NULL DEFAULT '0',
    funding_fee TEXT NOT NULL DEFAULT '0',

    realized_pnl TEXT,

    net_amount TEXT NOT NULL,

    currency TEXT NOT NULL,

    liquidity TEXT,
    is_maker INTEGER,

    ts INTEGER NOT NULL,
    created_at INTEGER NOT NULL,

    raw_json TEXT,

    FOREIGN KEY(run_id) REFERENCES strategy_runs(id),
    FOREIGN KEY(order_id) REFERENCES orders(id)
);
```

liquidity：

```text
MAKER
TAKER
UNKNOWN
```

```sql
CREATE INDEX IF NOT EXISTS idx_fills_run_id
ON fills(run_id);

CREATE INDEX IF NOT EXISTS idx_fills_order_id
ON fills(order_id);

CREATE INDEX IF NOT EXISTS idx_fills_symbol_ts
ON fills(market, symbol, ts);

CREATE INDEX IF NOT EXISTS idx_fills_asset_class
ON fills(asset_class);
```

---

# 8.14 positions

普通持仓表。

用于：

```text
A股
港股
美股
数字货币现货
```

```sql
CREATE TABLE IF NOT EXISTS positions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,

    run_id TEXT NOT NULL,

    market TEXT NOT NULL,
    exchange TEXT NOT NULL,
    symbol TEXT NOT NULL,
    asset_class TEXT NOT NULL,

    qty TEXT NOT NULL,
    available_qty TEXT NOT NULL,

    avg_price TEXT NOT NULL,
    market_price TEXT,

    market_value TEXT,
    cost_basis TEXT,

    unrealized_pnl TEXT,
    realized_pnl TEXT,

    currency TEXT NOT NULL,

    updated_ts INTEGER NOT NULL,

    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,

    UNIQUE(run_id, market, exchange, symbol),

    FOREIGN KEY(run_id) REFERENCES strategy_runs(id)
);
```

说明：

```text
qty:
  总持仓

available_qty:
  可卖数量

A股买入当天 qty 增加，但 available_qty 不增加。
数字货币现货一般 qty 与 available_qty 会受冻结订单影响。
```

```sql
CREATE INDEX IF NOT EXISTS idx_positions_run_id
ON positions(run_id);

CREATE INDEX IF NOT EXISTS idx_positions_symbol
ON positions(market, symbol);

CREATE INDEX IF NOT EXISTS idx_positions_asset_class
ON positions(asset_class);
```

---

# 8.15 crypto_positions

数字货币衍生品持仓表。

用于：

```text
CRYPTO_PERP
CRYPTO_FUTURE
```

```sql
CREATE TABLE IF NOT EXISTS crypto_positions (
    run_id TEXT NOT NULL,
    account_id TEXT NOT NULL,
    exchange TEXT NOT NULL,
    symbol TEXT NOT NULL,
    asset_class TEXT NOT NULL,
    margin_mode TEXT NOT NULL,
    position_side TEXT NOT NULL,
    leverage TEXT NOT NULL,
    qty TEXT NOT NULL,
    avg_price TEXT NOT NULL,
    margin_used TEXT NOT NULL,
    funding_fee TEXT NOT NULL DEFAULT '0',
    realized_pnl TEXT NOT NULL DEFAULT '0',
    unrealized_pnl TEXT NOT NULL DEFAULT '0',
    updated_at_ms INTEGER NOT NULL,

    PRIMARY KEY (run_id, account_id, exchange, symbol, position_side)
);
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

```sql
CREATE INDEX IF NOT EXISTS idx_crypto_positions_run_id
ON crypto_positions(run_id);

CREATE INDEX IF NOT EXISTS idx_crypto_positions_symbol
ON crypto_positions(exchange, symbol);

CREATE INDEX IF NOT EXISTS idx_crypto_positions_side
ON crypto_positions(position_side);
```

---

# 8.16 account_balances

账户余额表。

支持股票现金账户和数字货币多币种账户。

```sql
CREATE TABLE IF NOT EXISTS account_balances (
    id INTEGER PRIMARY KEY AUTOINCREMENT,

    run_id TEXT NOT NULL,

    market TEXT NOT NULL,
    exchange TEXT,

    asset TEXT NOT NULL,

    total TEXT NOT NULL,
    available TEXT NOT NULL,
    frozen TEXT NOT NULL DEFAULT '0',

    borrowed TEXT NOT NULL DEFAULT '0',
    interest TEXT NOT NULL DEFAULT '0',

    updated_ts INTEGER NOT NULL,

    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,

    UNIQUE(run_id, market, exchange, asset),

    FOREIGN KEY(run_id) REFERENCES strategy_runs(id)
);
```

asset 示例：

```text
CNY
HKD
USD
USDT
USDC
BTC
ETH
```

```sql
CREATE INDEX IF NOT EXISTS idx_account_balances_run_id
ON account_balances(run_id);

CREATE INDEX IF NOT EXISTS idx_account_balances_asset
ON account_balances(asset);
```

---

# 8.17 cash_snapshots

现金快照表。

主要用于股票账户，也可记录基准币种现金变化。

```sql
CREATE TABLE IF NOT EXISTS cash_snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,

    run_id TEXT NOT NULL,

    ts INTEGER NOT NULL,

    currency TEXT NOT NULL,

    cash TEXT NOT NULL,
    available_cash TEXT NOT NULL,
    frozen_cash TEXT NOT NULL DEFAULT '0',

    created_at INTEGER NOT NULL,

    FOREIGN KEY(run_id) REFERENCES strategy_runs(id)
);
```

```sql
CREATE INDEX IF NOT EXISTS idx_cash_snapshots_run_ts
ON cash_snapshots(run_id, ts);
```

---

# 8.18 position_snapshots

持仓快照表。

用于回测分析、Replay、PnL 曲线生成。

```sql
CREATE TABLE IF NOT EXISTS position_snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,

    run_id TEXT NOT NULL,

    ts INTEGER NOT NULL,

    market TEXT NOT NULL,
    exchange TEXT NOT NULL,
    symbol TEXT NOT NULL,
    asset_class TEXT NOT NULL,

    position_side TEXT,

    qty TEXT NOT NULL,
    available_qty TEXT NOT NULL,

    avg_price TEXT,
    entry_price TEXT,

    market_price TEXT,
    mark_price TEXT,

    market_value TEXT,

    unrealized_pnl TEXT,
    realized_pnl TEXT,

    currency TEXT NOT NULL,

    created_at INTEGER NOT NULL,

    FOREIGN KEY(run_id) REFERENCES strategy_runs(id)
);
```

```sql
CREATE INDEX IF NOT EXISTS idx_position_snapshots_run_ts
ON position_snapshots(run_id, ts);

CREATE INDEX IF NOT EXISTS idx_position_snapshots_symbol_ts
ON position_snapshots(market, symbol, ts);

CREATE INDEX IF NOT EXISTS idx_position_snapshots_asset_class
ON position_snapshots(asset_class);
```

---

# 8.19 portfolio_snapshots

组合权益快照表。

```sql
CREATE TABLE IF NOT EXISTS portfolio_snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,

    run_id TEXT NOT NULL,

    ts INTEGER NOT NULL,

    base_currency TEXT NOT NULL,

    cash TEXT NOT NULL,
    market_value TEXT NOT NULL,
    equity TEXT NOT NULL,

    margin_used TEXT NOT NULL DEFAULT '0',
    margin_available TEXT,

    realized_pnl TEXT NOT NULL DEFAULT '0',
    unrealized_pnl TEXT NOT NULL DEFAULT '0',

    total_fee TEXT NOT NULL DEFAULT '0',
    total_tax TEXT NOT NULL DEFAULT '0',
    total_funding_fee TEXT NOT NULL DEFAULT '0',

    drawdown TEXT,
    drawdown_pct TEXT,

    daily_return TEXT,
    cumulative_return TEXT,

    created_at INTEGER NOT NULL,

    FOREIGN KEY(run_id) REFERENCES strategy_runs(id)
);
```

```sql
CREATE INDEX IF NOT EXISTS idx_portfolio_snapshots_run_ts
ON portfolio_snapshots(run_id, ts);
```

---

# 8.20 risk_events

风控结构化审计投影。

真实不可变事件仍写入 `event_store`；`risk_events` 当前从 `algorithm.risk.*` 事件派生，用于查询 pre-trade 风控决策和 live reconciliation drift 这类审计记录。

```sql
CREATE TABLE IF NOT EXISTS risk_events (
    id TEXT PRIMARY KEY,
    event_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    account_id TEXT,
    symbol TEXT,
    risk_type TEXT NOT NULL,
    decision TEXT NOT NULL,
    reason TEXT,
    threshold TEXT,
    observed_value TEXT,
    ts_ms INTEGER NOT NULL,
    payload_json TEXT NOT NULL,
    FOREIGN KEY(event_id) REFERENCES event_store(event_id)
);
```

risk_type 示例：

```text
MAX_POSITION
MAX_DRAWDOWN
DAILY_LOSS
PRICE_LIMIT
T1_VIOLATION
LOT_SIZE
TRADING_TIME
INSUFFICIENT_CASH
ORDER_RATE_LIMIT
FUNDING_RATE
LIQUIDATION_RISK
LEVERAGE_LIMIT
MARGIN_RATIO
EXCHANGE_RATE_LIMIT
MIN_NOTIONAL
POST_ONLY_REJECT
REDUCE_ONLY_VIOLATION
reconciliation_drift
```

decision 示例：

```text
approved
warn
rejected
```

```sql
CREATE INDEX IF NOT EXISTS idx_risk_events_run_id
ON risk_events(run_id);

CREATE INDEX IF NOT EXISTS idx_risk_events_symbol
ON risk_events(symbol);

CREATE INDEX IF NOT EXISTS idx_risk_events_ts
ON risk_events(ts_ms);
```

---

# 8.21 insights

Alpha 信号表。

```sql
CREATE TABLE IF NOT EXISTS insights (
    id TEXT PRIMARY KEY,

    run_id TEXT NOT NULL,

    ts INTEGER NOT NULL,

    market TEXT NOT NULL,
    exchange TEXT NOT NULL,
    symbol TEXT NOT NULL,
    asset_class TEXT NOT NULL,

    direction TEXT NOT NULL,

    magnitude TEXT,
    confidence TEXT,
    weight TEXT,

    horizon_ms INTEGER,

    source_model TEXT NOT NULL,

    raw_json TEXT,

    created_at INTEGER NOT NULL,

    FOREIGN KEY(run_id) REFERENCES strategy_runs(id)
);
```

direction：

```text
LONG
SHORT
FLAT
```

```sql
CREATE INDEX IF NOT EXISTS idx_insights_run_ts
ON insights(run_id, ts);

CREATE INDEX IF NOT EXISTS idx_insights_symbol_ts
ON insights(market, symbol, ts);

CREATE INDEX IF NOT EXISTS idx_insights_asset_class
ON insights(asset_class);
```

---

# 8.22 portfolio_targets

目标仓位表。

```sql
CREATE TABLE IF NOT EXISTS portfolio_targets (
    id TEXT PRIMARY KEY,

    run_id TEXT NOT NULL,

    ts INTEGER NOT NULL,

    market TEXT NOT NULL,
    exchange TEXT NOT NULL,
    symbol TEXT NOT NULL,
    asset_class TEXT NOT NULL,

    target_qty TEXT,
    target_percent TEXT,
    target_value TEXT,

    position_side TEXT,

    source_model TEXT NOT NULL,

    raw_json TEXT,

    created_at INTEGER NOT NULL,

    FOREIGN KEY(run_id) REFERENCES strategy_runs(id)
);
```

```sql
CREATE INDEX IF NOT EXISTS idx_portfolio_targets_run_ts
ON portfolio_targets(run_id, ts);

CREATE INDEX IF NOT EXISTS idx_portfolio_targets_symbol_ts
ON portfolio_targets(market, symbol, ts);

CREATE INDEX IF NOT EXISTS idx_portfolio_targets_asset_class
ON portfolio_targets(asset_class);
```

---

# 8.23 configs

配置快照和管理型配置版本表。原始 migration 提供配置快照列；运行时 migration 会为管理型配置补齐 nullable lifecycle columns，保持旧 `RUN` 快照兼容。

```sql
CREATE TABLE IF NOT EXISTS configs (
    id TEXT PRIMARY KEY,

    name TEXT NOT NULL,

    config_type TEXT NOT NULL,

    content TEXT NOT NULL,

    format TEXT NOT NULL,

    checksum TEXT,

    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,

    lifecycle_version INTEGER,
    state TEXT,
    parent_version INTEGER,
    created_by TEXT,
    state_changed_at INTEGER,
    state_changed_by TEXT,
    state_change_reason TEXT,
    target_env TEXT,
    rollout TEXT,
    approved_by TEXT,
    approved_at INTEGER,
    published_by TEXT,
    published_at INTEGER
);
```

config_type：

```text
SYSTEM
STRATEGY
BROKER
DATA
RISK
MARKET
CRYPTO_EXCHANGE
```

format：

```text
TOML
JSON
YAML
```

lifecycle columns：

```text
lifecycle_version  管理型配置版本号；RUN 快照为空
state              draft / pending_review / approved / published / archived
parent_version     rollback 或派生版本来源
created_by         创建人
state_changed_at   最近状态变更时间
state_changed_by   最近状态变更人
state_change_reason 最近状态变更原因
target_env          发布目标环境；production 会启用独立审批约束
rollout             发布策略标签，例如 canary/full
approved_by         最近审批人
approved_at         最近审批时间
published_by        最近发布人
published_at        最近发布时间
```

```sql
CREATE INDEX IF NOT EXISTS idx_configs_name
ON configs(name);

CREATE INDEX IF NOT EXISTS idx_configs_type
ON configs(config_type);

CREATE UNIQUE INDEX IF NOT EXISTS idx_configs_name_lifecycle_version
ON configs(name, lifecycle_version)
WHERE lifecycle_version IS NOT NULL;
```

---

# 8.24 system_logs

系统日志索引表。

详细日志可以写文件，这里只保存关键事件索引。

```sql
CREATE TABLE IF NOT EXISTS system_logs (
    id TEXT PRIMARY KEY,

    run_id TEXT,

    ts INTEGER NOT NULL,

    level TEXT NOT NULL,

    target TEXT NOT NULL,

    message TEXT NOT NULL,

    fields_json TEXT,

    created_at INTEGER NOT NULL
);
```

level：

```text
TRACE
DEBUG
INFO
WARN
ERROR
```

```sql
CREATE INDEX IF NOT EXISTS idx_system_logs_run_id
ON system_logs(run_id);

CREATE INDEX IF NOT EXISTS idx_system_logs_ts
ON system_logs(ts);

CREATE INDEX IF NOT EXISTS idx_system_logs_level
ON system_logs(level);
```

---

# 9. 建表顺序

```text
1. strategy_runs
2. instruments
3. market_calendars
4. trading_sessions
5. fee_rules
6. lot_size_rules
7. price_limit_rules
8. crypto_market_meta
9. funding_rates
10. corporate_actions_meta
11. orders
12. order_events
13. fills
14. positions
15. crypto_positions
16. account_balances
17. cash_snapshots
18. position_snapshots
19. portfolio_snapshots
20. risk_events
21. insights
22. portfolio_targets
23. configs
24. system_logs
```

---

# 10. Parquet Schema

---

# 10.1 Stock Daily Candles

路径：

```text
data/parquet/{market}/daily/{symbol}/{year}.parquet
```

字段：

| 字段         | 类型      | 说明           |
| ---------- | ------- | ------------ |
| ts         | Int64   | 时间戳          |
| trade_date | Utf8    | 交易日期         |
| market     | Utf8    | CN / HK / US |
| exchange   | Utf8    | 交易所          |
| symbol     | Utf8    | 代码           |
| open       | Float64 | 开盘价          |
| high       | Float64 | 最高价          |
| low        | Float64 | 最低价          |
| close      | Float64 | 收盘价          |
| volume     | Float64 | 成交量          |
| amount     | Float64 | 成交额          |
| adj_factor | Float64 | 复权因子         |

---

# 10.2 Stock Minute Candles

路径：

```text
data/parquet/{market}/1m/{symbol}/{year}/{month}.parquet
```

字段：

| 字段        | 类型      | 说明  |
| --------- | ------- | --- |
| ts        | Int64   | 时间戳 |
| market    | Utf8    | 市场  |
| exchange  | Utf8    | 交易所 |
| symbol    | Utf8    | 代码  |
| timeframe | Utf8    | 1m  |
| open      | Float64 | 开盘价 |
| high      | Float64 | 最高价 |
| low       | Float64 | 最低价 |
| close     | Float64 | 收盘价 |
| volume    | Float64 | 成交量 |
| amount    | Float64 | 成交额 |

---

# 10.3 Stock Tick

路径：

```text
data/parquet/{market}/tick/{symbol}/{year}/{month}/{day}.parquet
```

字段：

| 字段          | 类型      | 说明  |
| ----------- | ------- | --- |
| ts          | Int64   | 时间戳 |
| market      | Utf8    | 市场  |
| exchange    | Utf8    | 交易所 |
| symbol      | Utf8    | 代码  |
| last_price  | Float64 | 最新价 |
| bid_price_1 | Float64 | 买一价 |
| bid_size_1  | Float64 | 买一量 |
| ask_price_1 | Float64 | 卖一价 |
| ask_size_1  | Float64 | 卖一量 |
| volume      | Float64 | 成交量 |
| amount      | Float64 | 成交额 |

---

# 10.4 Crypto Candles

路径：

```text
data/parquet/crypto/{instrument_type}/1m/{exchange}/{symbol}/{year}/{month}.parquet
```

字段：

| 字段                     | 类型      | 说明                   |
| ---------------------- | ------- | -------------------- |
| ts                     | Int64   | 时间戳                  |
| exchange               | Utf8    | 交易所                  |
| symbol                 | Utf8    | 交易对                  |
| instrument_type        | Utf8    | SPOT / PERP / FUTURE |
| open                   | Float64 | 开盘价                  |
| high                   | Float64 | 最高价                  |
| low                    | Float64 | 最低价                  |
| close                  | Float64 | 收盘价                  |
| volume                 | Float64 | 成交量                  |
| quote_volume           | Float64 | 计价币成交额               |
| trade_count            | Int64   | 成交笔数                 |
| taker_buy_volume       | Float64 | 主动买入成交量              |
| taker_buy_quote_volume | Float64 | 主动买入计价币成交额           |

---

# 10.5 Crypto Tick

路径：

```text
data/parquet/crypto/{instrument_type}/tick/{exchange}/{symbol}/{year}/{month}/{day}.parquet
```

字段：

| 字段               | 类型      | 说明                   |
| ---------------- | ------- | -------------------- |
| ts               | Int64   | 时间戳                  |
| exchange         | Utf8    | 交易所                  |
| symbol           | Utf8    | 交易对                  |
| instrument_type  | Utf8    | SPOT / PERP / FUTURE |
| last_price       | Float64 | 最新价                  |
| bid_price_1      | Float64 | 买一价                  |
| bid_size_1       | Float64 | 买一量                  |
| ask_price_1      | Float64 | 卖一价                  |
| ask_size_1       | Float64 | 卖一量                  |
| volume_24h       | Float64 | 24h 成交量              |
| quote_volume_24h | Float64 | 24h 成交额              |

---

# 10.6 Crypto Trades

路径：

```text
data/parquet/crypto/{instrument_type}/trade/{exchange}/{symbol}/{year}/{month}/{day}.parquet
```

字段：

| 字段             | 类型      | 说明          |
| -------------- | ------- | ----------- |
| ts             | Int64   | 时间戳         |
| exchange       | Utf8    | 交易所         |
| symbol         | Utf8    | 交易对         |
| trade_id       | Utf8    | 成交 ID       |
| price          | Float64 | 成交价         |
| qty            | Float64 | 成交数量        |
| quote_qty      | Float64 | 成交金额        |
| side           | Utf8    | BUY / SELL  |
| is_buyer_maker | Boolean | 是否买方为 maker |

---

# 10.7 Crypto OrderBook

路径：

```text
data/parquet/crypto/{instrument_type}/orderbook/{exchange}/{symbol}/{year}/{month}/{day}.parquet
```

字段：

| 字段          | 类型      | 说明                   |
| ----------- | ------- | -------------------- |
| ts          | Int64   | 时间戳                  |
| exchange    | Utf8    | 交易所                  |
| symbol      | Utf8    | 交易对                  |
| depth       | Int32   | 档位深度                 |
| bid_price_1 | Float64 | 买一价                  |
| bid_size_1  | Float64 | 买一量                  |
| ask_price_1 | Float64 | 卖一价                  |
| ask_size_1  | Float64 | 卖一量                  |
| ...         | ...     | 可扩展到 20 / 50 / 100 档 |

---

# 10.8 Funding Rate

路径：

```text
data/parquet/crypto/perp/funding_rate/{exchange}/{symbol}/{year}.parquet
```

字段：

| 字段           | 类型      | 说明     |
| ------------ | ------- | ------ |
| funding_time | Int64   | 资金费率时间 |
| exchange     | Utf8    | 交易所    |
| symbol       | Utf8    | 合约     |
| funding_rate | Float64 | 资金费率   |
| mark_price   | Float64 | 标记价格   |
| index_price  | Float64 | 指数价格   |

---

# 10.9 Open Interest

路径：

```text
data/parquet/crypto/perp/open_interest/{exchange}/{symbol}/{year}/{month}.parquet
```

字段：

| 字段                  | 类型      | 说明   |
| ------------------- | ------- | ---- |
| ts                  | Int64   | 时间戳  |
| exchange            | Utf8    | 交易所  |
| symbol              | Utf8    | 合约   |
| open_interest       | Float64 | 持仓量  |
| open_interest_value | Float64 | 持仓价值 |

---

# 10.10 Fundamentals

路径：

```text
data/parquet/{market}/fundamentals/{symbol}/{year}.parquet
```

字段：

| 字段                   | 类型      | 说明    |
| -------------------- | ------- | ----- |
| report_date          | Int64   | 报告期   |
| publish_date         | Int64   | 发布日期  |
| market               | Utf8    | 市场    |
| symbol               | Utf8    | 代码    |
| revenue              | Float64 | 营收    |
| net_income           | Float64 | 净利润   |
| total_assets         | Float64 | 总资产   |
| total_liabilities    | Float64 | 总负债   |
| roe                  | Float64 | ROE   |
| eps                  | Float64 | EPS   |
| book_value_per_share | Float64 | 每股净资产 |

---

# 10.11 Corporate Actions

路径：

```text
data/parquet/{market}/corporate_actions/{symbol}.parquet
```

字段：

| 字段          | 类型      | 说明    |
| ----------- | ------- | ----- |
| ex_date     | Int64   | 除权除息日 |
| market      | Utf8    | 市场    |
| symbol      | Utf8    | 代码    |
| action_type | Utf8    | 类型    |
| ratio       | Float64 | 拆股比例  |
| cash_amount | Float64 | 现金分红  |
| currency    | Utf8    | 币种    |

---

# 10.12 Features

路径：

```text
data/parquet/{market}/features/{feature_name}/{year}.parquet
```

字段：

| 字段           | 类型      | 说明   |
| ------------ | ------- | ---- |
| ts           | Int64   | 时间戳  |
| market       | Utf8    | 市场   |
| symbol       | Utf8    | 代码   |
| feature_name | Utf8    | 因子名称 |
| value        | Float64 | 因子值  |
| version      | Utf8    | 因子版本 |

数字货币因子路径：

```text
data/parquet/crypto/features/{feature_name}/{year}.parquet
```

---

# 11. Repository 设计

业务模块不能直接写 SQL。

所有 SQLite 操作必须通过 Repository。

---

## 11.1 StrategyRunRepository

```rust
pub trait StrategyRunRepository {
    async fn create_run(&self, run: &StrategyRun) -> Result<()>;

    async fn update_status(
        &self,
        run_id: &RunId,
        status: RunStatus,
    ) -> Result<()>;

    async fn finish_run(
        &self,
        run_id: &RunId,
        result: &RunResult,
    ) -> Result<()>;

    async fn find_by_id(
        &self,
        run_id: &RunId,
    ) -> Result<Option<StrategyRun>>;
}
```

---

## 11.2 OrderRepository

```rust
pub trait OrderRepository {
    async fn insert_order(&self, order: &Order) -> Result<()>;

    async fn update_order_status(
        &self,
        order_id: &OrderId,
        status: OrderStatus,
    ) -> Result<()>;

    async fn update_order_fill(
        &self,
        order_id: &OrderId,
        filled_qty: Decimal,
        remaining_qty: Decimal,
        avg_fill_price: Decimal,
    ) -> Result<()>;

    async fn bind_broker_order_id(
        &self,
        order_id: &OrderId,
        broker_order_id: &str,
    ) -> Result<()>;

    async fn find_by_id(
        &self,
        order_id: &OrderId,
    ) -> Result<Option<Order>>;

    async fn find_by_client_order_id(
        &self,
        client_order_id: &str,
    ) -> Result<Option<Order>>;

    async fn find_open_orders(
        &self,
        run_id: &RunId,
    ) -> Result<Vec<Order>>;
}
```

---

## 11.3 FillRepository

```rust
pub trait FillRepository {
    async fn insert_fill(&self, fill: &Fill) -> Result<()>;

    async fn find_by_order_id(
        &self,
        order_id: &OrderId,
    ) -> Result<Vec<Fill>>;

    async fn find_by_run_id(
        &self,
        run_id: &RunId,
    ) -> Result<Vec<Fill>>;
}
```

---

## 11.4 PositionRepository

```rust
pub trait PositionRepository {
    async fn upsert_position(&self, position: &Position) -> Result<()>;

    async fn find_position(
        &self,
        run_id: &RunId,
        symbol: &Symbol,
    ) -> Result<Option<Position>>;

    async fn find_positions(
        &self,
        run_id: &RunId,
    ) -> Result<Vec<Position>>;
}
```

---

## 11.5 CryptoPositionRepository

```rust
pub trait CryptoPositionRepository {
    async fn upsert_position(
        &self,
        position: &CryptoPosition,
    ) -> Result<()>;

    async fn find_position(
        &self,
        run_id: &RunId,
        exchange: &str,
        symbol: &str,
        side: PositionSide,
    ) -> Result<Option<CryptoPosition>>;

    async fn find_positions(
        &self,
        run_id: &RunId,
    ) -> Result<Vec<CryptoPosition>>;
}
```

---

## 11.6 AccountBalanceRepository

```rust
pub trait AccountBalanceRepository {
    async fn upsert_balance(
        &self,
        balance: &AccountBalance,
    ) -> Result<()>;

    async fn find_balances(
        &self,
        run_id: &RunId,
    ) -> Result<Vec<AccountBalance>>;

    async fn find_balance(
        &self,
        run_id: &RunId,
        asset: &str,
    ) -> Result<Option<AccountBalance>>;
}
```

---

## 11.7 PortfolioSnapshotRepository

```rust
pub trait PortfolioSnapshotRepository {
    async fn insert_snapshot(
        &self,
        snapshot: &PortfolioSnapshot,
    ) -> Result<()>;

    async fn find_by_run_id(
        &self,
        run_id: &RunId,
    ) -> Result<Vec<PortfolioSnapshot>>;
}
```

---

# 12. 数据写入策略

## 12.1 SQLite 写入策略

```text
订单创建：
  立即写入

订单状态变化：
  先写 event_store
  再写 order_events 投影
  同步更新 orders

成交：
  立即写入 fills
  同步更新 orders
  同步更新 positions / crypto_positions / account_balances

风控决策 / reconciliation drift：
  先写 event_store
  再写 risk_events 投影

组合快照：
  Backtest 可按 bar 写入
  Replay 可按秒或 bar 写入
  Paper / Live 可按事件或固定 interval 写入
  Live 启动时写 baseline cash snapshot，并可按 broker snapshot interval 持续写入 broker cash/position snapshots

系统日志：
  关键错误写 SQLite
  详细日志写文件
```

---

## 12.2 Parquet 写入策略

```text
日线：
  按 year 写文件

分钟线：
  按 year/month 写文件

Tick：
  按 year/month/day 写文件

OrderBook：
  按 year/month/day 写文件

Funding Rate：
  按 year 写文件

Open Interest：
  按 year/month 写文件

Features：
  按 feature_name/year 写文件
```

---

# 13. 状态恢复设计

系统重启后必须可以恢复：

```text
strategy_runs
orders
positions
crypto_positions
account_balances
cash_snapshots
portfolio_snapshots
configs
```

当前 live 启动恢复额外依赖：

```text
broker open orders
broker executions
order_events
system_logs
```

恢复流程：

```text
读取最近 RUNNING 的 strategy_runs
  ↓
读取 orders 中未完成订单
  ↓
读取本地 fills 作为已成交下限
  ↓
读取 positions / crypto_positions 当前持仓
  ↓
读取 account_balances 当前账户余额
  ↓
向 Broker 查询 open orders / executions
  ↓
同步本地订单状态与 fills
  ↓
写 broker.order.recovered / startup_recovery.* 审计记录
  ↓
  继续运行或标记为 FAILED
```

---

# 14. 订单幂等设计

订单使用 `client_order_id` 保证幂等。

规则：

```text
同一个 client_order_id 只能创建一张订单
重试下单时必须复用 client_order_id
Broker 返回 broker_order_id 后建立映射
如果网络超时，先通过 client_order_id 查询订单状态
不能盲目重新下单
```

相关字段：

```text
orders.client_order_id
orders.broker_order_id
order_events.payload_json
event_store.payload_json
```

---

# 15. A股 T+1 持仓设计

A股需要区分：

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

卖出校验：

```text
sell_qty <= available_qty
```

相关表：

```text
positions
position_snapshots
fills
```

---

# 16. 数字货币持仓设计

## 16.1 现货

数字货币现货使用：

```text
positions
account_balances
```

示例：

```text
BTCUSDT 买入 BTC：
  account_balances.USDT 减少
  account_balances.BTC 增加
  positions.BTCUSDT 增加
```

---

## 16.2 永续 / 交割合约

数字货币合约使用：

```text
crypto_positions
account_balances
funding_rates
```

需要维护：

```text
position_side
entry_price
mark_price
liquidation_price
leverage
margin_mode
initial_margin
maintenance_margin
unrealized_pnl
realized_pnl
funding_fee
margin_ratio
```

---

# 17. 多币种设计

Trader 必须支持多币种。

股票市场：

```text
CN: CNY
HK: HKD
US: USD
```

数字货币：

```text
USDT
USDC
BTC
ETH
```

账户余额统一使用：

```text
account_balances
```

组合估值使用：

```text
base_currency
```

未来可以增加：

```text
fx_rates
```

V1 暂不强制实现复杂汇率换算。

---

# 18. 查询示例

## 18.1 查询某次运行订单

```sql
SELECT *
FROM orders
WHERE run_id = ?
ORDER BY created_at ASC;
```

---

## 18.2 查询未完成订单

```sql
SELECT *
FROM orders
WHERE run_id = ?
  AND status IN (
    'NEW',
    'PENDING_SUBMIT',
    'SUBMITTED',
    'PARTIALLY_FILLED',
    'PENDING_CANCEL',
    'UNKNOWN',
    'SYNCING'
  );
```

---

## 18.3 查询成交记录

```sql
SELECT *
FROM fills
WHERE run_id = ?
ORDER BY ts ASC;
```

---

## 18.4 查询最新组合快照

```sql
SELECT *
FROM portfolio_snapshots
WHERE run_id = ?
ORDER BY ts DESC
LIMIT 1;
```

---

## 18.5 查询风险事件

```sql
SELECT *
FROM risk_events
WHERE run_id = ?
ORDER BY ts ASC;
```

---

## 18.6 查询数字货币合约持仓

```sql
SELECT *
FROM crypto_positions
WHERE run_id = ?
ORDER BY updated_ts DESC;
```

---

## 18.7 查询账户余额

```sql
SELECT *
FROM account_balances
WHERE run_id = ?
ORDER BY asset ASC;
```

---

# 19. Migration 目录设计

```text
migrations/
├── 0001_init.sql
├── 0002_audit_projections.sql
├── 0003_market_rules.sql
├── 0004_contract_accounting.sql
├── 0005_reference_snapshots_and_ops.sql
└── 0006_config_lifecycle.sql
```

---

## 19.1 0001_init.sql

```sql
PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA temp_store = MEMORY;
PRAGMA busy_timeout = 5000;
```

---

## 19.2 0002_audit_projections.sql

包含：

```text
order_events
risk_events
insights
portfolio_targets
```

---

## 19.3 0003_market_rules.sql

包含：

```text
market_calendars
trading_sessions
fee_rules
lot_size_rules
price_limit_rules
```

---

## 19.4 0004_contract_accounting.sql

包含：

```text
crypto_positions
funding_rates
```

---

## 19.5 0005_reference_snapshots_and_ops.sql

包含：

```text
crypto_market_meta
corporate_actions_meta
cash_snapshots
position_snapshots
configs
system_logs
```

---

## 19.6 0006_config_lifecycle.sql

包含：

```text
config_releases
run_config_versions
config_audits
```

---

# 20. 性能建议

## 20.1 SQLite

```text
开启 WAL
使用事务批量写入
不要每个 tick 写一次 portfolio_snapshot
Backtest 可按 bar 写快照
Replay 可按秒级或 bar 写快照
Paper / Live 关键事件必须立即写入
订单、成交、风控事件必须立即写入
```

---

## 20.2 Parquet

```text
按 market / symbol / year / month 分区
避免大量小文件
日线按年文件
分钟线按月文件
Tick 按日文件
OrderBook 按日文件
Funding Rate 按年文件
Open Interest 按月文件
因子按年文件
```

---

# 21. V1 不做的事情

V1 不实现：

```text
PostgreSQL
ClickHouse
Redis
Kafka
分布式存储
多节点一致性
实时 OLAP
Tick 全量写 SQLite
OrderBook 全量写 SQLite
复杂汇率换算
多用户权限系统
```

---

# 22. 后续扩展

未来可以扩展：

```text
PostgreSQL:
  多用户、多账户、服务化部署

ClickHouse:
  大规模行情查询和指标分析

Redis:
  实时状态缓存

Kafka / Redpanda:
  分布式事件流

DuckDB:
  本地 Parquet 分析查询

S3 / MinIO:
  对象存储历史行情

fx_rates:
  多币种估值和汇率换算
```

---

# 23. 设计结论

Trader 的数据库设计原则是：

```text
SQLite 管交易状态
Parquet 管历史行情
Repository 层隔离 SQL
Order 使用 client_order_id 保证幂等
所有运行结果通过 run_id 关联
所有关键状态都可以恢复
股票和数字货币现货复用 positions
数字货币合约使用 crypto_positions
多币种账户使用 account_balances
永续资金费率使用 funding_rates
MarketRule 按 CN / HK / US / CRYPTO 插件化
```

V1 成功标准：

```text
一次 backtest 可以完整落库
一次 replay 可以实时写入快照
paper trading 可以恢复订单、持仓和账户状态
A股 T+1 可以正确校验
港股 lot size 可以正确校验
美股碎股规则可以正确校验
数字货币现货余额可以正确维护
数字货币永续仓位可以正确维护
资金费率可以记录和参与 PnL
历史行情可以从 Parquet 按 symbol / time range 扫描
```
