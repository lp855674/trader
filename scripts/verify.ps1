param(
    [int]$Jobs = 1
)

$ErrorActionPreference = "Stop"
$env:CARGO_BUILD_JOBS = "$Jobs"

& "$PSScriptRoot\check-db-boundary.ps1"
& "$PSScriptRoot\check-storage-dto-boundary.ps1"
& "$PSScriptRoot\check-api-read-model-boundary.ps1"

cargo fmt --all -- --check
cargo check --workspace -j $Jobs
cargo test --workspace -j $Jobs
