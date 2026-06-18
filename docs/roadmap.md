# roadmap.md

## 1. Overview

Trader 采用渐进式开发路线。

## Current V1 Local Verification

当前分支完成的是 V1 local-verifiable release：本地 SQLite、Parquet、CLI、REST、WebSocket、Backtest、Replay、Paper、Live surface、fake broker adapters、报告导出均可通过 `scripts/v1-smoke.ps1` 验证。

这不等于生产实盘完成。真实 Futu/Binance/OKX/IB 网络连接、凭证管理、真实资金下单、生产级权限、监控告警和分布式部署仍属于后续 production/live-real-money 阶段。

## Schema Gap Closure

- Keep `event_store` as immutable audit truth.
- `order_events`、`risk_events`、`insights` and `portfolio_targets` exist as query projections.
- Market-rule reference tables exist as storage boundary; runtime rule assembly still needs phased wiring before claiming configurable multi-market support.
- `crypto_positions` and `funding_rates` exist as storage boundary and read-only API/CLI query surface; simulated paper runtime writes contract positions and funding settlement, and Binance reconciliation has drift-detection tests. Production crypto derivative accounting still needs real broker reconciliation scheduling, IBKR contract reconciliation, and reference-data ingestion.
- `cash_snapshots` and `position_snapshots` are captured by paper runtime; live/reconciliation snapshot capture remains follow-up work.
- API-launched Backtest, Paper, and Replay runs capture `RUN` config snapshots in `configs`; config approval/release lifecycle remains follow-up work.
- API-launched Backtest, Paper, and Replay runs index lifecycle messages in `system_logs`; broader production log indexing remains follow-up work.
- `crypto_market_meta` and `corporate_actions_meta` exist as storage boundary and read-only API query surface; automatic ingestion remains follow-up work.

## Contract Runtime Accounting Milestone

Current status:

```text
1. Storage boundary: complete
2. Simulated accounting: complete for paper CRYPTO_PERP / CRYPTO_FUTURE fills
3. Funding settlement: complete for simulated paper funding events
4. Broker reconciliation: Binance drift detection covered by broker tests
5. Contract risk checks: leverage, margin ratio, notional, liquidation buffer, funding bounds
6. CLI/API readback: complete for contract positions and funding rates
```

Remaining:

```text
1. Reference-data ingestion for exchange contract metadata and historical funding rates
2. Scheduled broker reported position snapshots
3. IBKR contract reconciliation
4. Live/reconciliation snapshot persistence
5. Production alerts and audit reports for reconciliation drift
```

目标：

```text
Phase 1
可运行回测

Phase 2
可运行模拟盘

Phase 3
可运行实盘

Phase 4
多市场统一交易平台

Phase 5
分布式量化交易系统
```

核心原则：

```text
先跑通

再稳定

再优化

最后扩展
```

---

# Phase 1

## Core Backtest Engine

目标：

```text
完成单机回测系统

支持策略开发

支持结果分析
```

---

## Features

### Market Data

实现：

```text
Parquet Reader

Bar Loader

Tick Loader

Trading Calendar

Data Cache
```

---

### Event Engine

实现：

```text
Event Bus

Event Dispatcher

Event Persistence

Replay Event Loader
```

---

### Strategy

实现：

```text
Strategy Trait

Strategy Registry

Strategy Context

SignalEvent
```

---

### Portfolio

实现：

```text
TargetPosition

Position Tracking

Capital Allocation

Portfolio Snapshot
```

---

### Risk

实现：

```text
Max Position

Max Exposure

Max Drawdown

Trading Halt
```

---

### Execution

实现：

```text
Target Position
→
Order Intent
```

---

### OMS

实现：

```text
Order Lifecycle

Order State Machine

Order Repository
```

---

### Mock Broker

实现：

```text
Market Order

Limit Order

Commission

Slippage
```

---

### Accounting

实现：

```text
Position

Cash

Equity

PnL
```

---

### Metrics

实现：

```text
Return

Sharpe

Sortino

Max Drawdown

Win Rate
```

---

## Deliverables

```text
Backtest

Replay

CLI

HTML Report

CSV Export
```

---

## Exit Criteria

```text
100+ symbols

10 years data

single strategy

stable backtest
```

---

# Phase 2

## Paper Trading

目标：

```text
支持实时行情

支持模拟交易

支持策略验证
```

---

## Features

### Market Data Gateway

实现：

```text
Futu

IB

Binance

OKX
```

实时行情接入。

---

### Live Event Engine

实现：

```text
Async Event Bus

Realtime Dispatcher

WebSocket Broadcast
```

---

### Paper Broker

实现：

```text
实时订单

模拟成交

盘口撮合

延迟模拟
```

---

### REST API

实现：

```text
Account

Orders

Positions

Strategies

Metrics
```

---

### WebSocket API

实现：

```text
Order Stream

Fill Stream

Position Stream

Account Stream
```

---

### Runtime Control

实现：

```text
Start Strategy

Stop Strategy

Reload Config
```

---

## Deliverables

```text
Realtime Paper Trading

REST API

WebSocket API

Multi Strategy
```

---

## Exit Criteria

```text
24h continuous run

strategy restart

state recovery
```

---

# Phase 3

## Live Trading

目标：

```text
连接真实券商

支持实盘交易
```

---

## Features

### Broker Integration

实现：

```text
Interactive Brokers

Futu

Binance

OKX
```

---

### Market Rules

实现：

```text
CN Equity

HK Equity

US Equity

Crypto Spot

Crypto Perp
```

---

### Advanced Risk

实现：

```text
Account Risk

Portfolio Risk

Daily Loss Limit

Kill Switch

Circuit Breaker
```

---

### Monitoring

实现：

```text
Health Check

Heartbeat

Alerting

Audit Logs
```

---

### Recovery

实现：

```text
Restart Recovery

Order Sync

Position Sync

Account Sync
```

---

## Deliverables

```text
Paper

Live

Broker Integration

Production Deployment
```

---

## Exit Criteria

```text
7x24 stable

real capital trading

automatic recovery
```

---

# Phase 4

## Multi Asset Platform

目标：

```text
统一股票和数字货币

统一账户体系

统一策略接口
```

---

## Features

### Asset Types

支持：

```text
EQUITY

ETF

CRYPTO_SPOT

CRYPTO_PERP

CRYPTO_FUTURE
```

---

### Cross Asset Portfolio

实现：

```text
Unified Exposure

Cross Asset Risk

Multi Currency NAV
```

---

### Currency Engine

实现：

```text
USD

HKD

CNY

USDT
```

---

### FX Conversion

实现：

```text
Realtime FX

Historical FX

PnL Conversion
```

---

### Portfolio Analytics

实现：

```text
Factor Exposure

Sector Exposure

Market Exposure

Currency Exposure
```

---

## Deliverables

```text
Multi Asset Trading

Unified Portfolio

Unified Risk
```

---

## Exit Criteria

```text
One strategy

Multi markets

Multi accounts

Unified reporting
```

---

# Phase 5

## Research Platform

目标：

```text
量化研究平台
```

---

## Features

### Factor Engine

实现：

```text
Factor Pipeline

Factor Registry

Factor Backfill
```

---

### Feature Store

实现：

```text
Offline Feature

Online Feature

Feature Cache
```

---

### Alpha Research

实现：

```text
Cross Section

Time Series

Factor Research
```

---

### Hyper Parameter Search

实现：

```text
Grid Search

Random Search

Walk Forward
```

---

### Strategy Lab

实现：

```text
Experiment Tracking

Result Registry

Version Management
```

---

## Deliverables

```text
Research Workflow

Alpha Discovery

Experiment Tracking
```

---

# Phase 6

## Distributed Architecture

目标：

```text
支持多节点部署
```

---

## Features

### Service Split

```text
Market Data Service

Strategy Service

Execution Service

Risk Service

OMS Service
```

---

### Message Queue

支持：

```text
NATS

Kafka
```

---

### Distributed Event Bus

实现：

```text
Event Routing

Event Persistence

Replay
```

---

### Horizontal Scaling

实现：

```text
Multiple Strategy Workers

Multiple Market Workers

Multiple Execution Workers
```

---

## Deliverables

```text
Cluster Deployment

Distributed Backtest

Distributed Live Trading
```

---

# Phase 7

## Institutional Grade

目标：

```text
机构级交易平台
```

---

## Features

### Smart Order Routing

```text
SOR
```

---

### TWAP

```text
Time Weighted Execution
```

---

### VWAP

```text
Volume Weighted Execution
```

---

### Iceberg

```text
Hidden Liquidity
```

---

### Portfolio Optimization

```text
Risk Parity

Mean Variance

Black Litterman
```

---

### Compliance

```text
Audit Trail

Permission Control

Operation Logs
```

---

## Deliverables

```text
Institutional Trading Platform
```

---

# MVP Scope

优先实现：

```text
Phase 1
+
Phase 2
+
部分 Phase 3
```

具体：

```text
Backtest

Replay

Paper

Live

Futu

Binance

SQLite

Parquet

REST API

WebSocket API
```

不做：

```text
Distributed

Kafka

Factor Research

Cluster

SOR

TWAP

VWAP
```

---

# Recommended Release Plan

## v0.1

```text
Event Bus

Strategy

Backtest

Mock Broker
```

---

## v0.2

```text
Portfolio

Risk

Accounting

Metrics
```

---

## v0.3

```text
Replay

REST API

WebSocket
```

---

## v0.4

```text
Paper Trading
```

---

## v0.5

```text
Futu Integration
```

---

## v0.6

```text
Binance Integration
```

---

## v1.0

```text
Production Ready

Paper

Live

A股

港股

美股

数字货币
```
