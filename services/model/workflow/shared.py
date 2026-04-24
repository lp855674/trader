from __future__ import annotations

import json
import re
from datetime import datetime, timezone
from pathlib import Path

import torch
from fastapi import HTTPException

from runtime.loader import get_models_dir, load_models
from runtime.schemas import LoadedModel

SAFE_NAME_PATTERN = re.compile(r"^[A-Za-z0-9_.-]+$")
SUPPORTED_MODELS = {"lstm", "alstm"}


def validate_name(value: str, field: str) -> str:
    if not SAFE_NAME_PATTERN.fullmatch(value):
        raise HTTPException(
            status_code=422,
            detail=f"Invalid {field}: must match {SAFE_NAME_PATTERN.pattern}",
        )
    return value


def build_model_id(symbol: str, model_type: str, now: datetime | None = None) -> str:
    ts = (now or datetime.now(timezone.utc)).strftime("%Y%m%d")
    return f"model_{symbol.replace('.', '_')}_{model_type}_{ts}"


def artifact_dir_for(symbol: str, model_type: str) -> Path:
    safe_symbol = validate_name(symbol, "symbol").replace(".", "_")
    safe_type = validate_name(model_type, "model_type").replace(".", "_")
    return get_models_dir() / f"{safe_symbol}_{safe_type}"


def write_artifact(
    *,
    symbol: str,
    model_type: str,
    checkpoint: dict,
    feature_set: str,
    prediction_semantics: str,
) -> str:
    trained_at = datetime.now(timezone.utc)
    model_id = build_model_id(symbol, model_type, trained_at)
    artifact_dir = artifact_dir_for(symbol, model_type)
    artifact_dir.mkdir(parents=True, exist_ok=True)

    torch.save(checkpoint, artifact_dir / "model.pt")
    training_window = checkpoint.get("training_window") or {}
    (artifact_dir / "metadata.json").write_text(
        json.dumps(
            {
                "model_id": model_id,
                "model_type": model_type,
                "symbol_universe": [symbol],
                "feature_set": feature_set,
                "lookback": checkpoint.get("lookback", 60),
                "prediction_semantics": prediction_semantics,
                "trained_at": trained_at.isoformat(),
                "requested_start": training_window.get("requested_start"),
                "requested_end": training_window.get("requested_end"),
                "effective_start": training_window.get("effective_start"),
                "effective_end": training_window.get("effective_end"),
                "training_window": training_window,
                "metrics": checkpoint.get("metrics"),
            }
        ),
        encoding="utf-8",
    )
    return model_id


def list_loaded_models() -> list[LoadedModel]:
    return load_models()
