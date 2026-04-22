from __future__ import annotations

import numpy as np
import torch
from fastapi import APIRouter, HTTPException
from pydantic import BaseModel, ConfigDict

from runtime.loader import find_model_for_request, load_models
from runtime.networks import LOOKBACK, bars_to_features, load_model
from workflow.predict import Bar
from workflow.shared import validate_name

router = APIRouter()


class BacktestRequest(BaseModel):
    model_config = ConfigDict(protected_namespaces=())

    symbol: str
    start: str
    end: str
    model_id: str | None = None
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
    trades: list[TradeSummary]


@router.post("/backtest", response_model=BacktestResponse)
async def backtest(req: BacktestRequest) -> BacktestResponse:
    symbol = validate_name(req.symbol, "symbol")
    model_type = validate_name(req.model_type, "model_type")
    loaded = find_model_for_request(load_models(), symbol, model_type)
    if loaded is None:
        raise HTTPException(status_code=404, detail=f"No model for {symbol}/{model_type}. Train first.")

    model = load_model(loaded.checkpoint)

    try:
        import yfinance as yf
    except ImportError as exc:
        raise HTTPException(status_code=500, detail="yfinance not installed; run: pip install yfinance") from exc

    ticker = symbol.split(".")[0]
    data_frame = yf.download(ticker, start=req.start, end=req.end, auto_adjust=True, progress=False)
    if data_frame.empty:
        raise HTTPException(status_code=422, detail=f"No price data for {symbol}")

    if isinstance(data_frame.columns, tuple) or (
        hasattr(data_frame.columns, "nlevels") and data_frame.columns.nlevels > 1
    ):
        data_frame.columns = data_frame.columns.get_level_values(0)

    closes = data_frame["Close"].values.astype(float)
    highs = data_frame["High"].values.astype(float)
    lows = data_frame["Low"].values.astype(float)
    opens = data_frame["Open"].values.astype(float)
    volumes = data_frame["Volume"].values.astype(float)
    timestamps = [int(ts.timestamp() * 1000) for ts in data_frame.index]

    trades = []
    returns = []
    position = 0.0

    for index in range(LOOKBACK, len(closes)):
        bars = [
            Bar(
                ts_ms=timestamps[past_index],
                open=opens[past_index],
                high=highs[past_index],
                low=lows[past_index],
                close=closes[past_index],
                volume=volumes[past_index],
            )
            for past_index in range(index - LOOKBACK, index)
        ]
        features = bars_to_features(bars)
        x = torch.tensor(features).unsqueeze(0)
        with torch.no_grad():
            raw_score = model(x).item()
        score = max(-1.0, min(1.0, raw_score))
        day_return = (closes[index] - closes[index - 1]) / (closes[index - 1] + 1e-8)

        if score > 0.6 and position == 0.0:
            position = 1.0
            trades.append(
                TradeSummary(ts_ms=timestamps[index], side="buy", score=score, price=float(closes[index]))
            )
        elif score < -0.6 and position == 1.0:
            position = 0.0
            trades.append(
                TradeSummary(ts_ms=timestamps[index], side="sell", score=score, price=float(closes[index]))
            )

        returns.append(position * day_return)

    returns_arr = np.array(returns) if returns else np.array([0.0])
    annualized_return = float(returns_arr.mean() * 252)
    sharpe = float(returns_arr.mean() / (returns_arr.std() + 1e-8) * np.sqrt(252))

    cumulative = np.cumprod(1 + returns_arr)
    running_max = np.maximum.accumulate(cumulative)
    drawdowns = (cumulative - running_max) / (running_max + 1e-8)
    max_drawdown = float(drawdowns.min())

    positive = sum(1 for value in returns if value > 0)
    win_rate = float(positive / len(returns)) if returns else 0.0

    return BacktestResponse(
        annualized_return=round(annualized_return, 4),
        sharpe=round(sharpe, 4),
        max_drawdown=round(max_drawdown, 4),
        win_rate=round(win_rate, 4),
        trades=trades[:100],
    )
