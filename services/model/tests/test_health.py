from __future__ import annotations

import json

import torch
from fastapi.testclient import TestClient

from main import app
from runtime.networks import SimpleLSTM
from tests.support import cleanup_case_dir, make_case_dir

def test_health(monkeypatch):
    models_dir = make_case_dir("health_models")
    monkeypatch.setenv("MODEL_ARTIFACTS_DIR", str(models_dir))
    monkeypatch.delenv("LSTM_MODELS_DIR", raising=False)

    torch.save(
        {
            "model_state": SimpleLSTM().state_dict(),
            "model_type": "lstm",
            "input_size": 158,
            "hidden_size": 64,
            "num_layers": 2,
            "lookback": 60,
        },
        models_dir / "AAPL_US_lstm.pt",
    )

    artifact_dir = models_dir / "alstm_alpha360_us_v1"
    artifact_dir.mkdir(parents=True, exist_ok=True)
    torch.save(
        {
            "model_state": SimpleLSTM().state_dict(),
            "model_type": "alstm",
            "input_size": 158,
            "hidden_size": 64,
            "num_layers": 2,
            "lookback": 60,
        },
        artifact_dir / "model.pt",
    )
    (artifact_dir / "metadata.json").write_text(
        json.dumps(
            {
                "model_id": "model_AAPL_US_alstm_20260422",
                "model_type": "alstm",
                "symbol_universe": ["AAPL.US"],
                "feature_set": "Alpha360",
                "lookback": 60,
                "prediction_semantics": "score in [-1,1]",
            }
        ),
        encoding="utf-8",
    )

    try:
        with TestClient(app) as client:
            resp = client.get("/health")
            assert resp.status_code == 200
            data = resp.json()
            assert data["status"] == "ok"
            assert data["models_loaded"] == 2
            assert data["legacy_models_loaded"] == 1
            assert data["artifact_models_loaded"] == 1
    finally:
        cleanup_case_dir(models_dir)
