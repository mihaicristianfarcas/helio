# FFmpeg's shared build for the Windows jobs of ci.yml and release.yml:
# one fixed BtbN/FFmpeg-Builds build, downloaded from a copy in this
# repository's own releases and checked against a SHA-256 before it is
# unpacked, as drivers.ps1 does for ViGEmBus. The release job signs and
# ships this build's DLLs, and its headers are compiled and run by
# ffmpeg-sys-next's build script, so the bytes must be the ones reviewed,
# not whatever BtbN's `latest` release holds that day. The copy is there
# because BtbN prunes its dated `autobuild-` releases, which would break
# every Windows job a few weeks after a pin.
#
#   ./tools/windows/ffmpeg-ci.ps1 [-AddToPath]
#
# Exports FFMPEG_DIR and LIBCLANG_PATH to the job (GITHUB_ENV); -AddToPath
# also puts the DLLs' folder on PATH, for the tests that load them. And
# FFMPEG_BUILD, the pin's tag, which the jobs put in their Rust cache's
# key: ffmpeg-sys-next's build output names FFmpeg's folder, and a cache
# from another pin linked against a folder that was not there (cargo
# reused that output although FFMPEG_DIR had changed).
#
# To move to a newer build: download a `win64-lgpl-shared` n8.x zip from
# an `autobuild-` release of github.com/BtbN/FFmpeg-Builds, copy it into a
# release of this repository that is a pre-release and never the latest
# one (Helio's updater reads the latest), and replace Tag, Asset and Sha256:
#
#   gh release create ci-ffmpeg-VERSION ZIP --prerelease --latest=false
#   shasum -a 256 ZIP
#
# The release's notes say where the zip came from. The script fails if the
# digest differs.
param([switch]$AddToPath)
$ErrorActionPreference = 'Stop'
$Pin = @{
    # BtbN's autobuild-2026-10-30-13-01, unchanged.
    Tag    = 'ci-ffmpeg-n8.1.3-27-g8dca1586f4'
    Asset  = 'ffmpeg-n8.1.3-27-g8dca1586f4-win64-lgpl-shared-8.1.zip'
    Sha256 = '21b96a5b382b031e89c1c91d7af8efc3db989c4c11bff27422a4c4ea24cf45ed'
}
$zip = Join-Path $env:RUNNER_TEMP $Pin.Asset
# Windows PowerShell's progress bar slows the download tenfold.
$ProgressPreference = 'SilentlyContinue'
Invoke-WebRequest -UseBasicParsing "https://github.com/mihaicristianfarcas/helio/releases/download/$($Pin.Tag)/$($Pin.Asset)" -OutFile $zip
$actual = (Get-FileHash -Algorithm SHA256 $zip).Hash
if ($actual -ne $Pin.Sha256) {
    Remove-Item $zip -ErrorAction SilentlyContinue
    throw "$($Pin.Asset) of $($Pin.Tag) is not the FFmpeg build Helio pins (SHA-256 $($Pin.Sha256), got $actual)."
}
Expand-Archive $zip -DestinationPath $env:RUNNER_TEMP
$dir = Join-Path $env:RUNNER_TEMP ($Pin.Asset -replace '\.zip$', '')
"FFMPEG_DIR=$dir" >> $env:GITHUB_ENV
"LIBCLANG_PATH=C:\Program Files\LLVM\bin" >> $env:GITHUB_ENV
"FFMPEG_BUILD=$($Pin.Tag)" >> $env:GITHUB_ENV
if ($AddToPath) { (Join-Path $dir 'bin') >> $env:GITHUB_PATH }
