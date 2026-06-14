param(
    [string]$Target = "x86_64-pc-windows-msvc",
    [switch]$SkipRun
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $RepoRoot

function Resolve-Zig015 {
    $envZig = [Environment]::GetEnvironmentVariable("ZIG")
    $candidates = @()
    if ($envZig) {
        $candidates += $envZig
    }
    $candidates += @(
        (Join-Path $HOME "tools\zig-0.15.2\zig.exe"),
        "C:\tools\zig-0.15.2\zig.exe"
    )

    $pathZig = Get-Command zig -ErrorAction SilentlyContinue
    if ($pathZig) {
        $candidates += $pathZig.Source
    }

    foreach ($candidate in $candidates) {
        if (-not $candidate) {
            continue
        }
        if (-not (Test-Path $candidate)) {
            continue
        }

        $version = (& $candidate version).Trim()
        if ($version -eq "0.15.2") {
            return (Resolve-Path $candidate).Path
        }

        Write-Warning "Ignoring Zig $version at $candidate; Ghostty currently requires Zig 0.15.2."
    }

    throw "Zig 0.15.2 was not found. Set ZIG to a Zig 0.15.2 executable or place it under `$HOME\tools\zig-0.15.2\zig.exe."
}

$zig = Resolve-Zig015
$env:ZIG = $zig
$env:RUST_BACKTRACE = "1"

Write-Host "Repository: $RepoRoot"
Write-Host "Target: $Target"
Write-Host "Zig: $zig"
& $zig version
cargo --version

cargo build --release --target $Target

$releaseDir = Join-Path $RepoRoot "target\$Target\release"
$ghosttyDll = Join-Path $releaseDir "ghostty-vt.dll"
if (-not (Test-Path $ghosttyDll)) {
    throw "Required runtime DLL was not found: $ghosttyDll"
}

if (-not $SkipRun) {
    $exe = Join-Path $releaseDir "codux.exe"
    if (-not (Test-Path $exe)) {
        throw "Built executable was not found: $exe"
    }
    & $exe --version
}
