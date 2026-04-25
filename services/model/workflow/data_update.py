from __future__ import annotations

import os
import subprocess
import sys
from datetime import UTC, datetime
from pathlib import Path

import numpy as np
from fastapi import APIRouter, HTTPException
from pydantic import BaseModel, ConfigDict
from qlib.data.cache import H
from workflow.shared import get_qlib_provider_dir

router = APIRouter()


class DataUpdateRequest(BaseModel):
    model_config = ConfigDict(protected_namespaces=())

    symbols: list[str]
    start: str | None = None
    end: str | None = None


class DataUpdateResult(BaseModel):
    symbol: str
    requested_start: str
    requested_end: str
    effective_start: str
    effective_end: str
    rows_written: int


class DataUpdateResponse(BaseModel):
    model_config = ConfigDict(protected_namespaces=())

    provider_uri: str
    calendar_start: str
    calendar_end: str
    updated: list[DataUpdateResult]


def _provider_uri() -> Path:
    return get_qlib_provider_dir()


def _read_calendar(path: Path) -> list[str]:
    if not path.exists():
        return []
    return [line.strip() for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]


def _read_instruments(path: Path) -> dict[str, tuple[str, str]]:
    instruments: dict[str, tuple[str, str]] = {}
    if not path.exists():
        return instruments
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        symbol, start, end = line.split("\t")[:3]
        instruments[symbol.upper()] = (start, end)
    return instruments


def _feature_rows(provider_uri: Path, qlib_symbol: str, field: str = "close") -> int:
    path = provider_uri / "features" / qlib_symbol.lower() / f"{field}.day.bin"
    if not path.exists():
        return 0
    raw = np.fromfile(path, dtype="<f4")
    return max(len(raw) - 1, 0)


def _update_workspace(provider_uri: Path) -> tuple[Path, Path]:
    work_root = provider_uri.parent / ".collector"
    source_dir = work_root / "source"
    normalize_dir = work_root / "normalize"
    source_dir.mkdir(parents=True, exist_ok=True)
    normalize_dir.mkdir(parents=True, exist_ok=True)
    return source_dir, normalize_dir


def _collector_script() -> Path:
    return Path(__file__).resolve().parent.parent / "vendor" / "qlib_scripts" / "data_collector" / "yahoo" / "collector.py"


def _collector_wrapper_script() -> Path:
    return Path(__file__).resolve().parent.parent / "vendor" / "qlib_scripts" / "data_collector" / "yahoo" / "update_with_symbols.py"


def _run_native_qlib_update(provider_uri: Path, symbols: list[str], start_date: str | None, end_date: str | None) -> None:
    source_dir, normalize_dir = _update_workspace(provider_uri)
    command = [
        sys.executable,
        str(_collector_wrapper_script()),
        "--qlib_data_1d_dir",
        str(provider_uri),
        "--source_dir",
        str(source_dir),
        "--normalize_dir",
        str(normalize_dir),
        "--delay",
        "0.5",
    ]
    for symbol in symbols:
        command.extend(["--symbol", symbol])
    if start_date:
        command.extend(["--start", start_date])
    if end_date:
        command.extend(["--end", end_date])

    result = subprocess.run(
        command,
        cwd=_collector_wrapper_script().parent,
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        stderr = result.stderr.strip() or result.stdout.strip() or "qlib yahoo update failed"
        raise HTTPException(status_code=500, detail=stderr)


def _symbol_result(provider_uri: Path, symbol: str, requested_start: str, requested_end: str) -> DataUpdateResult:
    qlib_symbol = symbol.split(".")[0].upper()
    instruments = _read_instruments(provider_uri / "instruments" / "all.txt")
    if qlib_symbol not in instruments:
        raise HTTPException(status_code=404, detail=f"{symbol} not found in updated qlib instruments")
    effective_start, effective_end = instruments[qlib_symbol]
    return DataUpdateResult(
        symbol=f"{qlib_symbol}.US",
        requested_start=requested_start,
        requested_end=requested_end,
        effective_start=effective_start,
        effective_end=effective_end,
        rows_written=_feature_rows(provider_uri, qlib_symbol),
    )


@router.post("/data/update", response_model=DataUpdateResponse)
async def update_data(req: DataUpdateRequest) -> DataUpdateResponse:
    if not req.symbols:
        raise HTTPException(status_code=422, detail="symbols must not be empty")

    provider_uri = _provider_uri()
    requested_start = req.start or ""
    requested_end = req.end or datetime.now(UTC).date().isoformat()
    _run_native_qlib_update(provider_uri, req.symbols, req.start, req.end)

    calendar = _read_calendar(provider_uri / "calendars" / "day.txt")
    if not calendar:
        raise HTTPException(status_code=500, detail="Qlib calendar is empty after update")

    H["c"].clear()
    updated = [_symbol_result(provider_uri, symbol, requested_start, requested_end) for symbol in req.symbols]
    return DataUpdateResponse(
        provider_uri=str(provider_uri),
        calendar_start=calendar[0],
        calendar_end=calendar[-1],
        updated=updated,
    )
