from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Any


@dataclass(frozen=True)
class ModelMetadata:
    model_id: str
    model_type: str
    lookback: int
    symbol_universe: list[str] | None
    feature_set: str | None
    prediction_semantics: str | None
    artifact_dir: Path | None = None


@dataclass(frozen=True)
class LoadedModel:
    metadata: ModelMetadata
    checkpoint: dict[str, Any]
    source_path: Path
    source_kind: str  # "legacy_flat_pt" | "artifact_dir"
