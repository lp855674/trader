from __future__ import annotations

import torch
from fastapi import APIRouter, HTTPException
from pydantic import BaseModel, ConfigDict, field_validator

from runtime.loader import find_model_for_request, load_models
from runtime.networks import LOOKBACK, bars_to_features, load_model
from workflow.shared import validate_name

router = APIRouter()


class Bar(BaseModel):
    ts_ms: int
    open: float
    high: float
    low: float
    close: float
    volume: float


class PredictRequest(BaseModel):
    model_config = ConfigDict(protected_namespaces=())

    symbol: str
    model_type: str = "alstm"
    bars: list[Bar]

    @field_validator("bars")
    @classmethod
    def check_bars_length(cls, bars):
        if len(bars) < LOOKBACK:
            raise ValueError(f"bars must have at least {LOOKBACK} entries, got {len(bars)}")
        return bars


class PredictResponse(BaseModel):
    score: float
    side: str
    confidence: float


@router.post("/predict", response_model=PredictResponse)
async def predict(req: PredictRequest) -> PredictResponse:
    symbol = validate_name(req.symbol, "symbol")
    model_type = validate_name(req.model_type, "model_type")
    loaded = find_model_for_request(load_models(), symbol, model_type)
    if loaded is None:
        raise HTTPException(
            status_code=404,
            detail={
                "error_code": "model_not_found",
                "message": f"No model found for {symbol}/{model_type}. Train first.",
            },
        )

    model = load_model(loaded.checkpoint)
    features = bars_to_features(req.bars[-LOOKBACK:])
    x = torch.tensor(features).unsqueeze(0)
    with torch.no_grad():
        raw_score = model(x).item()
    score = max(-1.0, min(1.0, raw_score))
    confidence = abs(score)

    if score > 0.6:
        side = "buy"
    elif score < -0.6:
        side = "sell"
    else:
        side = "hold"

    return PredictResponse(score=score, side=side, confidence=confidence)
