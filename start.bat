@echo off
cd /d "%~dp0"
if not exist "%~dp0data" mkdir "%~dp0data"
set QUANTD_DATABASE_URL=sqlite://./data/quantd.db
set QUANTD_ENV=dev
cargo run --bin quantd
pause
