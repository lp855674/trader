from pathlib import Path
import shutil

from fastapi.testclient import TestClient

from main import app

LOOKBACK = 60
LIVE_TEST_DIR = Path(__file__).resolve().parent / "live_tmp"


def _make_bars(count: int = LOOKBACK):
    return [
        {
            "ts_ms": 1_700_000_000_000 + idx * 60_000,
            "open": 100.0 + idx * 0.1,
            "high": 101.0 + idx * 0.1,
            "low": 99.0 + idx * 0.1,
            "close": 100.5 + idx * 0.1,
            "volume": 1_000 + idx * 10,
        }
        for idx in range(count)
    ]


def _reset_live_dir() -> Path:
    shutil.rmtree(LIVE_TEST_DIR, ignore_errors=True)
    LIVE_TEST_DIR.mkdir(parents=True, exist_ok=True)
    return LIVE_TEST_DIR


def test_health_and_predict_missing_model(monkeypatch):
    models_dir = _reset_live_dir()
    monkeypatch.setenv("LSTM_MODELS_DIR", str(models_dir))

    try:
        with TestClient(app) as client:
            health_resp = client.get("/health")
            assert health_resp.status_code == 200
            health_data = health_resp.json()
            assert health_data["status"] == "ok"
            assert health_data["models_loaded"] == 0
            assert models_dir.exists()

            predict_resp = client.post(
                "/predict",
                json={
                    "symbol": "AAPL",
                    "model_type": "alstm",
                    "bars": _make_bars(),
                },
            )
            assert predict_resp.status_code == 404
            detail = predict_resp.json()["detail"]
            assert isinstance(detail, dict)
            assert detail["error_code"] == "model_not_found"
            assert "AAPL/alstm" in detail["message"]
    finally:
        shutil.rmtree(models_dir, ignore_errors=True)
