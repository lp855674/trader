# LSTM 策略 + 长桥 Paper 账号 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 通过长桥 paper 账号实现基于 LSTM 的真实下单全链路：lstm-service (Python) 提供训练/推理/回测，Rust LstmStrategy 调用推理接口生成信号，ExecutionRouter 路由到长桥 paper 账号完成下单。

**Architecture:** Python FastAPI 服务（lstm-service）负责 Qlib Alpha158 特征工程 + LSTM/ALSTM 模型训练与推理；Rust `LstmStrategy` 实现 async `Strategy` trait，读取最近 N 根 K 线通过 HTTP 调用 lstm-service /predict 获取信号；长桥 paper 账号凭证存储在 `execution_profiles.config_json`，启动时从 DB 读取动态构建 `ExecutionRouter`。

**Tech Stack:** Python 3.11+, FastAPI, Qlib, PyTorch, uvicorn; Rust async-trait, reqwest, sqlx, longbridge SDK

---

## 文件变更索引

### 新增
- `services/lstm-service/main.py` — FastAPI 入口，路由注册
- `services/lstm-service/qlib_pipeline/train.py` — Qlib 训练逻辑（LSTM/ALSTM）
- `services/lstm-service/qlib_pipeline/predict.py` — Alpha158 特征计算 + 推理
- `services/lstm-service/qlib_pipeline/backtest.py` — Qlib 回测逻辑
- `services/lstm-service/qlib_pipeline/features.py` — 最近一期特征诊断
- `services/lstm-service/requirements.txt` — Python 依赖
- `services/lstm-service/tests/test_predict.py` — predict 单元测试
- `services/lstm-service/tests/test_train.py` — train 集成测试
- `crates/strategy/src/lstm.rs` — LstmStrategy 实现
- `crates/db/migrations/004_system_config_utils.sql` — 无新表，迁移占位注释

### 修改
- `crates/strategy/src/strategy.rs` — Strategy trait 改为 async
- `crates/strategy/src/lib.rs` — 导出 LstmStrategy
- `crates/strategy/Cargo.toml` — 添加 reqwest, db 依赖
- `crates/pipeline/src/pipeline.rs` — strategy.evaluate().await + Strategy variant
- `crates/db/src/bars.rs` — 新增 get_recent_bars
- `crates/db/src/db.rs` — 导出 get_recent_bars, system_config, execution_profiles 函数
- `crates/db/src/bootstrap.rs` — 新增 ensure_longbridge_paper_account
- `crates/db/src/system_config.rs` (新文件) — get/set system_config
- `crates/db/src/execution_profiles.rs` (新文件) — load_execution_profiles_by_kind
- `crates/longbridge_adapters/src/clients.rs` — 新增 connect_with_credentials
- `crates/quantd/src/main.rs` — 从 DB 动态构建 ExecutionRouter + LstmStrategy
- `crates/api/src/api.rs` — 新增 /v1/strategy/config 路由
- `crates/api/src/handlers.rs` — get/put strategy config handler

---

## Task 1: lstm-service 骨架 + /health

**Files:**
- Create: `services/lstm-service/main.py`
- Create: `services/lstm-service/requirements.txt`
- Create: `services/lstm-service/qlib_pipeline/__init__.py`
- Create: `services/lstm-service/tests/__init__.py`

- [ ] **Step 1: 创建目录结构**

```bash
mkdir -p services/lstm-service/qlib_pipeline
mkdir -p services/lstm-service/tests
mkdir -p services/lstm-service/models
```

- [ ] **Step 2: 写 requirements.txt**

```
# services/lstm-service/requirements.txt
fastapi==0.115.0
uvicorn[standard]==0.30.6
pydantic==2.8.2
pyqlib==0.9.6
torch==2.3.1
numpy==1.26.4
pandas==2.2.2
httpx==0.27.2   # for tests
pytest==8.3.2
pytest-asyncio==0.23.8
```

- [ ] **Step 3: 写 main.py 骨架**

```python
# services/lstm-service/main.py
from __future__ import annotations

import os
from pathlib import Path

from fastapi import FastAPI

from qlib_pipeline.train import router as train_router
from qlib_pipeline.predict import router as predict_router
from qlib_pipeline.backtest import router as backtest_router
from qlib_pipeline.features import router as features_router

MODELS_DIR = Path(os.getenv("LSTM_MODELS_DIR", "models"))
MODELS_DIR.mkdir(exist_ok=True)

app = FastAPI(title="lstm-service", version="0.1.0")

app.include_router(train_router)
app.include_router(predict_router)
app.include_router(backtest_router)
app.include_router(features_router)


@app.get("/health")
async def health() -> dict:
    model_files = list(MODELS_DIR.glob("*.pt"))
    return {"status": "ok", "models_loaded": len(model_files)}
```

- [ ] **Step 4: 创建空模块占位**

```python
# services/lstm-service/qlib_pipeline/__init__.py
```

```python
# services/lstm-service/tests/__init__.py
```

- [ ] **Step 5: 写 /health 测试**

```python
# services/lstm-service/tests/test_health.py
import pytest
from fastapi.testclient import TestClient
from main import app

client = TestClient(app)

def test_health():
    resp = client.get("/health")
    assert resp.status_code == 200
    data = resp.json()
    assert data["status"] == "ok"
    assert "models_loaded" in data
```

- [ ] **Step 6: 安装依赖并运行测试**

```bash
cd services/lstm-service
pip install -r requirements.txt
pytest tests/test_health.py -v
```

Expected: `PASSED`

- [ ] **Step 7: Commit**

```bash
git add services/lstm-service/
git commit -m "feat(lstm-service): scaffold FastAPI service with /health endpoint"
```

---

## Task 2: lstm-service /predict 端点

**Files:**
- Create: `services/lstm-service/qlib_pipeline/predict.py`
- Create: `services/lstm-service/tests/test_predict.py`

- [ ] **Step 1: 写 /predict 失败测试（模型不存在时返回 404）**

```python
# services/lstm-service/tests/test_predict.py
import pytest
from fastapi.testclient import TestClient
from main import app

client = TestClient(app)

SAMPLE_BARS = [
    {"ts_ms": 1700000000000 + i * 86400000,
     "open": 180.0 + i * 0.1, "high": 182.0, "low": 179.0,
     "close": 181.0 + i * 0.05, "volume": 50_000_000}
    for i in range(60)
]

def test_predict_missing_model_returns_404():
    resp = client.post("/predict", json={
        "symbol": "NONEXISTENT.US",
        "model_type": "lstm",
        "bars": SAMPLE_BARS,
    })
    assert resp.status_code == 404

def test_predict_too_few_bars_returns_422():
    resp = client.post("/predict", json={
        "symbol": "AAPL.US",
        "model_type": "lstm",
        "bars": SAMPLE_BARS[:5],  # only 5, need 60
    })
    assert resp.status_code == 422

def test_predict_schema():
    """Response shape is correct when a saved model exists (smoke test with mock)."""
    import torch, os
    from pathlib import Path
    # Create a trivial saved model stub for testing
    models_dir = Path(os.getenv("LSTM_MODELS_DIR", "models"))
    models_dir.mkdir(exist_ok=True)
    stub_path = models_dir / "AAPL_US_lstm.pt"
    # save minimal state dict
    import torch.nn as nn
    m = nn.Linear(158, 1)
    torch.save({"model_state": m.state_dict(), "model_type": "lstm",
                "input_size": 158, "hidden_size": 64, "num_layers": 2,
                "lookback": 60}, stub_path)
    try:
        resp = client.post("/predict", json={
            "symbol": "AAPL.US",
            "model_type": "lstm",
            "bars": SAMPLE_BARS,
        })
        assert resp.status_code == 200
        data = resp.json()
        assert "score" in data
        assert "side" in data
        assert data["side"] in ("buy", "sell", "hold")
        assert "confidence" in data
    finally:
        stub_path.unlink(missing_ok=True)
```

- [ ] **Step 2: 运行测试，确认失败**

```bash
cd services/lstm-service
pytest tests/test_predict.py -v
```

Expected: `ImportError` 或 `404` 因为 predict router 不存在

- [ ] **Step 3: 实现 predict.py**

```python
# services/lstm-service/qlib_pipeline/predict.py
from __future__ import annotations

import os
from pathlib import Path
from typing import List

import numpy as np
import torch
import torch.nn as nn
from fastapi import APIRouter, HTTPException
from pydantic import BaseModel, field_validator

MODELS_DIR = Path(os.getenv("LSTM_MODELS_DIR", "models"))
LOOKBACK = 60  # default

router = APIRouter()


class Bar(BaseModel):
    ts_ms: int
    open: float
    high: float
    low: float
    close: float
    volume: float


class PredictRequest(BaseModel):
    symbol: str
    model_type: str = "alstm"
    bars: List[Bar]

    @field_validator("bars")
    @classmethod
    def check_bars_length(cls, v):
        if len(v) < LOOKBACK:
            raise ValueError(f"bars must have at least {LOOKBACK} entries, got {len(v)}")
        return v


class PredictResponse(BaseModel):
    score: float
    side: str  # "buy" | "sell" | "hold"
    confidence: float


def _model_path(symbol: str, model_type: str) -> Path:
    safe = symbol.replace(".", "_")
    return MODELS_DIR / f"{safe}_{model_type}.pt"


def _bars_to_features(bars: List[Bar]) -> np.ndarray:
    """
    Convert raw OHLCV bars to a simplified 158-dim feature vector per bar.
    In production this is replaced by Qlib Alpha158; here we use OHLCV + returns
    padded to 158 dims with zeros for the stub/test path.
    """
    arr = np.array([[b.open, b.high, b.low, b.close, b.volume] for b in bars], dtype=np.float32)
    # Normalize by last close
    last_close = arr[-1, 3]
    if last_close > 0:
        arr[:, :4] /= last_close
    arr[:, 4] /= (arr[:, 4].mean() + 1e-8)
    # Pad to 158 features
    n = arr.shape[0]
    padded = np.zeros((n, 158), dtype=np.float32)
    padded[:, :5] = arr
    return padded  # shape: (lookback, 158)


class _SimpleLSTM(nn.Module):
    def __init__(self, input_size=158, hidden_size=64, num_layers=2):
        super().__init__()
        self.lstm = nn.LSTM(input_size, hidden_size, num_layers, batch_first=True)
        self.fc = nn.Linear(hidden_size, 1)

    def forward(self, x):
        out, _ = self.lstm(x)
        return self.fc(out[:, -1, :]).squeeze(-1)


def _load_model(path: Path, checkpoint: dict) -> nn.Module:
    model_type = checkpoint.get("model_type", "lstm")
    input_size = checkpoint.get("input_size", 158)
    hidden_size = checkpoint.get("hidden_size", 64)
    num_layers = checkpoint.get("num_layers", 2)
    model = _SimpleLSTM(input_size, hidden_size, num_layers)
    model.load_state_dict(checkpoint["model_state"])
    model.eval()
    return model


@router.post("/predict", response_model=PredictResponse)
async def predict(req: PredictRequest) -> PredictResponse:
    path = _model_path(req.symbol, req.model_type)
    if not path.exists():
        raise HTTPException(status_code=404,
                            detail=f"No model found for {req.symbol}/{req.model_type}. Train first.")

    checkpoint = torch.load(path, map_location="cpu", weights_only=False)
    model = _load_model(path, checkpoint)

    features = _bars_to_features(req.bars[-LOOKBACK:])  # (60, 158)
    x = torch.tensor(features).unsqueeze(0)             # (1, 60, 158)
    with torch.no_grad():
        raw_score = model(x).item()
    # Clamp to [-1, 1]
    score = max(-1.0, min(1.0, raw_score))
    confidence = abs(score)

    if score > 0.6:
        side = "buy"
    elif score < -0.6:
        side = "sell"
    else:
        side = "hold"

    return PredictResponse(score=score, side=side, confidence=confidence)
```

- [ ] **Step 4: 运行测试，确认通过**

```bash
cd services/lstm-service
pytest tests/test_predict.py -v
```

Expected: 3 tests `PASSED`

- [ ] **Step 5: Commit**

```bash
git add services/lstm-service/qlib_pipeline/predict.py services/lstm-service/tests/test_predict.py
git commit -m "feat(lstm-service): add /predict endpoint with OHLCV feature stub"
```

---

## Task 3: lstm-service /train 端点（Qlib LSTM + ALSTM）

**Files:**
- Create: `services/lstm-service/qlib_pipeline/train.py`
- Create: `services/lstm-service/tests/test_train.py`

- [ ] **Step 1: 写 /train 失败测试**

```python
# services/lstm-service/tests/test_train.py
import pytest
from fastapi.testclient import TestClient
from main import app

client = TestClient(app)

def test_train_unknown_model_type_returns_422():
    resp = client.post("/train", json={
        "symbol": "AAPL.US",
        "model_type": "unknown_model",
        "start": "2023-01-01",
        "end": "2023-12-31",
    })
    assert resp.status_code == 422

def test_train_response_schema():
    """Integration: requires internet + qlib data. Skip in CI without marker."""
    pytest.skip("Integration test: requires Qlib Yahoo data provider")
```

- [ ] **Step 2: 运行测试，确认 422 测试失败（router 不存在）**

```bash
cd services/lstm-service
pytest tests/test_train.py::test_train_unknown_model_type_returns_422 -v
```

Expected: error importing train router

- [ ] **Step 3: 实现 train.py**

```python
# services/lstm-service/qlib_pipeline/train.py
from __future__ import annotations

import os
import time
from datetime import datetime
from pathlib import Path
from typing import Literal

import numpy as np
import torch
import torch.nn as nn
from fastapi import APIRouter, HTTPException
from pydantic import BaseModel, field_validator

MODELS_DIR = Path(os.getenv("LSTM_MODELS_DIR", "models"))
MODELS_DIR.mkdir(exist_ok=True)

SUPPORTED_MODELS = {"lstm", "alstm"}

router = APIRouter()


class TrainRequest(BaseModel):
    symbol: str
    model_type: str = "alstm"
    start: str = "2020-01-01"
    end: str = "2024-12-31"

    @field_validator("model_type")
    @classmethod
    def check_model_type(cls, v):
        if v not in SUPPORTED_MODELS:
            raise ValueError(f"model_type must be one of {sorted(SUPPORTED_MODELS)}, got '{v}'")
        return v


class TrainMetrics(BaseModel):
    ic: float
    icir: float
    sharpe: float
    annualized_return: float


class TrainResponse(BaseModel):
    model_id: str
    metrics: TrainMetrics


def _model_path(symbol: str, model_type: str) -> Path:
    safe = symbol.replace(".", "_")
    return MODELS_DIR / f"{safe}_{model_type}.pt"


class _SimpleLSTM(nn.Module):
    def __init__(self, input_size=158, hidden_size=64, num_layers=2):
        super().__init__()
        self.lstm = nn.LSTM(input_size, hidden_size, num_layers, batch_first=True)
        self.fc = nn.Linear(hidden_size, 1)

    def forward(self, x):
        out, _ = self.lstm(x)
        return self.fc(out[:, -1, :]).squeeze(-1)


class _ALSTM(nn.Module):
    """LSTM with additive attention over time steps."""
    def __init__(self, input_size=158, hidden_size=64, num_layers=2):
        super().__init__()
        self.lstm = nn.LSTM(input_size, hidden_size, num_layers, batch_first=True)
        self.attention = nn.Linear(hidden_size, 1)
        self.fc = nn.Linear(hidden_size, 1)

    def forward(self, x):
        out, _ = self.lstm(x)                           # (B, T, H)
        attn_w = torch.softmax(self.attention(out), dim=1)  # (B, T, 1)
        context = (attn_w * out).sum(dim=1)             # (B, H)
        return self.fc(context).squeeze(-1)


def _build_model(model_type: str) -> nn.Module:
    if model_type == "lstm":
        return _SimpleLSTM()
    elif model_type == "alstm":
        return _ALSTM()
    raise ValueError(f"Unsupported model_type: {model_type}")


def _fetch_and_train(symbol: str, model_type: str, start: str, end: str):
    """
    Fetch data via Qlib Yahoo provider, compute Alpha158 features, train model.
    Returns (model, metrics_dict).
    """
    try:
        import qlib
        from qlib.constant import REG_US
        from qlib.data import D
        from qlib.contrib.data.handler import Alpha158
    except ImportError:
        raise HTTPException(status_code=500, detail="qlib not installed")

    # Initialize Qlib with Yahoo provider (US market)
    qlib_dir = Path(os.getenv("QLIB_DATA_DIR", "~/.qlib/qlib_data/us_data")).expanduser()
    qlib.init(provider_uri=str(qlib_dir), region=REG_US)

    # Qlib symbol format: strip ".US" suffix → "AAPL"
    qlib_symbol = symbol.split(".")[0]

    # Build Alpha158 handler
    handler = Alpha158(
        instruments=[qlib_symbol],
        start_time=start,
        end_time=end,
        fit_start_time=start,
        fit_end_time=end,
        infer_processors=[],
        learn_processors=[{"class": "RobustZScoreNorm", "kwargs": {"fields_group": "feature"}}],
    )
    df = handler.fetch()  # MultiIndex (datetime, instrument) → 158 feature cols + label

    if df.empty:
        raise HTTPException(status_code=422, detail=f"No data returned for {symbol} in {start}~{end}")

    features = df.drop(columns=["label"], errors="ignore").values.astype(np.float32)
    labels = df["label"].values.astype(np.float32) if "label" in df.columns else np.zeros(len(df), dtype=np.float32)

    # Build rolling windows (lookback=60)
    lookback = 60
    X, y = [], []
    for i in range(lookback, len(features)):
        X.append(features[i - lookback:i])
        y.append(labels[i])
    if len(X) < 10:
        raise HTTPException(status_code=422, detail="Insufficient data after windowing")

    X = torch.tensor(np.array(X))   # (N, 60, 158)
    y = torch.tensor(np.array(y))   # (N,)

    model = _build_model(model_type)
    optimizer = torch.optim.Adam(model.parameters(), lr=1e-3)
    criterion = nn.MSELoss()

    # Simple training loop (20 epochs, no validation split for MVP)
    model.train()
    for epoch in range(20):
        optimizer.zero_grad()
        pred = model(X)
        loss = criterion(pred, y)
        loss.backward()
        optimizer.step()

    # Compute IC (Information Coefficient = Pearson correlation of pred vs label)
    model.eval()
    with torch.no_grad():
        preds = model(X).numpy()
    labels_np = y.numpy()
    ic = float(np.corrcoef(preds, labels_np)[0, 1]) if labels_np.std() > 0 else 0.0
    icir = ic / (preds.std() + 1e-8)
    returns = preds * labels_np  # simplified return estimate
    sharpe = float(returns.mean() / (returns.std() + 1e-8) * np.sqrt(252))
    ann_return = float(returns.mean() * 252)

    return model, {"ic": round(ic, 4), "icir": round(icir, 4),
                   "sharpe": round(sharpe, 4), "annualized_return": round(ann_return, 4)}


@router.post("/train", response_model=TrainResponse)
async def train(req: TrainRequest) -> TrainResponse:
    model, metrics = _fetch_and_train(req.symbol, req.model_type, req.start, req.end)

    path = _model_path(req.symbol, req.model_type)
    torch.save({
        "model_state": model.state_dict(),
        "model_type": req.model_type,
        "input_size": 158,
        "hidden_size": 64,
        "num_layers": 2,
        "lookback": 60,
        "symbol": req.symbol,
        "trained_at": datetime.utcnow().isoformat(),
        "metrics": metrics,
    }, path)

    date_str = datetime.utcnow().strftime("%Y%m%d")
    model_id = f"lstm_{req.symbol.replace('.', '_')}_{req.model_type}_{date_str}"
    return TrainResponse(model_id=model_id, metrics=TrainMetrics(**metrics))
```

- [ ] **Step 4: 运行 422 测试**

```bash
cd services/lstm-service
pytest tests/test_train.py::test_train_unknown_model_type_returns_422 -v
```

Expected: `PASSED`

- [ ] **Step 5: Commit**

```bash
git add services/lstm-service/qlib_pipeline/train.py services/lstm-service/tests/test_train.py
git commit -m "feat(lstm-service): add /train endpoint with Qlib LSTM and ALSTM"
```

---

## Task 4: lstm-service /backtest + /models + /features

**Files:**
- Create: `services/lstm-service/qlib_pipeline/backtest.py`
- Create: `services/lstm-service/qlib_pipeline/features.py`
- Create: `services/lstm-service/tests/test_models_features.py`

- [ ] **Step 1: 写 /models 和 /features 测试**

```python
# services/lstm-service/tests/test_models_features.py
import torch, torch.nn as nn
from pathlib import Path
import os
from fastapi.testclient import TestClient
from main import app

client = TestClient(app)
MODELS_DIR = Path(os.getenv("LSTM_MODELS_DIR", "models"))

def _write_stub(symbol, model_type):
    MODELS_DIR.mkdir(exist_ok=True)
    safe = symbol.replace(".", "_")
    path = MODELS_DIR / f"{safe}_{model_type}.pt"
    m = nn.Linear(158, 1)
    torch.save({
        "model_state": m.state_dict(), "model_type": model_type,
        "input_size": 158, "hidden_size": 64, "num_layers": 2,
        "lookback": 60, "symbol": symbol,
        "trained_at": "2026-04-07T10:00:00",
        "metrics": {"ic": 0.05, "icir": 0.4, "sharpe": 1.2, "annualized_return": 0.15},
    }, path)
    return path

def test_get_models_lists_saved():
    path = _write_stub("AAPL.US", "alstm")
    try:
        resp = client.get("/models")
        assert resp.status_code == 200
        data = resp.json()
        assert isinstance(data, list)
        symbols = [m["symbol"] for m in data]
        assert "AAPL.US" in symbols
    finally:
        path.unlink(missing_ok=True)

def test_features_no_data_returns_404():
    resp = client.get("/features/NONEXISTENT.US")
    assert resp.status_code == 404
```

- [ ] **Step 2: 运行测试，确认失败**

```bash
cd services/lstm-service
pytest tests/test_models_features.py -v
```

Expected: `ImportError` 因为 backtest/features router 不存在

- [ ] **Step 3: 实现 features.py**

```python
# services/lstm-service/qlib_pipeline/features.py
from __future__ import annotations

import os
from pathlib import Path

from fastapi import APIRouter, HTTPException

MODELS_DIR = Path(os.getenv("LSTM_MODELS_DIR", "models"))

router = APIRouter()


@router.get("/features/{symbol}")
async def get_features(symbol: str) -> dict:
    """Return the latest Alpha158 feature vector for a symbol (diagnostic endpoint)."""
    try:
        import qlib
        from qlib.constant import REG_US
        from qlib.contrib.data.handler import Alpha158
        import pandas as pd

        qlib_dir = Path(os.getenv("QLIB_DATA_DIR", "~/.qlib/qlib_data/us_data")).expanduser()
        qlib.init(provider_uri=str(qlib_dir), region=REG_US)

        qlib_symbol = symbol.split(".")[0]
        handler = Alpha158(instruments=[qlib_symbol], infer_processors=[])
        df = handler.fetch()
        if df.empty:
            raise HTTPException(status_code=404, detail=f"No data for {symbol}")

        last = df.iloc[-1]
        ts_ms = int(last.name[0].timestamp() * 1000) if hasattr(last.name[0], "timestamp") else 0
        alpha = {k: round(float(v), 6) for k, v in last.drop("label", errors="ignore").items()}
        return {"symbol": symbol, "ts_ms": ts_ms, "alpha158": alpha}

    except ImportError:
        raise HTTPException(status_code=500, detail="qlib not available")
    except Exception as exc:
        raise HTTPException(status_code=404, detail=str(exc))
```

- [ ] **Step 4: 实现 backtest.py**

```python
# services/lstm-service/qlib_pipeline/backtest.py
from __future__ import annotations

import os
from datetime import datetime
from pathlib import Path
from typing import List, Optional

import numpy as np
import torch
from fastapi import APIRouter, HTTPException
from pydantic import BaseModel

from .predict import _model_path, _bars_to_features, _load_model, LOOKBACK

MODELS_DIR = Path(os.getenv("LSTM_MODELS_DIR", "models"))

router = APIRouter()


class BacktestRequest(BaseModel):
    symbol: str
    start: str
    end: str
    model_id: Optional[str] = None
    model_type: str = "alstm"


class TradeSummary(BaseModel):
    ts_ms: int
    side: str
    score: float
    price: float


class BacktestResponse(BaseModel):
    annualized_return: float
    sharpe: float
    max_drawdown: float
    win_rate: float
    trades: List[TradeSummary]


@router.post("/backtest", response_model=BacktestResponse)
async def backtest(req: BacktestRequest) -> BacktestResponse:
    path = _model_path(req.symbol, req.model_type)
    if not path.exists():
        raise HTTPException(status_code=404, detail=f"No model for {req.symbol}/{req.model_type}")

    checkpoint = torch.load(path, map_location="cpu", weights_only=False)
    model = _load_model(path, checkpoint)

    try:
        import yfinance as yf
        ticker = req.symbol.split(".")[0]
        df = yf.download(ticker, start=req.start, end=req.end, auto_adjust=True)
        if df.empty:
            raise HTTPException(status_code=422, detail=f"No price data for {req.symbol}")
    except ImportError:
        raise HTTPException(status_code=500, detail="yfinance not installed; add to requirements.txt")

    closes = df["Close"].values.astype(float)
    highs = df["High"].values.astype(float)
    lows = df["Low"].values.astype(float)
    opens = df["Open"].values.astype(float)
    volumes = df["Volume"].values.astype(float)
    timestamps = [int(t.timestamp() * 1000) for t in df.index]

    trades = []
    returns = []
    position = 0.0

    for i in range(LOOKBACK, len(closes)):
        from .predict import Bar
        bars = [
            Bar(ts_ms=timestamps[j], open=opens[j], high=highs[j],
                low=lows[j], close=closes[j], volume=volumes[j])
            for j in range(i - LOOKBACK, i)
        ]
        features = _bars_to_features(bars)
        x = torch.tensor(features).unsqueeze(0)
        with torch.no_grad():
            raw = model(x).item()
        score = max(-1.0, min(1.0, raw))

        day_return = (closes[i] - closes[i - 1]) / (closes[i - 1] + 1e-8)

        if score > 0.6 and position == 0.0:
            position = 1.0
            trades.append(TradeSummary(ts_ms=timestamps[i], side="buy",
                                       score=score, price=closes[i]))
        elif score < -0.6 and position == 1.0:
            position = 0.0
            trades.append(TradeSummary(ts_ms=timestamps[i], side="sell",
                                       score=score, price=closes[i]))

        returns.append(position * day_return)

    returns_arr = np.array(returns)
    ann_return = float(returns_arr.mean() * 252)
    sharpe = float(returns_arr.mean() / (returns_arr.std() + 1e-8) * np.sqrt(252))

    cum = np.cumprod(1 + returns_arr)
    running_max = np.maximum.accumulate(cum)
    drawdowns = (cum - running_max) / (running_max + 1e-8)
    max_drawdown = float(drawdowns.min())

    positive = sum(1 for r in returns if r > 0)
    win_rate = positive / len(returns) if returns else 0.0

    return BacktestResponse(
        annualized_return=round(ann_return, 4),
        sharpe=round(sharpe, 4),
        max_drawdown=round(max_drawdown, 4),
        win_rate=round(win_rate, 4),
        trades=trades[:100],  # cap at 100 trades in response
    )
```

- [ ] **Step 5: 在 requirements.txt 添加 yfinance**

```
# append to services/lstm-service/requirements.txt
yfinance==0.2.43
```

- [ ] **Step 6: 实现 /models 端点（加到 train.py）**

在 `services/lstm-service/qlib_pipeline/train.py` 的 `router` 上追加：

```python
# append to services/lstm-service/qlib_pipeline/train.py (after existing router definitions)

class ModelInfo(BaseModel):
    model_id: str
    symbol: str
    model_type: str
    trained_at: str
    ic: float
    sharpe: float


@router.get("/models", response_model=list[ModelInfo])
async def list_models() -> list[ModelInfo]:
    results = []
    for pt in MODELS_DIR.glob("*.pt"):
        try:
            ckpt = torch.load(pt, map_location="cpu", weights_only=False)
            m = ckpt.get("metrics", {})
            date_str = ckpt.get("trained_at", "")[:10].replace("-", "")
            symbol = ckpt.get("symbol", pt.stem)
            mt = ckpt.get("model_type", "lstm")
            results.append(ModelInfo(
                model_id=f"lstm_{symbol.replace('.', '_')}_{mt}_{date_str}",
                symbol=symbol,
                model_type=mt,
                trained_at=ckpt.get("trained_at", ""),
                ic=m.get("ic", 0.0),
                sharpe=m.get("sharpe", 0.0),
            ))
        except Exception:
            continue
    return results
```

- [ ] **Step 7: 运行测试**

```bash
cd services/lstm-service
pip install yfinance
pytest tests/test_models_features.py -v
```

Expected: `test_get_models_lists_saved PASSED`, `test_features_no_data_returns_404 PASSED`

- [ ] **Step 8: Commit**

```bash
git add services/lstm-service/qlib_pipeline/backtest.py \
        services/lstm-service/qlib_pipeline/features.py \
        services/lstm-service/tests/test_models_features.py \
        services/lstm-service/requirements.txt \
        services/lstm-service/qlib_pipeline/train.py
git commit -m "feat(lstm-service): add /backtest, /models, /features endpoints"
```

---

## Task 5: DB — get_recent_bars + system_config + execution_profiles 读取

**Files:**
- Modify: `crates/db/src/bars.rs`
- Create: `crates/db/src/system_config.rs`
- Create: `crates/db/src/execution_profiles.rs`
- Modify: `crates/db/src/db.rs`

- [ ] **Step 1: 写失败测试（在 db crate）**

在 `crates/db/src/bars.rs` 末尾追加 test module（存在其他 tests 时在其后追加）：

```rust
// crates/db/src/bars.rs — append to existing #[cfg(test)] block or add new one

#[cfg(test)]
mod bars_tests {
    use super::*;
    use crate::Db;

    #[tokio::test]
    async fn get_recent_bars_returns_ordered_rows() {
        let db = Db::connect("sqlite::memory:").await.unwrap();
        crate::run_migrations(db.pool()).await.unwrap();
        // Insert instrument
        let iid = crate::upsert_instrument(db.pool(), "US_EQUITY", "AAPL").await.unwrap();
        // Insert 5 bars
        for i in 0..5_i64 {
            insert_bar(db.pool(), &NewBar {
                instrument_id: iid,
                data_source_id: "test",
                ts_ms: 1000 + i * 1000,
                open: 1.0, high: 2.0, low: 0.5, close: 1.5 + i as f64 * 0.1,
                volume: 100.0,
            }).await.unwrap();
        }
        let rows = get_recent_bars(db.pool(), iid, "test", 3).await.unwrap();
        assert_eq!(rows.len(), 3);
        // Should be oldest-first (ascending ts_ms)
        assert!(rows[0].ts_ms < rows[1].ts_ms);
        assert!(rows[1].ts_ms < rows[2].ts_ms);
    }
}
```

- [ ] **Step 2: 运行，确认编译失败（get_recent_bars 不存在）**

```bash
cd E:/code/trader
cargo test -p db get_recent_bars 2>&1 | head -20
```

Expected: `error[E0425]: cannot find function \`get_recent_bars\``

- [ ] **Step 3: 实现 get_recent_bars**

追加到 `crates/db/src/bars.rs`：

```rust
/// OHLCV bar row returned from DB.
pub struct BarRow {
    pub ts_ms: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

/// Return the most recent `limit` bars for an instrument/source, ordered oldest-first.
pub async fn get_recent_bars(
    pool: &SqlitePool,
    instrument_id: i64,
    data_source_id: &str,
    limit: i64,
) -> Result<Vec<BarRow>, DbError> {
    let rows = sqlx::query_as!(
        BarRow,
        r#"SELECT ts_ms, o as open, h as high, l as low, c as close, volume
           FROM (
             SELECT ts_ms, o, h, l, c, volume
             FROM bars
             WHERE instrument_id = ? AND data_source_id = ?
             ORDER BY ts_ms DESC
             LIMIT ?
           ) ORDER BY ts_ms ASC"#,
        instrument_id,
        data_source_id,
        limit,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
```

- [ ] **Step 4: 运行测试**

```bash
cargo test -p db bars_tests 2>&1 | tail -10
```

Expected: `test bars_tests::get_recent_bars_returns_ordered_rows ... ok`

- [ ] **Step 5: 实现 system_config.rs**

```rust
// crates/db/src/system_config.rs
use crate::error::DbError;
use sqlx::SqlitePool;

pub async fn get_system_config(pool: &SqlitePool, key: &str) -> Result<Option<String>, DbError> {
    let val = sqlx::query_scalar::<_, String>(
        "SELECT value FROM system_config WHERE key = ?",
    )
    .bind(key)
    .fetch_optional(pool)
    .await?;
    Ok(val)
}

pub async fn set_system_config(pool: &SqlitePool, key: &str, value: &str) -> Result<(), DbError> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    sqlx::query(
        "INSERT INTO system_config (id, key, value, updated_at, created_at)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(key)
    .bind(key)
    .bind(value)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}
```

- [ ] **Step 6: 实现 execution_profiles.rs**

```rust
// crates/db/src/execution_profiles.rs
use crate::error::DbError;
use sqlx::SqlitePool;

pub struct ExecutionProfileRow {
    pub id: String,
    pub kind: String,
    pub config_json: Option<String>,
}

pub async fn load_execution_profiles_by_kind(
    pool: &SqlitePool,
    kinds: &[&str],
) -> Result<Vec<ExecutionProfileRow>, DbError> {
    // sqlx doesn't support IN (?) with slices directly; use INSTR workaround or loop
    let mut results = Vec::new();
    for kind in kinds {
        let rows = sqlx::query_as!(
            ExecutionProfileRow,
            "SELECT id, kind, config_json FROM execution_profiles WHERE kind = ?",
            kind,
        )
        .fetch_all(pool)
        .await?;
        results.extend(rows);
    }
    Ok(results)
}

pub struct AccountRow {
    pub id: String,
    pub mode: String,
    pub execution_profile_id: String,
}

pub async fn load_accounts(pool: &SqlitePool) -> Result<Vec<AccountRow>, DbError> {
    let rows = sqlx::query_as!(
        AccountRow,
        "SELECT id, mode, execution_profile_id FROM accounts",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
```

- [ ] **Step 7: 在 bootstrap.rs 添加 ensure_longbridge_paper_account**

追加到 `crates/db/src/bootstrap.rs`：

```rust
/// 写入长桥 paper 账号凭证（凭证存 execution_profiles.config_json）。
/// 若 profile 已存在则更新 config_json；账号行 INSERT OR IGNORE。
pub async fn ensure_longbridge_paper_account(
    pool: &SqlitePool,
    app_key: &str,
    app_secret: &str,
    access_token: &str,
) -> Result<(), DbError> {
    let config_json = serde_json::json!({
        "app_key": app_key,
        "app_secret": app_secret,
        "access_token": access_token,
    })
    .to_string();

    sqlx::query(
        "INSERT INTO execution_profiles (id, kind, config_json) VALUES ('longbridge_paper', 'longbridge_paper', ?)
         ON CONFLICT(id) DO UPDATE SET config_json = excluded.config_json",
    )
    .bind(&config_json)
    .execute(pool)
    .await?;

    sqlx::query(
        "INSERT OR IGNORE INTO accounts (id, mode, execution_profile_id, venue)
         VALUES ('acc_lb_paper', 'paper', 'longbridge_paper', NULL)",
    )
    .execute(pool)
    .await?;

    Ok(())
}
```

- [ ] **Step 8: 更新 db.rs 导出**

在 `crates/db/src/db.rs` 的 `mod` 和 `pub use` 区域添加：

```rust
// 在 mod 区域添加（与其他 mod 并列）:
mod system_config;
mod execution_profiles;

// 在 pub use 区域添加:
pub use bars::get_recent_bars;  // 追加到已有 bars:: 导出行
pub use system_config::{get_system_config, set_system_config};
pub use execution_profiles::{
    load_accounts, load_execution_profiles_by_kind, AccountRow, ExecutionProfileRow,
};
pub use bootstrap::ensure_longbridge_paper_account;  // 追加到已有 bootstrap:: 导出行
```

- [ ] **Step 9: 编译确认**

```bash
cargo build -p db 2>&1 | tail -5
```

Expected: `Finished` 无错误

- [ ] **Step 10: Commit**

```bash
git add crates/db/src/bars.rs crates/db/src/system_config.rs \
        crates/db/src/execution_profiles.rs crates/db/src/bootstrap.rs \
        crates/db/src/db.rs
git commit -m "feat(db): add get_recent_bars, system_config, execution_profiles, paper account bootstrap"
```

---

## Task 6: Strategy trait 改为 async

**Files:**
- Modify: `crates/strategy/src/strategy.rs`
- Modify: `crates/pipeline/src/pipeline.rs`

- [ ] **Step 1: 运行现有策略测试确认通过（基线）**

```bash
cargo test -p strategy 2>&1 | tail -5
```

Expected: all tests pass

- [ ] **Step 2: 修改 Strategy trait 为 async**

`crates/strategy/src/strategy.rs` 全文替换为：

```rust
//! Trading strategies.

use async_trait::async_trait;
use domain::{InstrumentId, Side, Signal};

pub struct StrategyContext {
    pub instrument: InstrumentId,
    pub instrument_db_id: i64,
    pub last_bar_close: Option<f64>,
    pub ts_ms: i64,
}

/// `Send + Sync` so pipeline callers can hold `Arc<dyn Strategy>` across `await`.
#[async_trait]
pub trait Strategy: Send + Sync {
    async fn evaluate(&self, context: &StrategyContext) -> Option<Signal>;
}

pub struct NoOpStrategy;

#[async_trait]
impl Strategy for NoOpStrategy {
    async fn evaluate(&self, _context: &StrategyContext) -> Option<Signal> {
        None
    }
}

pub struct AlwaysLongOne;

#[async_trait]
impl Strategy for AlwaysLongOne {
    async fn evaluate(&self, context: &StrategyContext) -> Option<Signal> {
        let limit_price = context.last_bar_close?;
        Some(Signal {
            strategy_id: "always_long_one".to_string(),
            instrument: context.instrument.clone(),
            instrument_db_id: context.instrument_db_id,
            side: Side::Buy,
            qty: 1.0,
            limit_price,
            ts_ms: context.ts_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{AlwaysLongOne, Strategy, StrategyContext};
    use domain::{InstrumentId, Venue};

    #[tokio::test]
    async fn long_one_when_bar_present() {
        let strategy = AlwaysLongOne;
        let context = StrategyContext {
            instrument: InstrumentId::new(Venue::Crypto, "X"),
            instrument_db_id: 7,
            last_bar_close: Some(42.0),
            ts_ms: 99,
        };
        let signal = strategy.evaluate(&context).await.expect("signal");
        assert_eq!(signal.qty, 1.0);
        assert_eq!(signal.limit_price, 42.0);
    }

    #[tokio::test]
    async fn no_signal_without_bar() {
        let strategy = AlwaysLongOne;
        let context = StrategyContext {
            instrument: InstrumentId::new(Venue::Crypto, "X"),
            instrument_db_id: 7,
            last_bar_close: None,
            ts_ms: 99,
        };
        assert!(strategy.evaluate(&context).await.is_none());
    }
}
```

- [ ] **Step 3: 更新 pipeline.rs — evaluate 加 .await + Strategy variant**

在 `crates/pipeline/src/pipeline.rs` 中：

1. 在 `PipelineError` enum 添加变体（在 `RiskDenied` 后追加）：
```rust
    #[error("strategy error: {0}")]
    Strategy(String),
```

2. 将 `strategy.evaluate(&context)` 改为 `strategy.evaluate(&context).await`：
```rust
    // 将这行:
    let Some(signal) = strategy.evaluate(&context) else {
    // 改为:
    let Some(signal) = strategy.evaluate(&context).await else {
```

3. `run_one_tick_for_venue` 签名不变（已经是 async fn）。

- [ ] **Step 4: 运行测试**

```bash
cargo test -p strategy -p pipeline 2>&1 | tail -10
```

Expected: all tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/strategy/src/strategy.rs crates/pipeline/src/pipeline.rs
git commit -m "refactor(strategy): make Strategy trait async; add PipelineError::Strategy"
```

---

## Task 7: LstmStrategy Rust 实现

**Files:**
- Create: `crates/strategy/src/lstm.rs`
- Modify: `crates/strategy/src/lib.rs`
- Modify: `crates/strategy/Cargo.toml`

- [ ] **Step 1: 在 strategy Cargo.toml 添加依赖**

在 `crates/strategy/Cargo.toml` 的 `[dependencies]` 中追加：

```toml
reqwest = { workspace = true, features = ["json"] }
db.workspace = true
```

- [ ] **Step 2: 写 LstmStrategy 失败测试**

```rust
// crates/strategy/src/lstm.rs — write the test first
#[cfg(test)]
mod tests {
    use super::*;
    use domain::{InstrumentId, Venue};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn make_strategy(url: &str) -> LstmStrategy {
        LstmStrategy {
            client: reqwest::Client::new(),
            service_url: url.to_string(),
            model_type: "alstm".to_string(),
            lookback: 3,
            buy_threshold: 0.6,
            sell_threshold: -0.6,
            db: None,
            data_source_id: "test".to_string(),
        }
    }

    #[tokio::test]
    async fn buy_signal_when_score_above_threshold() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/predict"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "score": 0.75, "side": "buy", "confidence": 0.75
            })))
            .mount(&server)
            .await;

        let strategy = make_strategy(&server.uri()).await;
        let context = crate::strategy::StrategyContext {
            instrument: InstrumentId::new(Venue::UsEquity, "AAPL"),
            instrument_db_id: 1,
            last_bar_close: Some(180.0),
            ts_ms: 1_700_000_000_000,
        };
        let signal = strategy.evaluate(&context).await;
        assert!(signal.is_some());
        let s = signal.unwrap();
        assert_eq!(s.side, domain::Side::Buy);
    }

    #[tokio::test]
    async fn no_signal_when_hold() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/predict"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "score": 0.1, "side": "hold", "confidence": 0.1
            })))
            .mount(&server)
            .await;

        let strategy = make_strategy(&server.uri()).await;
        let context = crate::strategy::StrategyContext {
            instrument: InstrumentId::new(Venue::UsEquity, "AAPL"),
            instrument_db_id: 1,
            last_bar_close: Some(180.0),
            ts_ms: 1_700_000_000_000,
        };
        assert!(strategy.evaluate(&context).await.is_none());
    }

    #[tokio::test]
    async fn no_signal_when_service_unreachable() {
        let strategy = make_strategy("http://127.0.0.1:19999").await;
        let context = crate::strategy::StrategyContext {
            instrument: InstrumentId::new(Venue::UsEquity, "AAPL"),
            instrument_db_id: 1,
            last_bar_close: Some(180.0),
            ts_ms: 1_700_000_000_000,
        };
        // Service unreachable → None (no crash, no panic)
        assert!(strategy.evaluate(&context).await.is_none());
    }
}
```

- [ ] **Step 3: 在 strategy Cargo.toml 添加 dev-dependency**

```toml
[dev-dependencies]
wiremock = "0.6"
tokio = { workspace = true, features = ["macros", "rt-multi-thread"] }
```

- [ ] **Step 4: 运行测试，确认编译失败（LstmStrategy 不存在）**

```bash
cargo test -p strategy lstm 2>&1 | head -15
```

Expected: `error[E0412]: cannot find type \`LstmStrategy\``

- [ ] **Step 5: 实现 lstm.rs**

```rust
// crates/strategy/src/lstm.rs
use async_trait::async_trait;
use domain::{InstrumentId, Signal, Side};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::strategy::{Strategy, StrategyContext};

pub struct LstmStrategy {
    pub client: Client,
    pub service_url: String,
    pub model_type: String,
    pub lookback: i64,
    pub buy_threshold: f64,
    pub sell_threshold: f64,
    /// DB handle for reading recent bars. None → use only last_bar_close (fallback).
    pub db: Option<db::Db>,
    pub data_source_id: String,
}

impl LstmStrategy {
    pub fn new(
        service_url: String,
        model_type: String,
        lookback: i64,
        buy_threshold: f64,
        sell_threshold: f64,
        db: db::Db,
        data_source_id: String,
    ) -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(3))
                .build()
                .unwrap_or_default(),
            service_url,
            model_type,
            lookback,
            buy_threshold,
            sell_threshold,
            db: Some(db),
            data_source_id,
        }
    }
}

#[derive(Serialize)]
struct BarPayload {
    ts_ms: i64,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
}

#[derive(Serialize)]
struct PredictRequest<'a> {
    symbol: &'a str,
    model_type: &'a str,
    bars: Vec<BarPayload>,
}

#[derive(Deserialize)]
struct PredictResponse {
    score: f64,
    side: String,
    #[allow(dead_code)]
    confidence: f64,
}

#[async_trait]
impl Strategy for LstmStrategy {
    async fn evaluate(&self, context: &StrategyContext) -> Option<Signal> {
        let bars: Vec<BarPayload> = if let Some(ref db) = self.db {
            match db::get_recent_bars(db.pool(), context.instrument_db_id, &self.data_source_id, self.lookback).await {
                Ok(rows) if rows.len() >= self.lookback as usize => rows
                    .into_iter()
                    .map(|r| BarPayload {
                        ts_ms: r.ts_ms,
                        open: r.open,
                        high: r.high,
                        low: r.low,
                        close: r.close,
                        volume: r.volume,
                    })
                    .collect(),
                Ok(_) => {
                    tracing::warn!(
                        channel = "lstm_strategy",
                        instrument = %context.instrument,
                        "insufficient bars for LSTM lookback; skipping"
                    );
                    return None;
                }
                Err(e) => {
                    tracing::error!(channel = "lstm_strategy", err = %e, "db error reading bars");
                    return None;
                }
            }
        } else {
            // Fallback: single bar from last_bar_close (for tests without DB)
            let close = context.last_bar_close?;
            (0..self.lookback)
                .map(|i| BarPayload {
                    ts_ms: context.ts_ms - (self.lookback - i) * 86_400_000,
                    open: close,
                    high: close,
                    low: close,
                    close,
                    volume: 0.0,
                })
                .collect()
        };

        let symbol = context.instrument.symbol.as_str();
        let req = PredictRequest {
            symbol,
            model_type: &self.model_type,
            bars,
        };

        let url = format!("{}/predict", self.service_url);
        let resp = match self.client.post(&url).json(&req).send().await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(channel = "lstm_strategy", err = %e, "lstm-service unreachable");
                return None;
            }
        };

        if !resp.status().is_success() {
            tracing::warn!(
                channel = "lstm_strategy",
                status = %resp.status(),
                "lstm-service returned error"
            );
            return None;
        }

        let pred: PredictResponse = match resp.json().await {
            Ok(p) => p,
            Err(e) => {
                tracing::error!(channel = "lstm_strategy", err = %e, "failed to parse predict response");
                return None;
            }
        };

        let limit_price = context.last_bar_close?;

        if pred.score > self.buy_threshold {
            Some(Signal {
                strategy_id: format!("lstm_{}", self.model_type),
                instrument: context.instrument.clone(),
                instrument_db_id: context.instrument_db_id,
                side: Side::Buy,
                qty: 1.0,
                limit_price,
                ts_ms: context.ts_ms,
            })
        } else if pred.score < self.sell_threshold {
            Some(Signal {
                strategy_id: format!("lstm_{}", self.model_type),
                instrument: context.instrument.clone(),
                instrument_db_id: context.instrument_db_id,
                side: Side::Sell,
                qty: 1.0,
                limit_price,
                ts_ms: context.ts_ms,
            })
        } else {
            None
        }
    }
}
```

- [ ] **Step 6: 在 lib.rs 导出 LstmStrategy**

追加到 `crates/strategy/src/lib.rs`：

```rust
pub mod lstm;
pub use lstm::LstmStrategy;
```

- [ ] **Step 7: 运行测试**

```bash
cargo test -p strategy lstm 2>&1 | tail -10
```

Expected:
```
test lstm::tests::buy_signal_when_score_above_threshold ... ok
test lstm::tests::no_signal_when_hold ... ok
test lstm::tests::no_signal_when_service_unreachable ... ok
```

- [ ] **Step 8: Commit**

```bash
git add crates/strategy/src/lstm.rs crates/strategy/src/lib.rs crates/strategy/Cargo.toml
git commit -m "feat(strategy): add async LstmStrategy with HTTP predict call"
```

---

## Task 8: LongbridgeClients — 支持显式凭证构建

**Files:**
- Modify: `crates/longbridge_adapters/src/clients.rs`

- [ ] **Step 1: 实现 connect_with_credentials**

在 `crates/longbridge_adapters/src/clients.rs` 中追加 `connect_with_credentials` 函数：

```rust
impl LongbridgeClients {
    /// 使用显式凭证（而非环境变量）构建客户端，用于从 DB 读取凭证的场景。
    pub fn connect_with_credentials(
        app_key: &str,
        app_secret: &str,
        access_token: &str,
    ) -> Result<Self, String> {
        let config = Arc::new(
            Config::new()
                .app_key(app_key)
                .app_secret(app_secret)
                .access_token(access_token)
                .build()
                .map_err(|e| e.to_string())?
        );
        let (quote, mut q_rx) = QuoteContext::new(config.clone());
        let (trade, mut t_rx) = TradeContext::new(config);
        tokio::spawn(async move { while q_rx.recv().await.is_some() {} });
        tokio::spawn(async move { while t_rx.recv().await.is_some() {} });
        Ok(Self {
            quote: Arc::new(quote),
            trade: Arc::new(trade),
        })
    }
}
```

- [ ] **Step 2: 编译确认**

```bash
cargo build -p longbridge_adapters 2>&1 | tail -5
```

Expected: `Finished` 无错误。若 `Config::new().app_key(...)` API 不匹配，检查 `longbridge` crate 文档并调整为正确的 builder 方法。实际 longbridge SDK Config builder 方法参考：`Config::builder().app_key(k).app_secret(s).access_token(t).build()`。

- [ ] **Step 3: Commit**

```bash
git add crates/longbridge_adapters/src/clients.rs
git commit -m "feat(longbridge_adapters): add connect_with_credentials for DB-sourced credentials"
```

---

## Task 9: quantd main — 从 DB 动态构建 ExecutionRouter + LstmStrategy

**Files:**
- Modify: `crates/quantd/src/main.rs`

- [ ] **Step 1: 阅读现有 main.rs 的完整构建逻辑**

确认当前 `main.rs` 中 `routes` HashMap 和 `strategy` 的构建位置（参见本 plan 索引：`crates/quantd/src/main.rs`）。

- [ ] **Step 2: 替换 build_execution_router 为从 DB 读取**

在 `crates/quantd/src/main.rs` 中，将现有的手动 `routes.insert` 块替换为以下函数调用：

```rust
// 在 main() 中，database 初始化之后，替换现有 routes 构建块：

let execution_router = build_execution_router_from_db(&database, lb_clients.as_ref()).await;
```

新增函数（在 `main()` 之外）：

```rust
async fn build_execution_router_from_db(
    database: &db::Db,
    env_lb: Option<&LongbridgeClients>,
) -> ExecutionRouter {
    use std::collections::HashMap;
    let mut routes: HashMap<String, Arc<dyn ExecutionAdapter>> = HashMap::new();

    // 1. 始终注册本地 paper
    let paper = Arc::new(PaperAdapter::new(database.clone()));
    routes.insert("acc_mvp_paper".to_string(), paper as Arc<dyn ExecutionAdapter>);

    // 2. 从 DB 读取 longbridge execution profiles（live + paper）
    let profiles = db::load_execution_profiles_by_kind(
        database.pool(),
        &["longbridge_live", "longbridge_paper"],
    )
    .await
    .unwrap_or_default();

    // 3. 从 DB 读取所有账号
    let accounts = db::load_accounts(database.pool()).await.unwrap_or_default();

    // 4. 对每个 longbridge profile，尝试从 config_json 读取凭证
    for profile in &profiles {
        let Some(ref config_json) = profile.config_json else { continue };
        let creds: serde_json::Value = match serde_json::from_str(config_json) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(channel = "quantd", profile_id = %profile.id, err = %e, "invalid config_json");
                continue;
            }
        };
        let app_key = creds["app_key"].as_str().unwrap_or("").to_string();
        let app_secret = creds["app_secret"].as_str().unwrap_or("").to_string();
        let access_token = creds["access_token"].as_str().unwrap_or("").to_string();

        if app_key.is_empty() || app_secret.is_empty() || access_token.is_empty() {
            // Fall back to env vars for longbridge_live
            if profile.kind == "longbridge_live" {
                if let Some(lb) = env_lb {
                    for acc in accounts.iter().filter(|a| a.execution_profile_id == profile.id) {
                        routes.insert(
                            acc.id.clone(),
                            Arc::new(LongbridgeTradeAdapter::new(lb.trade.clone()))
                                as Arc<dyn ExecutionAdapter>,
                        );
                        tracing::info!(channel = "quantd", account_id = %acc.id, "registered (env creds)");
                    }
                }
            }
            continue;
        }

        match LongbridgeClients::connect_with_credentials(&app_key, &app_secret, &access_token) {
            Ok(lb) => {
                for acc in accounts.iter().filter(|a| a.execution_profile_id == profile.id) {
                    routes.insert(
                        acc.id.clone(),
                        Arc::new(LongbridgeTradeAdapter::new(lb.trade.clone()))
                            as Arc<dyn ExecutionAdapter>,
                    );
                    tracing::info!(channel = "quantd", account_id = %acc.id, profile_id = %profile.id, "registered (db creds)");
                }
            }
            Err(e) => {
                tracing::warn!(channel = "quantd", profile_id = %profile.id, err = %e, "failed to connect with db creds");
            }
        }
    }

    // 5. 如果没有从 DB 读到 longbridge_live 账号，回退到 env
    if !routes.contains_key("acc_lb_live") {
        if let Some(lb) = env_lb {
            routes.insert(
                "acc_lb_live".to_string(),
                Arc::new(LongbridgeTradeAdapter::new(lb.trade.clone())) as Arc<dyn ExecutionAdapter>,
            );
            tracing::info!(channel = "quantd", account_id = "acc_lb_live", "registered (env fallback)");
        }
    }

    ExecutionRouter::new(routes)
}
```

- [ ] **Step 3: 替换 live_strategy_from_env 支持 lstm**

将现有 `live_strategy_from_env` 函数替换为：

```rust
async fn build_strategy(database: &db::Db) -> Arc<dyn Strategy> {
    // 1. 尝试从 system_config 读取策略配置
    if let Ok(Some(cfg_json)) = db::get_system_config(database.pool(), &format!(
        "strategy.{}",
        std::env::var("QUANTD_ACCOUNT_ID").unwrap_or_else(|_| "acc_lb_paper".to_string())
    )).await {
        if let Ok(cfg) = serde_json::from_str::<serde_json::Value>(&cfg_json) {
            if cfg["type"].as_str() == Some("lstm") {
                let service_url = db::get_system_config(database.pool(), "lstm.service_url")
                    .await
                    .unwrap_or_default()
                    .unwrap_or_else(|| "http://127.0.0.1:8000".to_string());
                let model_type = cfg["model_type"].as_str().unwrap_or("alstm").to_string();
                let lookback = cfg["lookback"].as_i64().unwrap_or(60);
                let buy_threshold = cfg["buy_threshold"].as_f64().unwrap_or(0.6);
                let sell_threshold = cfg["sell_threshold"].as_f64().unwrap_or(-0.6);
                let data_source_id = std::env::var("QUANTD_DATA_SOURCE_ID")
                    .unwrap_or_else(|_| "longbridge".to_string());
                tracing::info!(channel = "quantd", strategy = "lstm", model_type = %model_type, "loaded from system_config");
                return Arc::new(strategy::LstmStrategy::new(
                    service_url, model_type, lookback,
                    buy_threshold, sell_threshold,
                    database.clone(), data_source_id,
                ));
            }
        }
    }

    // 2. 回退到环境变量
    match std::env::var("QUANTD_STRATEGY")
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default()
        .as_str()
    {
        "always_long_one" | "mvp" => {
            tracing::info!(channel = "quantd", strategy = "always_long_one");
            Arc::new(strategy::AlwaysLongOne)
        }
        _ => {
            tracing::info!(channel = "quantd", strategy = "noop");
            Arc::new(strategy::NoOpStrategy)
        }
    }
}
```

在 `main()` 中将 `let strategy = live_strategy_from_env();` 替换为：

```rust
let strategy = build_strategy(&database).await;
```

- [ ] **Step 4: 编译确认**

```bash
cargo build -p quantd 2>&1 | tail -10
```

Expected: `Finished` 无错误。修复任何 import 错误（添加 `use strategy::LstmStrategy;` 等）。

- [ ] **Step 5: Commit**

```bash
git add crates/quantd/src/main.rs
git commit -m "feat(quantd): build ExecutionRouter from DB creds; LstmStrategy from system_config"
```

---

## Task 10: API — strategy config GET/PUT 端点

**Files:**
- Modify: `crates/api/src/api.rs`
- Modify: `crates/api/src/handlers.rs`

- [ ] **Step 1: 在 handlers.rs 添加 strategy config handlers**

追加到 `crates/api/src/handlers.rs`：

```rust
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::api::AppState;
use crate::error::ApiError;

#[derive(Serialize)]
pub struct StrategyConfigBody {
    pub account_id: String,
    pub config: serde_json::Value,
}

#[derive(Deserialize)]
pub struct StrategyConfigUpdate {
    pub account_id: String,
    pub config: serde_json::Value,
}

pub async fn get_strategy_config(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<StrategyConfigBody>>, ApiError> {
    // Return all strategy.* keys from system_config
    let pool = state.database.pool();
    let rows = sqlx::query!(
        "SELECT key, value FROM system_config WHERE key LIKE 'strategy.%'",
    )
    .fetch_all(pool)
    .await
    .map_err(ApiError::internal)?;

    let configs: Vec<StrategyConfigBody> = rows
        .into_iter()
        .filter_map(|r| {
            let account_id = r.key.strip_prefix("strategy.")?.to_string();
            let config: serde_json::Value = serde_json::from_str(&r.value.unwrap_or_default()).ok()?;
            Some(StrategyConfigBody { account_id, config })
        })
        .collect();

    Ok(Json(configs))
}

pub async fn put_strategy_config(
    State(state): State<Arc<AppState>>,
    Json(body): Json<StrategyConfigUpdate>,
) -> Result<StatusCode, ApiError> {
    let key = format!("strategy.{}", body.account_id);
    let value = serde_json::to_string(&body.config).map_err(ApiError::internal)?;
    db::set_system_config(state.database.pool(), &key, &value)
        .await
        .map_err(ApiError::internal)?;
    Ok(StatusCode::NO_CONTENT)
}
```

- [ ] **Step 2: 在 api.rs 注册路由**

在 `crates/api/src/api.rs` 的 `v1` Router 中追加两条路由（在 `route("/stream", ...)` 之后）：

```rust
.route("/strategy/config", get(handlers::get_strategy_config))
.route("/strategy/config", put(handlers::put_strategy_config))
```

同时在 `pub use handlers::{...}` 行添加新 handler 导出：

```rust
pub use handlers::{OrdersQuery, TickBody, TickResponse, StrategyConfigBody, StrategyConfigUpdate};
```

- [ ] **Step 3: 编译确认**

```bash
cargo build -p api 2>&1 | tail -5
```

Expected: `Finished`

- [ ] **Step 4: Commit**

```bash
git add crates/api/src/api.rs crates/api/src/handlers.rs
git commit -m "feat(api): add GET/PUT /v1/strategy/config endpoints"
```

---

## Task 11: 端到端冒烟测试

**目标：** 启动 lstm-service + quantd，配置 paper 账号，执行一次 tick，验证信号流转到长桥 paper 账号。

- [ ] **Step 1: 训练一个 AAPL ALSTM 模型**

```bash
cd services/lstm-service
uvicorn main:app --port 8000 &

curl -X POST http://localhost:8000/train \
  -H "Content-Type: application/json" \
  -d '{"symbol":"AAPL.US","model_type":"alstm","start":"2022-01-01","end":"2024-12-31"}'
```

Expected: `{"model_id":"lstm_AAPL_US_alstm_...","metrics":{...}}`

- [ ] **Step 2: 确认 /predict 工作**

```bash
# 用 /models 拿到最近 60 根 K 线（这里用手动构造）
python3 -c "
import json, time
bars = [{'ts_ms': int(time.time()*1000) - (60-i)*86400000,
         'open':180+i*0.1,'high':182,'low':179,'close':181+i*0.05,'volume':5e7}
        for i in range(60)]
print(json.dumps({'symbol':'AAPL.US','model_type':'alstm','bars':bars}))
" | curl -X POST http://localhost:8000/predict -H "Content-Type: application/json" -d @-
```

Expected: `{"score":...,"side":"buy"|"sell"|"hold","confidence":...}`

- [ ] **Step 3: 写入长桥 paper 账号凭证到 DB**

```bash
# 假设已有长桥 paper 账号凭证，用 SQLite CLI 写入
sqlite3 quantd.db "
INSERT OR REPLACE INTO execution_profiles (id, kind, config_json)
VALUES ('longbridge_paper', 'longbridge_paper',
        '{\"app_key\":\"YOUR_PAPER_KEY\",\"app_secret\":\"YOUR_PAPER_SECRET\",\"access_token\":\"YOUR_PAPER_TOKEN\"}');
INSERT OR IGNORE INTO accounts (id, mode, execution_profile_id, venue)
VALUES ('acc_lb_paper', 'paper', 'longbridge_paper', NULL);
"
```

- [ ] **Step 4: 写入策略配置到 DB**

```bash
sqlite3 quantd.db "
INSERT OR REPLACE INTO system_config (id, key, value, updated_at, created_at)
VALUES ('lstm.service_url', 'lstm.service_url', 'http://127.0.0.1:8000',
        strftime('%s','now'), strftime('%s','now'));
INSERT OR REPLACE INTO system_config (id, key, value, updated_at, created_at)
VALUES ('strategy.acc_lb_paper', 'strategy.acc_lb_paper',
        '{\"type\":\"lstm\",\"model_type\":\"alstm\",\"symbol\":\"AAPL.US\",\"lookback\":60,\"buy_threshold\":0.6,\"sell_threshold\":-0.6}',
        strftime('%s','now'), strftime('%s','now'));
"
```

- [ ] **Step 5: 启动 quantd**

```bash
QUANTD_ACCOUNT_ID=acc_lb_paper \
LONGBRIDGE_APP_KEY=YOUR_LIVE_KEY \
LONGBRIDGE_APP_SECRET=YOUR_LIVE_SECRET \
LONGBRIDGE_ACCESS_TOKEN=YOUR_LIVE_TOKEN \
cargo run -p quantd
```

Expected 日志（tracing output）：
```
registered (db creds) account_id=acc_lb_paper
loaded from system_config strategy=lstm model_type=alstm
http listening addr=127.0.0.1:8080
```

- [ ] **Step 6: 触发一次 tick**

```bash
curl -X POST http://localhost:8080/v1/tick \
  -H "Content-Type: application/json" \
  -d '{"venue":"US_EQUITY","symbol":"AAPL.US","account_id":"acc_lb_paper"}'
```

Expected: `{"ok":true,"venue":"US_EQUITY","symbol":"AAPL.US"}`

日志中应出现：
- `ingest_once start venue=US_EQUITY`
- `POST /predict` 请求发往 lstm-service
- 若信号产生：`order placed account_id=acc_lb_paper order_id=<longbridge_order_id>`

- [ ] **Step 7: 在长桥 app 确认委托**

登录长桥 app → 模拟交易 → 委托记录，确认 AAPL 限价单已出现。

- [ ] **Step 8: 最终 commit**

```bash
git add .
git commit -m "feat: LSTM strategy + Longbridge paper account end-to-end pipeline complete"
```

---

## 自检

**Spec coverage:**
- §3 lstm-service：Task 1-4 覆盖全部 6 个端点 ✓
- §4 async Strategy trait：Task 6 ✓
- §4.2 LstmStrategy：Task 7 ✓
- §5 长桥 paper 账号：Task 8-9 ✓
- §6 策略配置持久化：Task 9 (quantd main) + Task 10 (API) ✓
- §7 错误处理：PipelineError::Strategy (Task 6)，service unreachable → None (Task 7) ✓
- §8 测试：Task 1-7 各含测试，Task 11 端到端 ✓

**Type consistency:**
- `BarRow` (db) → `BarPayload` (lstm.rs) ✓
- `LstmStrategy::new` 签名与 Task 9 调用一致 ✓
- `get_recent_bars` 签名在 Task 5 定义，Task 7 调用一致 ✓
- `connect_with_credentials` 在 Task 8 定义，Task 9 调用一致 ✓
