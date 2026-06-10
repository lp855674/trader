param(
    [int]$Jobs = 1
)

$ErrorActionPreference = "Stop"
$env:CARGO_BUILD_JOBS = "$Jobs"

cargo clippy --workspace --all-targets -j $Jobs
