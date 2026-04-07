# services/lstm-service/qlib_pipeline/backtest.py
from __future__ import annotations

import os
from pathlib import Path
from typing import List, Optional

import numpy as np
import torch
from fastapi import APIRouter, HTTPException
from pydantic import BaseModel, ConfigDict

from .predict import _model_path, _bars_to_features, _load_model, LOOKBACK, Bar

MODELS_DIR = Path(os.getenv("LSTM_MODELS_DIR", "models"))

router = APIRouter()


class BacktestRequest(BaseModel):
    model_config = ConfigDict(protected_namespaces=())
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
        raise HTTPException(status_code=404, detail=f"No model for {req.symbol}/{req.model_type}. Train first.")

    checkpoint = torch.load(path, map_location="cpu", weights_only=False)
    model = _load_model(path, checkpoint)

    try:
        import yfinance as yf
        ticker = req.symbol.split(".")[0]
        df = yf.download(ticker, start=req.start, end=req.end, auto_adjust=True, progress=False)
        if df.empty:
            raise HTTPException(status_code=422, detail=f"No price data for {req.symbol}")
    except ImportError:
        raise HTTPException(status_code=500, detail="yfinance not installed; run: pip install yfinance")

    # Handle multi-level columns from yfinance (ticker as second level)
    if isinstance(df.columns, tuple) or (hasattr(df.columns, 'nlevels') and df.columns.nlevels > 1):
        df.columns = df.columns.get_level_values(0)

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
                                       score=score, price=float(closes[i])))
        elif score < -0.6 and position == 1.0:
            position = 0.0
            trades.append(TradeSummary(ts_ms=timestamps[i], side="sell",
                                       score=score, price=float(closes[i])))

        returns.append(position * day_return)

    returns_arr = np.array(returns) if returns else np.array([0.0])
    ann_return = float(returns_arr.mean() * 252)
    sharpe = float(returns_arr.mean() / (returns_arr.std() + 1e-8) * np.sqrt(252))

    cum = np.cumprod(1 + returns_arr)
    running_max = np.maximum.accumulate(cum)
    drawdowns = (cum - running_max) / (running_max + 1e-8)
    max_drawdown = float(drawdowns.min())

    positive = sum(1 for r in returns if r > 0)
    win_rate = float(positive / len(returns)) if returns else 0.0

    return BacktestResponse(
        annualized_return=round(ann_return, 4),
        sharpe=round(sharpe, 4),
        max_drawdown=round(max_drawdown, 4),
        win_rate=round(win_rate, 4),
        trades=trades[:100],
    )
