# FFmpeg's shared build for the Windows jobs of ci.yml and release.yml:
# downloaded from one fixed BtbN/FFmpeg-Builds release and checked against a
# SHA-256 before it is unpacked, as drivers.ps1 does for ViGEmBus. The
# release job signs and ships this build's DLLs, and its headers are
# compiled and run by ffmpeg-sys-next's build script, so the bytes must be
# the ones reviewed, not whatever the `latest` release holds that day.
#
#   ./tools/windows/ffmpeg-ci.ps1 [-AddToPath]
#
# Exports FFMPEG_DIR and LIBCLANG_PATH to the job (GITHUB_ENV); -AddToPath
# also puts the DLLs' folder on PATH, for the tests that load them.
#
# To move to a newer build, pick a tag from github.com/BtbN/FFmpeg-Builds
# (an `autobuild-` release; BtbN prunes the old ones, so a pin that no
# longer downloads fails here and is moved on purpose), download its
# `win64-lgpl-shared` n8.x zip, and replace Tag, Asset and Sha256 with its
# name and `shasum -a 256`. The script fails if the digest differs.
param([switch]$AddToPath)
$ErrorActionPreference = 'Stop'
$Pin = @{
    Tag    = 'autobuild-2026-10-30-13-01'
    Asset  = 'ffmpeg-n8.1.3-27-g8dca1586f4-win64-lgpl-shared-8.1.zip'
    Sha256 = '21b96a5b382b031e89c1c91d7af8efc3db989c4c11bff27422a4c4ea24cf45ed'
}
$zip = Join-Path $env:RUNNER_TEMP $Pin.Asset
# Windows PowerShell's progress bar slows the download tenfold.
$ProgressPreference = 'SilentlyContinue'
Invoke-WebRequest -UseBasicParsing "https://github.com/BtbN/FFmpeg-Builds/releases/download/$($Pin.Tag)/$($Pin.Asset)" -OutFile $zip
$actual = (Get-FileHash -Algorithm SHA256 $zip).Hash
if ($actual -ne $Pin.Sha256) {
    Remove-Item $zip -ErrorAction SilentlyContinue
    throw "$($Pin.Asset) of $($Pin.Tag) is not the FFmpeg build Helio pins (SHA-256 $($Pin.Sha256), got $actual)."
}
Expand-Archive $zip -DestinationPath $env:RUNNER_TEMP
$dir = Join-Path $env:RUNNER_TEMP ($Pin.Asset -replace '\.zip$', '')
"FFMPEG_DIR=$dir" >> $env:GITHUB_ENV
"LIBCLANG_PATH=C:\Program Files\LLVM\bin" >> $env:GITHUB_ENV
if ($AddToPath) { (Join-Path $dir 'bin') >> $env:GITHUB_PATH }
