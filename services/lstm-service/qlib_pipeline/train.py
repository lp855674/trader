# services/lstm-service/qlib_pipeline/train.py
from __future__ import annotations

import os
from datetime import datetime
from pathlib import Path

import numpy as np
import torch
import torch.nn as nn
from fastapi import APIRouter, HTTPException
from pydantic import BaseModel, ConfigDict, field_validator

MODELS_DIR = Path(os.getenv("LSTM_MODELS_DIR", "models"))
MODELS_DIR.mkdir(parents=True, exist_ok=True)

SUPPORTED_MODELS = {"lstm", "alstm"}

router = APIRouter()


class TrainRequest(BaseModel):
    model_config = ConfigDict(protected_namespaces=())

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
    model_config = ConfigDict(protected_namespaces=())

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

    # Simple training loop (20 epochs)
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


class ModelInfo(BaseModel):
    model_config = ConfigDict(protected_namespaces=())
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
