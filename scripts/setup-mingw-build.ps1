<#
.SYNOPSIS
    No-admin local build setup for Starwood on Windows.

.DESCRIPTION
    The default Rust target on this machine is `x86_64-pc-windows-msvc`, which
    needs Visual Studio's `link.exe` plus the Windows SDK. Those aren't present
    here and installing them needs admin rights.

    This script avoids that entirely by building with the GNU toolchain
    (`x86_64-pc-windows-gnu`), which uses a self-contained MinGW-w64 `gcc`/`ld`
    instead of MSVC. Everything is installed *locally under your user profile* —
    no admin, nothing touched system-wide.

    It will:
      1. Ensure the `x86_64-pc-windows-gnu` Rust toolchain + std are installed
         (via rustup if available).
      2. Download a portable MinGW-w64 (WinLibs, MSVCRT variant) into
         %USERPROFILE%\starwood-toolchain (only if no gcc is already available).
      3. Build and test the workspace against the GNU target.

    Re-running is safe: existing downloads/toolchains are reused.

.NOTES
    Run from anywhere; it locates the repo as its own parent's parent.
#>

$ErrorActionPreference = 'Stop'
[System.Net.ServicePointManager]::SecurityProtocol = [System.Net.SecurityProtocolType]::Tls12

$RepoRoot   = Split-Path -Parent $PSScriptRoot
$ToolDir    = Join-Path $env:USERPROFILE 'starwood-toolchain'
$MingwDir   = Join-Path $ToolDir 'mingw64'
$MingwBin   = Join-Path $MingwDir 'bin'
$GnuTriple  = 'x86_64-pc-windows-gnu'

# Pinned portable MinGW-w64 (GCC 15.2.0, POSIX threads, MSVCRT runtime — matches
# Rust's x86_64-pc-windows-gnu ABI). Hosted on GitHub releases.
$MingwUrl = 'https://github.com/brechtsanders/winlibs_mingw/releases/download/15.2.0posix-14.0.0-msvcrt-r7/winlibs-x86_64-posix-seh-gcc-15.2.0-mingw-w64msvcrt-14.0.0-r7.zip'

function Write-Step($msg) { Write-Host "`n==> $msg" -ForegroundColor Cyan }

# ---------------------------------------------------------------------------
# 1. Locate rustup / cargo and make sure the GNU toolchain + std are installed.
# ---------------------------------------------------------------------------
Write-Step 'Locating Rust tooling'

$rustup = $null
$cmd = Get-Command rustup -ErrorAction SilentlyContinue
if ($cmd) { $rustup = $cmd.Source }
elseif (Test-Path "$env:USERPROFILE\.cargo\bin\rustup.exe") { $rustup = "$env:USERPROFILE\.cargo\bin\rustup.exe" }

if ($rustup) {
    $env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
    Write-Host "rustup: $rustup"
    Write-Step "Ensuring $GnuTriple toolchain + std (via rustup, user-local)"
    & $rustup toolchain install "stable-$GnuTriple" --profile minimal
    & $rustup component add rust-std --toolchain "stable-$GnuTriple"
    $CargoCmd  = 'cargo'
    $CargoArgs = @("+stable-$GnuTriple")
} else {
    Write-Host 'rustup not found; will use the GNU toolchain cargo directly.'
    $gnuCargo = "$env:USERPROFILE\.rustup\toolchains\stable-$GnuTriple\bin\cargo.exe"
    if (-not (Test-Path $gnuCargo)) {
        throw @"
The GNU toolchain is not installed and rustup is unavailable.
Install rustup locally (no admin) from https://win.rustup.rs (rustup-init.exe),
then re-run this script. rustup installs entirely under %USERPROFILE%.
"@
    }
    $CargoCmd  = $gnuCargo
    $CargoArgs = @()
}

# ---------------------------------------------------------------------------
# 2. Ensure a GCC/ld linker is available (portable MinGW, no admin).
# ---------------------------------------------------------------------------
Write-Step 'Checking for a C compiler / linker (gcc)'

$haveGcc = [bool](Get-Command gcc -ErrorAction SilentlyContinue)
if (-not $haveGcc -and (Test-Path "$MingwBin\gcc.exe")) {
    $env:PATH = "$MingwBin;$env:PATH"
    $haveGcc = $true
}

if (-not $haveGcc) {
    Write-Step 'Downloading portable MinGW-w64 (WinLibs) — this is a large file (~260 MB)'
    New-Item -ItemType Directory -Force -Path $ToolDir | Out-Null
    $zip = Join-Path $ToolDir 'winlibs.zip'
    if (-not (Test-Path $zip)) {
        Write-Host "Downloading $MingwUrl"
        Invoke-WebRequest -Uri $MingwUrl -OutFile $zip
    }
    Write-Step 'Extracting MinGW-w64'
    # The archive contains a top-level mingw64\ directory.
    Expand-Archive -Path $zip -DestinationPath $ToolDir -Force
    if (-not (Test-Path "$MingwBin\gcc.exe")) {
        throw "Extraction finished but gcc.exe not found at $MingwBin"
    }
    $env:PATH = "$MingwBin;$env:PATH"
    Remove-Item $zip -ErrorAction SilentlyContinue
}

Write-Step 'Toolchain versions'
& gcc --version | Select-Object -First 1
& $CargoCmd @CargoArgs --version

# ---------------------------------------------------------------------------
# 3. Build & test against the GNU target.
# ---------------------------------------------------------------------------
Push-Location $RepoRoot
try {
    Write-Step 'Building the render crate'
    & $CargoCmd @CargoArgs build -p starwood_render

    Write-Step 'Running the render crate tests'
    & $CargoCmd @CargoArgs test -p starwood_render

    Write-Step 'Building the full game binary'
    & $CargoCmd @CargoArgs build -p starwood

    Write-Host "`nAll good. To run the game:" -ForegroundColor Green
    Write-Host "  `$env:PATH = `"$MingwBin;`$env:PATH`""
    if ($rustup) {
        Write-Host "  cargo +stable-$GnuTriple run -p starwood"
    } else {
        Write-Host "  & `"$CargoCmd`" run -p starwood"
    }
}
finally {
    Pop-Location
}
