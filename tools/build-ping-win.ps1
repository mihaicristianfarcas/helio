# Build Ping for Windows: Ping.exe (the app) beside FFmpeg's DLLs and the
# host's engine (pong.exe, which the app's hosting switch runs), in
# target\Ping, as the release archive holds them; and ping.exe and
# ping-agent.exe, the CLIs, in target\release.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools\build-ping-win.ps1 [-FFmpeg DIR] [-Install]
#
# -FFmpeg: an FFmpeg 8 shared build (include\, lib\, bin\), e.g. BtbN's
# ffmpeg-n8.1-latest-win64-lgpl-shared; default: the FFMPEG_DIR environment
# variable. The bindings need LLVM's libclang (C:\Program Files\LLVM, or
# LIBCLANG_PATH).
# -Install: install target\Ping as the release's installer does
# (tools\install-ping.ps1): in %LOCALAPPDATA%\Programs\Ping, the Start menu
# and Settings > Apps.
param(
    [string]$FFmpeg = $env:FFMPEG_DIR,
    [switch]$Install
)
$ErrorActionPreference = 'Stop'
if (-not $FFmpeg) { throw 'Pass -FFmpeg DIR or set FFMPEG_DIR: an FFmpeg 8 shared build (include\, lib\, bin\).' }
$repo = Split-Path -Parent $PSScriptRoot
$env:FFMPEG_DIR = $FFmpeg
if (-not $env:LIBCLANG_PATH) { $env:LIBCLANG_PATH = 'C:\Program Files\LLVM\bin' }

Push-Location $repo
try {
    $ErrorActionPreference = 'Continue'
    cargo build --release -p ping-app -p pong -p ping-core -p ping-agent --bins 2>&1 | ForEach-Object { "$_" } | Select-Object -Last 30
    $ErrorActionPreference = 'Stop'
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed ($LASTEXITCODE)" }

    $dist = Join-Path $repo 'target\Ping'
    if (Test-Path $dist) { Remove-Item -Recurse -Force $dist }
    New-Item -ItemType Directory -Force $dist | Out-Null
    Copy-Item (Join-Path $repo 'target\release\ping-app.exe') (Join-Path $dist 'Ping.exe')
    Copy-Item (Join-Path $repo 'target\release\pong.exe') $dist
    # What avcodec needs: itself, avutil, and swresample (some decoders).
    foreach ($dll in Get-ChildItem (Join-Path $FFmpeg 'bin') -Filter '*.dll' | Where-Object { $_.Name -match '^(avcodec|avutil|swresample)-\d+\.dll$' }) {
        Copy-Item $dll.FullName $dist
    }
    # The CLI finds the DLLs beside itself too.
    foreach ($dll in Get-ChildItem $dist -Filter '*.dll') { Copy-Item $dll.FullName (Join-Path $repo 'target\release') }
    Write-Output "built $dist"

    if ($Install) {
        & (Join-Path $PSScriptRoot 'install-ping.ps1') -Source $dist -NoStart
    }
} finally {
    Pop-Location
}
