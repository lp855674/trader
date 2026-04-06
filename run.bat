@echo off
REM Set working directory to project root
cd /d "%~dp0"

echo Working directory: %CD%
echo Database path: sqlite://./data/quantd.db (relative to project root)

if not exist "%CD%\data" mkdir "%CD%\data"

REM Set environment variables
set QUANTD_DATABASE_URL=sqlite://./data/quantd.db
set QUANTD_ENV=dev
set QUANTD_LOG=info

echo Starting quantd...
cargo run --bin quantd

echo.
echo Press any key to exit...
pause >nul
