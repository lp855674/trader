from __future__ import annotations

import subprocess

import numpy as np
from fastapi.testclient import TestClient

from main import app
from workflow import data_update


def test_update_data_returns_calendar_range(monkeypatch, tmp_path):
    provider = tmp_path / "qlib_data" / "us_data"
    (provider / "calendars").mkdir(parents=True)
    (provider / "instruments").mkdir(parents=True)
    (provider / "features" / "aapl").mkdir(parents=True)
    (provider / "calendars" / "day.txt").write_text("2020-01-01\n2025-12-31\n", encoding="utf-8")
    (provider / "instruments" / "all.txt").write_text("AAPL\t2020-01-02\t2025-12-31\n", encoding="utf-8")
    np.array([0, 1.0, 2.0, 3.0], dtype="<f4").tofile(provider / "features" / "aapl" / "close.day.bin")

    monkeypatch.setattr(data_update, "_provider_uri", lambda: provider)

    calls: list[list[str]] = []

    def fake_run(command, **kwargs):
        calls.append(command)
        return subprocess.CompletedProcess(command, 0, stdout="ok", stderr="")

    monkeypatch.setattr(data_update.subprocess, "run", fake_run)

    with TestClient(app) as client:
        resp = client.post(
            "/data/update",
            json={
                "symbols": ["AAPL.US"],
                "start": "2020-01-01",
                "end": "2025-12-31",
            },
        )

    assert resp.status_code == 200
    body = resp.json()
    assert "--symbol" in calls[0]
    assert "AAPL.US" in calls[0]
    assert "--start" in calls[0]
    assert "2020-01-01" in calls[0]
    assert "--end" in calls[0]
    assert "2025-12-31" in calls[0]
    assert body["provider_uri"] == str(provider)
    assert body["calendar_start"] == "2020-01-01"
    assert body["calendar_end"] == "2025-12-31"
    assert body["updated"][0]["effective_start"] == "2020-01-02"
    assert body["updated"][0]["effective_end"] == "2025-12-31"
    assert body["updated"][0]["rows_written"] == 3


def test_update_data_requires_symbols():
    with TestClient(app) as client:
        resp = client.post("/data/update", json={"symbols": []})

    assert resp.status_code == 422
    assert "symbols must not be empty" in resp.json()["detail"]


def test_update_data_surfaces_native_qlib_failure(monkeypatch, tmp_path):
    provider = tmp_path / "qlib_data" / "us_data"
    provider.mkdir(parents=True)
    monkeypatch.setattr(data_update, "_provider_uri", lambda: provider)

    def fake_run(command, **kwargs):
        return subprocess.CompletedProcess(command, 1, stdout="", stderr="native qlib failed")

    monkeypatch.setattr(data_update.subprocess, "run", fake_run)

    with TestClient(app) as client:
        resp = client.post("/data/update", json={"symbols": ["AAPL.US"]})

    assert resp.status_code == 500
    assert "native qlib failed" in resp.json()["detail"]
