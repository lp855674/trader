from __future__ import annotations

import json
import os
import re
from pathlib import Path

import torch

from .schemas import LoadedModel, ModelMetadata

SAFE_NAME_PATTERN = re.compile(r"^[A-Za-z0-9_.-]+$")


def get_models_dir() -> Path:
    path = Path(os.getenv("MODEL_ARTIFACTS_DIR") or os.getenv("LSTM_MODELS_DIR") or "models")
    path.mkdir(parents=True, exist_ok=True)
    return path


def _safe_name(value: str) -> bool:
    return bool(SAFE_NAME_PATTERN.fullmatch(value))


def _parse_legacy_name(path: Path) -> tuple[str | None, str | None]:
    stem = path.stem
    if "_" not in stem:
        return None, None
    symbol_part, model_type = stem.rsplit("_", 1)
    if not _safe_name(symbol_part) or not _safe_name(model_type):
        return None, None
    return symbol_part.replace("_", "."), model_type


def discover_legacy_models(models_dir: Path) -> list[LoadedModel]:
    loaded = []
    for path in sorted(models_dir.glob("*.pt")):
        symbol, model_type = _parse_legacy_name(path)
        if symbol is None or model_type is None:
            continue
        checkpoint = torch.load(path, map_location="cpu", weights_only=False)
        metadata = ModelMetadata(
            model_id=path.stem,
            model_type=checkpoint.get("model_type", model_type),
            lookback=checkpoint.get("lookback", 60),
            symbol_universe=[symbol],
            feature_set=None,
            prediction_semantics=None,
            artifact_dir=None,
        )
        loaded.append(
            LoadedModel(
                metadata=metadata,
                checkpoint=checkpoint,
                source_path=path,
                source_kind="legacy_flat_pt",
            )
        )
    return loaded


def discover_artifact_models(models_dir: Path) -> list[LoadedModel]:
    loaded = []
    for artifact_dir in sorted(path for path in models_dir.iterdir() if path.is_dir()):
        metadata_path = artifact_dir / "metadata.json"
        model_path = artifact_dir / "model.pt"
        if not metadata_path.exists() or not model_path.exists():
            continue
        data = json.loads(metadata_path.read_text(encoding="utf-8"))
        checkpoint = torch.load(model_path, map_location="cpu", weights_only=False)
        metadata = ModelMetadata(
            model_id=data["model_id"],
            model_type=data["model_type"],
            lookback=data.get("lookback", checkpoint.get("lookback", 60)),
            symbol_universe=data.get("symbol_universe"),
            feature_set=data.get("feature_set"),
            prediction_semantics=data.get("prediction_semantics"),
            artifact_dir=artifact_dir,
        )
        loaded.append(
            LoadedModel(
                metadata=metadata,
                checkpoint=checkpoint,
                source_path=model_path,
                source_kind="artifact_dir",
            )
        )
    return loaded


def load_models(models_dir: Path | None = None) -> list[LoadedModel]:
    base_dir = models_dir or get_models_dir()
    return discover_artifact_models(base_dir) + discover_legacy_models(base_dir)


def find_model_for_request(
    models: list[LoadedModel],
    symbol: str,
    model_type: str,
) -> LoadedModel | None:
    for loaded in models:
        if loaded.metadata.model_type != model_type:
            continue
        universe = loaded.metadata.symbol_universe
        if universe is None or symbol in universe:
            return loaded
    return None
