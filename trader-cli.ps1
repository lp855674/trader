[CmdletBinding(PositionalBinding = $false)]
param(
    [ValidateSet("check", "run", "build", "test", "clean")]
    [string]$Task = "run",

    [string]$Config = "",
    [string]$RustLog = "",
    [switch]$Release,
    [switch]$DebugBuild,
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$CliArgs
)

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$LocalEnvScript = Join-Path $ScriptDir "trader-cli.local.ps1"
if (Test-Path -LiteralPath $LocalEnvScript)
{
    . $LocalEnvScript
}

function Require-Command
{
    param(
        [string]$Name,
        [string]$InstallHint
    )

    if (-not (Get-Command $Name -ErrorAction SilentlyContinue))
    {
        throw "$Name not found. $InstallHint"
    }
}

function Set-OptionalEnv
{
    param(
        [string]$Name,
        [string]$Value
    )

    if (-not [string]::IsNullOrWhiteSpace($Value))
    {
        Set-Item -Path "Env:$Name" -Value $Value
    }
}

function Invoke-Cargo
{
    param([string[]]$Arguments)

    cargo @Arguments
    if ($LASTEXITCODE -ne 0)
    {
        exit $LASTEXITCODE
    }
}

Set-Location $ScriptDir
Require-Command "cargo" "Install Rust from https://rustup.rs/."
Set-OptionalEnv "RUST_LOG" $RustLog

$effectiveCliArgs = @($CliArgs)
if (-not [string]::IsNullOrWhiteSpace($Config) -and -not ($effectiveCliArgs -contains "--config"))
{
    $effectiveCliArgs += @("--config", $Config)
}

$cargoArgs = @("-p", "trader-cli")
$cargoProfileArgs = @()
$cargoJobArgs = @()
$linksExecutable = @("run", "build", "test") -contains $Task

if ($Release -or ($linksExecutable -and -not $DebugBuild))
{
    $cargoProfileArgs += "--release"
}
else
{
    $cargoJobArgs += @("-j", "1")
    $env:_CL_ = "/Z7"
}

switch ($Task)
{
    "check"
    {
        Invoke-Cargo -Arguments (@("check") + $cargoArgs + $cargoJobArgs)
        break
    }
    "run"
    {
        Invoke-Cargo -Arguments (@("run") + $cargoArgs + $cargoJobArgs + $cargoProfileArgs + @("--") + $effectiveCliArgs)
        break
    }
    "build"
    {
        Invoke-Cargo -Arguments (@("build") + $cargoArgs + $cargoJobArgs + $cargoProfileArgs)
        break
    }
    "test"
    {
        Invoke-Cargo -Arguments (@("test") + $cargoArgs + $cargoJobArgs + $cargoProfileArgs)
        break
    }
    "clean"
    {
        cargo clean -p trader-cli
        break
    }
}
