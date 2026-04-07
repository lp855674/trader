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
    model.load_state_dict(checkpoint["model_state"], strict=False)
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
