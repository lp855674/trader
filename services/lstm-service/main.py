from __future__ import annotations

import os
from pathlib import Path

from fastapi import FastAPI

from qlib_pipeline.train import router as train_router
from qlib_pipeline.predict import router as predict_router
from qlib_pipeline.backtest import router as backtest_router
from qlib_pipeline.features import router as features_router

MODELS_DIR = Path(os.getenv("LSTM_MODELS_DIR", "models"))
MODELS_DIR.mkdir(exist_ok=True)

app = FastAPI(title="lstm-service", version="0.1.0")

app.include_router(train_router)
app.include_router(predict_router)
app.include_router(backtest_router)
app.include_router(features_router)


@app.get("/health")
async def health() -> dict:
    model_files = list(MODELS_DIR.glob("*.pt"))
    return {"status": "ok", "models_loaded": len(model_files)}
