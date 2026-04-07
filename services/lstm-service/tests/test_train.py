import pytest
from fastapi.testclient import TestClient
from main import app

client = TestClient(app)

def test_train_unknown_model_type_returns_422():
    resp = client.post("/train", json={
        "symbol": "AAPL.US",
        "model_type": "unknown_model",
        "start": "2023-01-01",
        "end": "2023-12-31",
    })
    assert resp.status_code == 422

def test_train_response_schema():
    """Integration: requires internet + qlib data. Skip in CI without marker."""
    pytest.skip("Integration test: requires Qlib Yahoo data provider")
