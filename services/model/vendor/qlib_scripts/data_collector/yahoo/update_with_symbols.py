from __future__ import annotations

import argparse
import multiprocessing
from pathlib import Path

import collector as yahoo_collector_module
from collector import Run, YahooCollectorUS1d
from dump_bin import DumpDataUpdate


class YahooCollectorUS1dSymbols(YahooCollectorUS1d):
    symbols_override: list[str] = []

    def get_instrument_list(self):
        return list(self.symbols_override)


class RunWithSymbols(Run):
    @property
    def collector_class_name(self):
        return "YahooCollectorUS1dSymbols"


yahoo_collector_module.YahooCollectorUS1dSymbols = YahooCollectorUS1dSymbols


def _default_start(qlib_data_1d_dir: Path) -> str:
    calendar_path = qlib_data_1d_dir / "calendars" / "day.txt"
    if not calendar_path.exists():
        return "2000-01-01"
    calendar = [line.strip() for line in calendar_path.read_text(encoding="utf-8").splitlines() if line.strip()]
    if len(calendar) >= 2:
        return calendar[-2]
    if calendar:
        return calendar[-1]
    return "2000-01-01"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--qlib_data_1d_dir", required=True)
    parser.add_argument("--source_dir", required=True)
    parser.add_argument("--normalize_dir", required=True)
    parser.add_argument("--start")
    parser.add_argument("--end")
    parser.add_argument("--delay", type=float, default=0.5)
    parser.add_argument("--check_data_length", type=int)
    parser.add_argument("--symbol", action="append", dest="symbols", required=True)
    args = parser.parse_args()

    qlib_dir = Path(args.qlib_data_1d_dir).expanduser().resolve()
    YahooCollectorUS1dSymbols.symbols_override = [symbol.split(".")[0].upper() for symbol in args.symbols]

    run = RunWithSymbols(
        source_dir=args.source_dir,
        normalize_dir=args.normalize_dir,
        max_workers=1,
        interval="1d",
        region="US",
    )
    start = args.start or _default_start(qlib_dir)
    run.download_data(
        delay=args.delay,
        start=start,
        end=args.end,
        check_data_length=args.check_data_length,
    )
    run.max_workers = max(multiprocessing.cpu_count() - 2, 1)
    run.normalize_data_1d_extend(str(qlib_dir))
    DumpDataUpdate(
        data_path=run.normalize_dir,
        qlib_dir=str(qlib_dir),
        exclude_fields="symbol,date",
        max_workers=run.max_workers,
    ).dump()


if __name__ == "__main__":
    main()
