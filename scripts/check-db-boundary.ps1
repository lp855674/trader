$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repoRoot

$matches = rg -n "sqlx|SqlitePool|Pool<Sqlite>|SqliteConnection|Transaction<" `
    crates `
    --glob "*.rs" `
    --glob "Cargo.toml" `
    --glob "!crates/storage/**"

if ($LASTEXITCODE -eq 0) {
    Write-Host "Database boundary violation found outside the persistence boundary:"
    Write-Host $matches
    exit 1
}

if ($LASTEXITCODE -gt 1) {
    exit $LASTEXITCODE
}

Write-Host "Database boundary check passed."
