from __future__ import annotations

from datetime import datetime, timezone

import numpy as np
import torch
import torch.nn as nn
from fastapi import APIRouter, HTTPException
from pydantic import BaseModel, ConfigDict, field_validator

from runtime.networks import LOOKBACK, build_model
from workflow.shared import SUPPORTED_MODELS, list_loaded_models, validate_name, write_artifact

router = APIRouter()


class TrainRequest(BaseModel):
    model_config = ConfigDict(protected_namespaces=())

    symbol: str
    model_type: str = "alstm"
    start: str = "2020-01-01"
    end: str = "2024-12-31"

    @field_validator("model_type")
    @classmethod
    def check_model_type(cls, model_type):
        if model_type not in SUPPORTED_MODELS:
            raise ValueError(f"model_type must be one of {sorted(SUPPORTED_MODELS)}, got '{model_type}'")
        return model_type


class TrainMetrics(BaseModel):
    ic: float
    icir: float
    sharpe: float
    annualized_return: float


class TrainResponse(BaseModel):
    model_config = ConfigDict(protected_namespaces=())

    model_id: str
    metrics: TrainMetrics


class ModelInfo(BaseModel):
    model_config = ConfigDict(protected_namespaces=())

    model_id: str
    model_type: str
    source_kind: str
    symbols: list[str]


def fetch_and_train(symbol: str, model_type: str, start: str, end: str):
    validated_symbol = validate_name(symbol, "symbol")
    validate_name(model_type, "model_type")
    try:
        import qlib
        from qlib.constant import REG_US
        from qlib.contrib.data.handler import Alpha158
    except ImportError as exc:
        raise HTTPException(status_code=500, detail="qlib not installed") from exc

    from runtime.loader import get_models_dir
    qlib_dir = get_models_dir().parent / ".qlib"
    qlib_dir = qlib_dir if qlib_dir.exists() else None
    provider_uri = str(qlib_dir) if qlib_dir else None
    if provider_uri is None:
        import os
        from pathlib import Path

        provider_uri = str(Path(os.getenv("QLIB_DATA_DIR", "~/.qlib/qlib_data/us_data")).expanduser())
    qlib.init(provider_uri=provider_uri, region=REG_US)

    handler = Alpha158(
        instruments="all",
        start_time=start,
        end_time=end,
        fit_start_time=start,
        fit_end_time=end,
        infer_processors=[],
        learn_processors=[],
    )
    data_frame = handler.fetch()

    qlib_symbol = validated_symbol.split(".")[0]
    try:
        data_frame = data_frame.xs(qlib_symbol, level="instrument")
    except KeyError as exc:
        raise HTTPException(
            status_code=422,
            detail=f"No data returned for {symbol} in {start}~{end}",
        ) from exc

    if data_frame.empty:
        raise HTTPException(status_code=422, detail=f"No data returned for {symbol} in {start}~{end}")

    features = data_frame.drop(columns=["label"], errors="ignore").values.astype(np.float32)
    labels = (
        data_frame["label"].values.astype(np.float32)
        if "label" in data_frame.columns
        else np.zeros(len(data_frame), dtype=np.float32)
    )
    if features.shape[1] > 158:
        features = features[:, :158]

    windows = []
    targets = []
    for index in range(LOOKBACK, len(features)):
        windows.append(features[index - LOOKBACK:index])
        targets.append(labels[index])
    if len(windows) < 10:
        raise HTTPException(status_code=422, detail="Insufficient data after windowing")

    x_tensor = torch.tensor(np.array(windows))
    y_tensor = torch.tensor(np.array(targets))

    model = build_model(model_type)
    optimizer = torch.optim.Adam(model.parameters(), lr=1e-3)
    criterion = nn.MSELoss()

    model.train()
    for _ in range(20):
        optimizer.zero_grad()
        pred = model(x_tensor)
        loss = criterion(pred, y_tensor)
        loss.backward()
        optimizer.step()

    model.eval()
    with torch.no_grad():
        predictions = model(x_tensor).numpy()
    labels_np = y_tensor.numpy()
    ic = float(np.corrcoef(predictions, labels_np)[0, 1]) if labels_np.std() > 0 else 0.0
    icir = ic / (predictions.std() + 1e-8)
    returns = predictions * labels_np
    sharpe = float(returns.mean() / (returns.std() + 1e-8) * np.sqrt(252))
    annualized_return = float(returns.mean() * 252)

    metrics = {
        "ic": round(ic, 4),
        "icir": round(icir, 4),
        "sharpe": round(sharpe, 4),
        "annualized_return": round(annualized_return, 4),
    }
    checkpoint = {
        "model_state": model.state_dict(),
        "model_type": model_type,
        "input_size": 158,
        "hidden_size": 64,
        "num_layers": 2,
        "lookback": LOOKBACK,
        "symbol": symbol,
        "trained_at": datetime.now(timezone.utc).isoformat(),
        "metrics": metrics,
    }
    return checkpoint, metrics


@router.post("/train", response_model=TrainResponse)
async def train(req: TrainRequest) -> TrainResponse:
    checkpoint, metrics = fetch_and_train(req.symbol, req.model_type, req.start, req.end)
    model_id = write_artifact(
        symbol=req.symbol,
        model_type=req.model_type,
        checkpoint=checkpoint,
        feature_set="Alpha158",
        prediction_semantics="score in [-1,1]",
    )
    return TrainResponse(model_id=model_id, metrics=TrainMetrics(**metrics))


@router.get("/models", response_model=list[ModelInfo])
async def list_models() -> list[ModelInfo]:
    results = []
    for loaded in list_loaded_models():
        results.append(
            ModelInfo(
                model_id=loaded.metadata.model_id,
                model_type=loaded.metadata.model_type,
                source_kind=loaded.source_kind,
                symbols=loaded.metadata.symbol_universe or [],
            )
        )
    return results
