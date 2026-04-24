Set-Location E:\code\trader
$env:QUANTD_DATABASE_URL = 'sqlite:E:\code\trader\quantd_tui_manual.db'
$env:QUANTD_HTTP_BIND = '127.0.0.1:18081'
$env:QUANTD_ACCOUNT_ID = 'acc_mvp_paper'
$env:QUANTD_DATA_SOURCE_ID = 'paper_bars'
$env:QUANTD_UNIVERSE_LOOP_ENABLED = '0'
$env:QUANTD_EXEC_SYMBOL_COOLDOWN_SECS = '300'
$env:RUST_LOG = 'info'
cargo run -p quantd
