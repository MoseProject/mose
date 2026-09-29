# Captures and analyzes a native crash of the Tauri desktop shell (apps/desktop) on Windows.
#
# Background: a *debug* build of oca-desktop.exe can abort at startup with an unlogged
# "Microsoft Visual C++ Runtime Library" dialog, before tracing/panic-hook init ever runs.
# WinDbg/procdump analysis (2026-09-28, this repo) found the real cause: the debug C runtime's
# heap validator (`ucrtbased.dll`, only linked in debug builds) catches a heap-corrupting
# destructor call on a `std::map<enum, const char*>` owned by a statically-linked C++
# dependency (most likely whisper.cpp via whisper-rs-sys, given the demangled symbol
# `std::_Tree_val<...std::pair<enum asr_tensor const, char const*>...>::_Erase_tree`), torn
# down during the process's static-initialization phase. A `--release` build (non-debug CRT,
# `ucrtbase.dll`) launches fine -- this is a debug-build-only false alarm, not something that
# affects the shipped installer (release.yml always builds `--release`). This script exists to
# reproduce that finding on another machine, or to re-diagnose if the underlying static
# dependency's ABI ever actually changes and starts corrupting real state.
#
# This does NOT require Administrator rights: it extracts cdb.exe (the console-only member of
# "Debugging Tools for Windows") from the Windows SDK's installer via an MSI admin-image
# extraction (`msiexec /a ... TARGETDIR=...`), which just copies files -- no system-wide
# install, no registry writes. WinDbg Preview (the GUI app, `winget install Microsoft.WinDbg`)
# was tried first and rejected: its `-c`/`-cf` command-line automation did not execute in this
# environment (no interactive session to drive its window), while cdb.exe's is designed for
# exactly this non-interactive use and worked on the first try.
param(
    [string]$FfmpegDir = "C:\Users\lucas\AppData\Local\Microsoft\WinGet\Packages\BtbN.FFmpeg.LGPL.Shared.8.1_Microsoft.Winget.Source_8wekyb3d8bbwe\ffmpeg-n8.1.2-34-g9b6c8969e0-win64-lgpl-shared-8.1",
    [string]$GStreamerBinDir = "C:\Users\lucas\gstreamer\1.0\mingw_x86_64\bin",
    [string]$BuildProfile = "debug",  # "debug" (reproduces the crash) or "release" (should launch fine)
    [string]$OutDir = "$PSScriptRoot\..\crash-dumps",
    [string]$CdbDir = "D:\tools\debuggers"
)

$ErrorActionPreference = "Stop"
$RepoRoot = Resolve-Path "$PSScriptRoot\.."
$exe = Join-Path $RepoRoot "target\$BuildProfile\oca-desktop.exe"
if (-not (Test-Path $exe)) {
    Write-Host "Build it first: cargo build -p desktop$(if ($BuildProfile -eq 'release') { ' --release' })" -ForegroundColor Red
    exit 1
}
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

# --- Step 1: get cdb.exe without installing anything system-wide ------------------------------
$cdb = Get-ChildItem $CdbDir -Recurse -Filter "cdb.exe" -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $cdb) {
    Write-Host "cdb.exe not found under $CdbDir -- extracting it from the Windows SDK installer..."
    $layoutDir = "$env:TEMP\sdklayout"
    $setup = "$env:TEMP\winsdksetup.exe"
    if (-not (Test-Path $setup)) {
        Invoke-WebRequest -Uri "https://download.microsoft.com/download/f4b30f2a-4fc3-430e-9b03-c842b5f5f9f1/KIT_BUNDLE_WINDOWSSDK_MEDIACREATION/winsdksetup.exe" -OutFile $setup
    }
    Start-Process -FilePath $setup -ArgumentList "/layout", $layoutDir, "/features", "OptionId.WindowsDesktopDebuggers", "/quiet" -Wait
    $msi = Get-ChildItem $layoutDir -Recurse -Filter "X64 Debuggers And Tools-x64_en-us.msi" | Select-Object -First 1
    if (-not $msi) { Write-Host "Debuggers MSI not found in SDK layout." -ForegroundColor Red; exit 1 }
    New-Item -ItemType Directory -Force -Path $CdbDir | Out-Null
    Start-Process -FilePath "msiexec.exe" -ArgumentList "/a", "`"$($msi.FullName)`"", "/qn", "TARGETDIR=`"$CdbDir`"" -Wait
    $cdb = Get-ChildItem $CdbDir -Recurse -Filter "cdb.exe" -ErrorAction SilentlyContinue | Select-Object -First 1
}
if (-not $cdb) { Write-Host "Could not obtain cdb.exe." -ForegroundColor Red; exit 1 }
Write-Host "Using debugger: $($cdb.FullName)"

# --- Step 2: capture a full-memory dump on the first exception (procdump, no admin needed) ----
if (-not (Get-Command procdump -ErrorAction SilentlyContinue)) {
    winget install --id Microsoft.Sysinternals.Suite -e --accept-source-agreements --accept-package-agreements
}
$env:PATH = "$FfmpegDir\bin;$GStreamerBinDir;$env:PATH"
Get-ChildItem $OutDir -Filter "*.dmp" -ErrorAction SilentlyContinue | Remove-Item -Force
Write-Host "Launching $exe under procdump -- this blocks until it crashes or exits normally."
& procdump -accepteula -ma -x $OutDir $exe
$dump = Get-ChildItem $OutDir -Filter "*.dmp" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $dump) {
    Write-Host "No dump was captured -- '$BuildProfile' build did not crash (expected for release)." -ForegroundColor Green
    exit 0
}
Write-Host "Captured dump: $($dump.FullName)"

# --- Step 3: symbolize and print the faulting thread's stack -----------------------------------
$log = Join-Path $OutDir "cdb-analysis.log"
& $cdb.FullName -z $dump.FullName -y "$RepoRoot\target\$BuildProfile;srv*" -c ".ecxr; kb 30; q" 2>&1 |
    Tee-Object -FilePath $log
Write-Host "`nFull analysis written to $log" -ForegroundColor Cyan
