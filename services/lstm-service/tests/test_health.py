import os
import shutil
from pathlib import Path

from fastapi.testclient import TestClient

from main import app

HEALTH_MODELS_DIR = Path(__file__).resolve().parent / "health_tmp"


def test_health(monkeypatch):
    monkeypatch.setenv("LSTM_MODELS_DIR", str(HEALTH_MODELS_DIR))
    shutil.rmtree(HEALTH_MODELS_DIR, ignore_errors=True)
    HEALTH_MODELS_DIR.mkdir(parents=True, exist_ok=True)
    try:
        with TestClient(app) as client:
            resp = client.get("/health")
            assert resp.status_code == 200
            data = resp.json()
            assert data["status"] == "ok"
            assert "models_loaded" in data
    finally:
        shutil.rmtree(HEALTH_MODELS_DIR, ignore_errors=True)
