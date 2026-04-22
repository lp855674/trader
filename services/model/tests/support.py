from __future__ import annotations

import shutil
import uuid
from pathlib import Path


def make_case_dir(name: str) -> Path:
    base_dir = Path(__file__).resolve().parent / ".tmp"
    base_dir.mkdir(parents=True, exist_ok=True)
    case_dir = base_dir / f"{name}_{uuid.uuid4().hex}"
    case_dir.mkdir(parents=True, exist_ok=True)
    return case_dir


def cleanup_case_dir(path: Path) -> None:
    shutil.rmtree(path, ignore_errors=True)
