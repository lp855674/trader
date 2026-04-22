from __future__ import annotations

from fastapi import FastAPI

from runtime.loader import load_models
from workflow.backtest import router as backtest_router
from workflow.features import router as features_router
from workflow.predict import router as predict_router
from workflow.train import router as train_router

app = FastAPI(title="model-service", version="0.1.0")

app.include_router(train_router)
app.include_router(predict_router)
app.include_router(backtest_router)
app.include_router(features_router)


@app.get("/health")
async def health() -> dict:
    loaded_models = load_models()
    legacy_count = sum(1 for model in loaded_models if model.source_kind == "legacy_flat_pt")
    artifact_count = sum(1 for model in loaded_models if model.source_kind == "artifact_dir")
    return {
        "status": "ok",
        "models_loaded": len(loaded_models),
        "legacy_models_loaded": legacy_count,
        "artifact_models_loaded": artifact_count,
    }
