# LSTM 策略 + 长桥 Paper 账号全链路设计

> 历史说明：本文中的 `services/lstm-service` 与 `lstm.service_url` 描述的是最初设计；当前有效路径为 `services/model`，主配置 key 为 `model.service_url`。

日期：2026-04-07  
状态：Draft  
范围：lstm-service（Python）、LstmStrategy（Rust）、长桥 paper 账号接入、策略配置持久化

---

## 1. 背景与目标

当前 `quantd` 已有完整的 ingest → strategy → risk → exec pipeline，但策略层只有规则型占位（`NoOpStrategy`、`AlwaysLongOne`），执行层只有本地模拟盘（`PaperAdapter`）和长桥实盘（`LongbridgeTradeAdapter`）。

本规格交付：

1. **lstm-service**：独立 Python FastAPI 服务，基于 Qlib，支持 LSTM/ALSTM 等时序模型的训练、推理、回测与诊断。
2. **LstmStrategy**：Rust 侧策略实现，HTTP 调用 lstm-service 获取信号，实现 `Strategy` trait（改为 async）。
3. **长桥 paper 账号**：从 `execution_profiles` 表读取凭证，动态构建 `LongbridgeTradeAdapter`，注册为 `acc_lb_paper`。
4. **策略配置持久化**：通过 `system_config` 表配置策略参数，账号与策略绑定关系入库。

---

## 2. 总体架构

```
┌─────────────────────────────────────────────────────────┐
│                   quantd (Rust)                         │
│                                                         │
│  LongbridgeCandleIngest → bars 表                       │
│       ↓                                                 │
│  LstmStrategy ──HTTP POST /predict──→ lstm-service      │
│       ↓ Signal                                          │
│  RiskLimits check                                       │
│       ↓                                                 │
│  ExecutionRouter                                        │
│    acc_lb_paper → LongbridgeTradeAdapter(paper creds)   │
│    acc_lb_live  → LongbridgeTradeAdapter(live creds)    │
│    acc_mvp_paper→ PaperAdapter                          │
└─────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────┐
│             lstm-service (Python FastAPI)                │
│                                                         │
│  POST /train      Qlib 拉数据 → Alpha158 → 训练模型      │
│  POST /predict    Alpha158 特征 → 模型推理 → score/side  │
│  POST /backtest   历史区间回测 → 收益指标                 │
│  GET  /models     已训练模型列表                         │
│  GET  /features/{symbol}  最近一期 Alpha158 特征诊断     │
│  GET  /health     服务存活                               │
└─────────────────────────────────────────────────────────┘
```

**不改动**：`ExecutionRouter` 核心逻辑、`PipelineError`、`api` 路由、`PaperAdapter`、ingest 链路。

---

## 3. lstm-service

### 3.1 目录结构

```
services/lstm-service/
  main.py                  FastAPI 入口
  models/                  训练好的模型文件 lstm_{symbol}_{model_type}.pt
  qlib_pipeline/
    train.py               训练逻辑
    predict.py             推理逻辑
    backtest.py            回测逻辑
    features.py            Alpha158 特征计算
  requirements.txt         qlib, torch, fastapi, uvicorn, pandas
```

### 3.2 API

#### POST /train
```json
// 请求
{ "symbol": "AAPL.US", "model_type": "lstm|alstm|gru|transformer",
  "start": "2020-01-01", "end": "2024-12-31" }

// 响应
{ "model_id": "lstm_AAPL.US_alstm_20260407",
  "metrics": { "ic": 0.05, "icir": 0.42, "sharpe": 1.2, "annualized_return": 0.15 } }
```

#### POST /predict
```json
// 请求
{ "symbol": "AAPL.US", "model_type": "alstm",
  "bars": [{ "ts_ms": 1700000000000, "open": 180.0, "high": 182.0,
              "low": 179.0, "close": 181.0, "volume": 50000000 }, ...] }
// bars 数量 >= lookback（默认 60）

// 响应
{ "score": 0.73, "side": "buy|sell|hold", "confidence": 0.81 }
```

#### POST /backtest
```json
// 请求
{ "symbol": "AAPL.US", "start": "2025-01-01", "end": "2026-01-01",
  "model_id": "lstm_AAPL.US_alstm_20260407" }

// 响应
{ "annualized_return": 0.18, "sharpe": 1.4, "max_drawdown": -0.09,
  "win_rate": 0.54, "trades": [...] }
```

#### GET /models
```json
[{ "model_id": "lstm_AAPL.US_alstm_20260407", "symbol": "AAPL.US",
   "model_type": "alstm", "trained_at": "2026-04-07T10:00:00Z",
   "ic": 0.05, "sharpe": 1.2 }]
```

#### GET /features/{symbol}
```json
{ "symbol": "AAPL.US", "ts_ms": 1744000000000,
  "alpha158": { "ROC5": 0.012, "STD20": 0.018, "CORR20": -0.03, "..." : "..." } }
```

#### GET /health
```json
{ "status": "ok", "models_loaded": 3 }
```

### 3.3 模型支持

MVP 实现 `lstm` 和 `alstm`，其余占位：

| model_type | Qlib 类 | 说明 |
|---|---|---|
| `lstm` | `qlib.contrib.model.pytorch_lstm.LSTM` | 标准 LSTM |
| `alstm` | `qlib.contrib.model.pytorch_alstm.ALSTM` | LSTM + Attention，推荐 |
| `gru` | `qlib.contrib.model.pytorch_gru.GRU` | 占位，后续实现 |
| `transformer` | `qlib.contrib.model.pytorch_transformer.Transformer` | 占位，后续实现 |

### 3.4 特征

使用 Qlib `Alpha158`（158 维技术因子），完整集合包含动量、波动率、成交量、趋势类因子。数据源：
- 美股（`AAPL.US`）：Qlib `YahooStockData` provider
- 港股（`700.HK`）：后续扩 provider
- A 股：后续扩 provider

---

## 4. Rust 侧：Strategy trait 改为 async

### 4.1 trait 变更

```rust
// crates/strategy/src/core/trait.rs（修改）
#[async_trait]
pub trait Strategy: Send + Sync {
    async fn evaluate(&self, context: &StrategyContext) -> Option<Signal>;
}
```

所有现有策略（`NoOpStrategy`、`AlwaysLongOne`）同步更新为 async 实现（函数体不变，加 `async`）。

### 4.2 LstmStrategy

```
crates/strategy/src/lstm.rs（新增）
```

字段：
- `client: reqwest::Client`
- `service_url: String`（从 system_config 读取，key: `lstm.service_url`）
- `model_type: String`（"alstm"）
- `lookback: usize`（默认 60）
- `buy_threshold: f64`（默认 0.6）
- `sell_threshold: f64`（默认 -0.6）

`evaluate()` 流程：
1. 从 `bars` 表读最近 `lookback` 根 K 线（按 `ts_ms DESC LIMIT lookback`）
2. `POST /predict` 发给 lstm-service，超时 3 秒
3. `score > buy_threshold` → `Signal { side: Buy, ... }`
4. `score < sell_threshold` → `Signal { side: Sell, ... }`
5. 否则 → `None`（hold）

---

## 5. 长桥 Paper 账号接入

### 5.1 数据库配置

利用现有 `execution_profiles(id, kind, config_json)` 和 `accounts(id, mode, execution_profile_id, venue)` 表。

Paper 账号通过 seed 函数写入（不通过环境变量）：

```sql
INSERT OR IGNORE INTO execution_profiles VALUES (
  'longbridge_paper', 'longbridge_paper',
  '{"app_key":"<paper_key>","app_secret":"<paper_secret>","access_token":"<paper_token>"}'
);

INSERT OR IGNORE INTO accounts VALUES (
  'acc_lb_paper', 'paper', 'longbridge_paper', NULL
);
```

新增 `db::ensure_longbridge_paper_account(pool, app_key, app_secret, access_token)` 函数（通过 API 写入，或管理员手动 INSERT）。

### 5.2 启动时动态构建 ExecutionRouter

`main.rs` 启动逻辑：

1. 查询 `execution_profiles WHERE kind IN ('longbridge_live','longbridge_paper')`
2. 解析 `config_json` 得到凭证，用 `Config::from_apikey(app_key, app_secret, access_token)` 构建各自 `LongbridgeClients`
3. 查询 `accounts WHERE enabled=1`，按 `execution_profile_id` 映射到对应 adapter
4. 构建 `ExecutionRouter`

环境变量凭证（`LONGBRIDGE_APP_KEY` 等）仅作为 `longbridge_live` 回退，向后兼容。

### 5.3 ExecutionRouter 热重载

将 `AppState.execution_router` 类型从 `ExecutionRouter` 改为 `Arc<RwLock<ExecutionRouter>>`，账号变更 API 写 DB 后同步更新内存路由。

---

## 6. 策略配置持久化

使用现有 `system_config(key, value)` 表：

| key | value（示例） |
|---|---|
| `lstm.service_url` | `http://127.0.0.1:8000` |
| `strategy.acc_lb_paper` | `{"type":"lstm","model_type":"alstm","symbol":"AAPL.US","lookback":60,"buy_threshold":0.6,"sell_threshold":-0.6}` |

`QUANTD_STRATEGY` 环境变量保留，仅用于 dev/test（`noop`、`always_long_one`）。生产环境读 DB 配置。

**HTTP API 扩展**：
```
GET  /v1/strategy/config        查看当前策略配置
PUT  /v1/strategy/config        更新策略配置
```

---

## 7. 错误处理

| 场景 | 行为 |
|---|---|
| lstm-service 不可达 | `PipelineError::Strategy("lstm_service_unavailable")` → HTTP 502，不下单，写 audit_log |
| lstm-service 超时（3s） | 同上 |
| score 在阈值区间（hold） | `Ok(None)`，正常跳过 |
| 长桥 paper 凭证失效 | `ExecError::Longbridge(...)` → HTTP 502，`error_code: broker_error` |
| 账号未配置 | `ExecError::NotConfigured` → HTTP 500，`error_code: execution_not_configured` |

---

## 8. 测试策略

### lstm-service（Python）
- 单元测试：Alpha158 特征计算正确性、predict 输出 score ∈ [-1, 1]
- 集成测试：Yahoo Finance 拉真实历史数据，跑完整 train → predict 流程

### Rust 侧
- `LstmStrategy` 单元测试：`wiremock` mock HTTP server，验证阈值逻辑
- `ExecutionRouter` 热重载：DB 写入新账号后路由更新正确
- Pipeline 集成测试：`LstmStrategy` + `PaperAdapter`，不依赖真实长桥

### 端到端冒烟
1. 启动 `quantd` + `lstm-service`（本地）
2. `POST /v1/tick` 触发一次 pipeline
3. 验证：lstm-service 收到 predict 请求、长桥 paper 账号收到订单

---

## 9. 不在本规格范围

- A 股、港股数据源（provider 扩展后续规格）
- GRU、Transformer 模型完整实现（占位，后续规格）
- 凭证加密存储（明文适合本地单机，加密后续规格）
- 多标的组合管理
