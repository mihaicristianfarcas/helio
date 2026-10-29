# Build Helio for Windows: Helio.exe (the app) beside FFmpeg's DLLs and the
# host's engine (helio-host.exe, which the app's hosting switch runs), with
# bin\helio.exe (the `helio` command, which runs Helio.exe's), in
# target\Helio, as the release archive holds them; and helio.exe and
# helio-agent.exe, the CLIs on their own, in target\release.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools\build-helio-win.ps1 [-FFmpeg DIR] [-Install]
#
# -FFmpeg: an FFmpeg 8 shared build (include\, lib\, bin\), e.g. BtbN's
# ffmpeg-n8.1-latest-win64-lgpl-shared; default: the FFMPEG_DIR environment
# variable. The bindings need LLVM's libclang (C:\Program Files\LLVM, or
# LIBCLANG_PATH).
# -Install: install target\Helio as the release's installer does
# (tools\install-helio.ps1): in %LOCALAPPDATA%\Programs\Helio, the Start menu
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
    cargo build --release -p helio-app -p helio-host -p helio-cli -p helio-agent --bins 2>&1 | ForEach-Object { "$_" } | Select-Object -Last 30
    $ErrorActionPreference = 'Stop'
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed ($LASTEXITCODE)" }

    $dist = Join-Path $repo 'target\Helio'
    if (Test-Path $dist) { Remove-Item -Recurse -Force $dist }
    New-Item -ItemType Directory -Force $dist | Out-Null
    Copy-Item (Join-Path $repo 'target\release\helio-app.exe') (Join-Path $dist 'Helio.exe')
    Copy-Item (Join-Path $repo 'target\release\helio-host.exe') $dist
    New-Item -ItemType Directory (Join-Path $dist 'bin') | Out-Null
    Copy-Item (Join-Path $repo 'target\release\helio-console.exe') (Join-Path $dist 'bin\helio.exe')
    # What avcodec needs: itself, avutil, and swresample (some decoders).
    foreach ($dll in Get-ChildItem (Join-Path $FFmpeg 'bin') -Filter '*.dll' | Where-Object { $_.Name -match '^(avcodec|avutil|swresample)-\d+\.dll$' }) {
        Copy-Item $dll.FullName $dist
    }
    # The CLI finds the DLLs beside itself too.
    foreach ($dll in Get-ChildItem $dist -Filter '*.dll') { Copy-Item $dll.FullName (Join-Path $repo 'target\release') }
    Write-Output "built $dist"

    if ($Install) {
        & (Join-Path $PSScriptRoot 'install-helio.ps1') -Source $dist -NoStart
    }
} finally {
    Pop-Location
}
