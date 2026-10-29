# Package Helio for Windows as a release: the app and the host's engine,
# built as one (HELIO_RELEASE=1: they then follow releases, not the main
# branch), in one zip archive for a GitHub release and the website's
# downloads.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools\package-windows.ps1 [-FFmpeg DIR]
#     -> target\dist\Helio-VERSION-windows-x86_64.zip  (Helio.exe beside FFmpeg's DLLs,
#                                                        helio-host.exe, bin\helio.exe, install.ps1,
#                                                        Install Helio.cmd)
#
# helio-host.exe sits beside Helio.exe, where the app's hosting switch finds it;
# turning hosting on copies it to C:\Program Files\Helio Host, which the service
# runs (helio-host/src/hosting/windows.rs). bin\helio.exe is the `helio`
# command (helio-app/src/bin/console.rs), in a folder of its own that
# Helio's Settings > General puts on the PATH. VERSION is the workspace's. -FFmpeg
# (default: FFMPEG_DIR) and LLVM are what tools\build-helio-win.ps1 needs.
# The files sit at the archive's root, so Explorer's Extract All gives one
# folder. The programs are not signed: SmartScreen asks before Helio.exe
# first starts. The release workflow (.github/workflows/release.yml) runs
# this on a Windows runner. HELIO_PROFILE=dev packages a debug build (CI
# checks the packaging so).
param(
    [string]$FFmpeg = $env:FFMPEG_DIR
)
$ErrorActionPreference = 'Stop'
if (-not $FFmpeg) { throw 'Pass -FFmpeg DIR or set FFMPEG_DIR: an FFmpeg 8 shared build (include\, lib\, bin\).' }
$repo = Split-Path -Parent $PSScriptRoot
$manifest = Get-Content (Join-Path $repo 'Cargo.toml') -Raw
if ($manifest -notmatch '(?ms)^\[workspace\.package\].*?^version = "([^"]+)"') { throw 'no version in Cargo.toml [workspace.package]' }
$version = $Matches[1]
$env:FFMPEG_DIR = $FFmpeg
if (-not $env:LIBCLANG_PATH) { $env:LIBCLANG_PATH = 'C:\Program Files\LLVM\bin' }
$env:HELIO_RELEASE = '1'
# Not $profile: PowerShell's own name for the user's profile script.
$cargoProfile = if ($env:HELIO_PROFILE) { $env:HELIO_PROFILE } else { 'release' }
$out = if ($cargoProfile -eq 'dev') { 'debug' } else { $cargoProfile }

Push-Location $repo
try {
    # cargo's progress on stderr is not an error (Windows PowerShell makes
    # it one under 'Stop').
    $ErrorActionPreference = 'Continue'
    cargo build --profile $cargoProfile -p helio-app -p helio-host
    $ErrorActionPreference = 'Stop'
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed ($LASTEXITCODE)" }

    $release = Join-Path $repo "target\$out"
    $dist = Join-Path $repo 'target\dist'
    New-Item -ItemType Directory -Force $dist | Out-Null
    Add-Type -AssemblyName System.IO.Compression.FileSystem

    # zip NAME { fill the folder $args[0] }: the archive target\dist\NAME.zip.
    function zip([string]$name, [scriptblock]$fill) {
        $stage = Join-Path ([IO.Path]::GetTempPath()) $name
        if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
        New-Item -ItemType Directory $stage | Out-Null
        & $fill $stage
        $archive = Join-Path $dist "$name.zip"
        if (Test-Path $archive) { Remove-Item -Force $archive }
        [IO.Compression.ZipFile]::CreateFromDirectory($stage, $archive, 'Optimal', $false)
        Remove-Item -Recurse -Force $stage
        $sum = (Get-FileHash -Algorithm SHA256 $archive).Hash.ToLower()
        Write-Output "$sum  $name.zip"
    }

    # Archives of an earlier build here, under any name, are not this one's.
    Remove-Item (Join-Path $dist '*-windows-x86_64.zip') -ErrorAction SilentlyContinue
    zip "Helio-$version-windows-x86_64" {
        param($dir)
        Copy-Item (Join-Path $release 'helio-app.exe') (Join-Path $dir 'Helio.exe')
        Copy-Item (Join-Path $release 'helio-host.exe') $dir
        New-Item -ItemType Directory (Join-Path $dir 'bin') | Out-Null
        Copy-Item (Join-Path $release 'helio-console.exe') (Join-Path $dir 'bin\helio.exe')
        # The DLLs tools\build-helio-win.ps1 puts beside Helio.exe: avcodec,
        # avutil, and swresample (some decoders).
        Get-ChildItem (Join-Path $FFmpeg 'bin') -Filter '*.dll' |
            Where-Object { $_.Name -match '^(avcodec|avutil|swresample)-\d+\.dll$' } |
            ForEach-Object { Copy-Item $_.FullName $dir }
        # FFmpeg is LGPL: its license goes with its DLLs.
        $license = Join-Path $FFmpeg 'LICENSE.txt'
        if (Test-Path $license) { Copy-Item $license (Join-Path $dir 'FFmpeg LICENSE.txt') }
        Copy-Item (Join-Path $PSScriptRoot 'install-helio.ps1') (Join-Path $dir 'install.ps1')
        # What a person double-clicks. Helio.exe also runs from the unpacked
        # folder as it is, hosting included; installed, it is in the Start
        # menu and Settings > Apps. The window closes once Helio opens, and
        # stays to show why if it did not install.
        $cmd = "@echo off`r`nrem Install Helio from this folder for you: in the Start menu, and in Settings > Apps to uninstall.`r`n" +
            "powershell -NoProfile -ExecutionPolicy Bypass -File `"%~dp0install.ps1`"`r`nif errorlevel 1 pause`r`n"
        [IO.File]::WriteAllText((Join-Path $dir 'Install Helio.cmd'), $cmd)
    }
} finally {
    Pop-Location
}
