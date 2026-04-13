# Remove API key entirely
$env:QUANTD_ACCOUNT_ID = "acc_lb_paper"
$env:QUANTD_DATABASE_URL = "sqlite:quantd.db"
$env:RUST_LOG = "info"
# Don't set QUANTD_API_KEY at all
cargo run -p quantd
