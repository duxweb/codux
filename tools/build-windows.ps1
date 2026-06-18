param(
    [string]$Target = "x86_64-pc-windows-msvc",
    [switch]$SkipRun
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $RepoRoot

$env:RUST_BACKTRACE = "1"

Write-Host "Repository: $RepoRoot"
Write-Host "Target: $Target"
cargo --version

cargo build --release --target $Target

$releaseDir = Join-Path $RepoRoot "target\$Target\release"

if (-not $SkipRun) {
    $exe = Join-Path $releaseDir "codux.exe"
    if (-not (Test-Path $exe)) {
        throw "Built executable was not found: $exe"
    }
    & $exe --version
}
