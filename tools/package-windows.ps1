# Package Helio for Windows as a release: the app and the host's engine,
# built as one (HELIO_RELEASE=1: they then follow releases, not the main
# branch), as an installer and as a zip archive of the same files, for a
# GitHub release and the website's downloads.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools\package-windows.ps1 [-FFmpeg DIR] [-RequireSignature] [-Sign]
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools\package-windows.ps1 -StageOnly [-FFmpeg DIR] [-RequireSignature]
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools\package-windows.ps1 -From DIR [-Sign]
#     -> target\dist\Helio-VERSION-windows-x86_64-setup.exe  (tools\windows\helio.iss: installs Helio
#                                                             for whoever runs it, no administrator)
#        target\dist\Helio-VERSION-windows-x86_64.zip        (the same files, unpacked; Helio's own update
#                                                             runs the installer, helio-update/src/install/windows.rs)
#
# The files: Helio.exe beside FFmpeg's DLLs (avcodec, avutil, swresample)
# and FFmpeg's license, helio-host.exe, bin\helio.exe, and drivers\: the
# ViGEmBus installer that turning hosting on runs, and the drivers'
# notices (tools\windows\drivers.ps1). helio-host.exe
# sits beside Helio.exe, where the app's hosting switch finds it; turning
# hosting on copies it to C:\Program Files\Helio Host, which the service
# runs (helio-host/src/hosting/windows.rs). bin\helio.exe is the `helio`
# command (helio-app/src/bin/console.rs), in a folder of its own that
# Helio's Settings > General puts on the PATH. VERSION is the workspace's.
# -FFmpeg (default: FFMPEG_DIR) and LLVM are what tools\build-helio-win.ps1
# needs; the installer, Inno Setup 6.6 or newer (ISCC.exe where Inno Setup
# installs it, or INNO_SETUP_COMPILER).
#
# -StageOnly builds the files into target\stage\Helio and stops there: the
# release workflow (.github/workflows/release.yml) signs and packages them
# on another runner, with -From. -RequireSignature builds the engine with
# its `require-signature` feature: it then turns hosting on only with an
# engine signed by HELIO_HOST_WINDOWS_PUBLISHER, which must be set
# (helio-host/src/hosting/signature.rs).
#
# -Sign signs every program and DLL before they are packaged, so the zip
# and the installer hold the same signed files, then the installer, its
# uninstaller and the uninstaller's copies of itself (helio.iss's SignTool),
# with ssign: Certum's certificate in its cloud (SimplySign), from
# CERTUM_EMAIL and CERTUM_OTP (the TOTP seed) or CERTUM_TOKEN (a code read
# from the authenticator, for signing by hand), as ssign reads them. ssign
# is the one on PATH, or SSIGN. Each signature is timestamped (ssign's
# default, Certum's timestamp authority), so it stays valid after the
# certificate expires. Without -Sign nothing is signed, and SmartScreen
# asks before Helio first starts.
#
# HELIO_PROFILE=dev packages a debug build, its installer uncompressed (CI
# checks the packaging so).
param(
    [string]$FFmpeg = $env:FFMPEG_DIR,
    [switch]$StageOnly,
    [string]$From,
    [switch]$RequireSignature,
    [switch]$Sign
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'windows\drivers.ps1')
$manifest = Get-Content (Join-Path $repo 'Cargo.toml') -Raw
if ($manifest -notmatch '(?ms)^\[workspace\.package\].*?^version = "([^"]+)"') { throw 'no version in Cargo.toml [workspace.package]' }
$version = $Matches[1]
# Not $profile: PowerShell's own name for the user's profile script.
$cargoProfile = if ($env:HELIO_PROFILE) { $env:HELIO_PROFILE } else { 'release' }
$name = "Helio-$version-windows-x86_64"
$dist = Join-Path $repo 'target\dist'

# Build, and put the files where the packages are made from: $stage.
function Build([string]$stage) {
    if (-not $FFmpeg) { throw 'Pass -FFmpeg DIR or set FFMPEG_DIR: an FFmpeg 8 shared build (include\, lib\, bin\).' }
    $env:FFMPEG_DIR = $FFmpeg
    if (-not $env:LIBCLANG_PATH) { $env:LIBCLANG_PATH = 'C:\Program Files\LLVM\bin' }
    $env:HELIO_RELEASE = '1'
    $features = @()
    if ($RequireSignature) {
        if (-not $env:HELIO_HOST_WINDOWS_PUBLISHER) {
            throw '-RequireSignature needs HELIO_HOST_WINDOWS_PUBLISHER: the signing certificate''s whole subject (CN=..., O=..., C=...).'
        }
        $features = @('--features', 'helio-host/require-signature')
    }
    $out = if ($cargoProfile -eq 'dev') { 'debug' } else { $cargoProfile }
    Push-Location $repo
    try {
        # cargo's progress on stderr is not an error (Windows PowerShell
        # makes it one under 'Stop').
        $ErrorActionPreference = 'Continue'
        cargo build --profile $cargoProfile -p helio-app -p helio-host @features
        $ErrorActionPreference = 'Stop'
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed ($LASTEXITCODE)" }
    } finally {
        Pop-Location
    }

    $release = Join-Path $repo "target\$out"
    if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
    New-Item -ItemType Directory -Force (Join-Path $stage 'bin') | Out-Null
    Copy-Item (Join-Path $release 'helio-app.exe') (Join-Path $stage 'Helio.exe')
    Copy-Item (Join-Path $release 'helio-host.exe') $stage
    Copy-Item (Join-Path $release 'helio-console.exe') (Join-Path $stage 'bin\helio.exe')
    # The DLLs tools\build-helio-win.ps1 puts beside Helio.exe: avcodec,
    # avutil, and swresample (some decoders).
    $dlls = @(Get-ChildItem (Join-Path $FFmpeg 'bin') -Filter '*.dll' |
        Where-Object { $_.Name -match '^(avcodec|avutil|swresample)-\d+\.dll$' })
    if ($dlls.Count -ne 3) { throw "FFmpeg's bin has $($dlls.Count) of avcodec, avutil and swresample, not the three." }
    $dlls | ForEach-Object { Copy-Item $_.FullName $stage }
    # FFmpeg is LGPL: its license goes with its DLLs.
    $license = Join-Path $FFmpeg 'LICENSE.txt'
    if (-not (Test-Path $license)) { throw "There is no LICENSE.txt in $FFmpeg, and FFmpeg's DLLs go only with it." }
    Copy-Item $license (Join-Path $stage 'FFmpeg LICENSE.txt')
    Add-Drivers $repo $stage
}

function Ssign {
    $ssign = if ($env:SSIGN) { $env:SSIGN } else { (Get-Command ssign -ErrorAction SilentlyContinue).Source }
    if (-not $ssign) { throw '-Sign needs ssign (cargo install ssign), on PATH or in SSIGN.' }
    if (-not $env:CERTUM_EMAIL -or -not ($env:CERTUM_OTP -or $env:CERTUM_TOKEN)) {
        throw '-Sign needs CERTUM_EMAIL, and CERTUM_OTP or CERTUM_TOKEN, in the environment (never on the command line).'
    }
    $ssign
}

# Sign the files staged in $stage, each with the name Windows shows for it
# (UAC's "Program name", when hosting is turned on: Helio Host). One
# login signs them all: ssign keeps the session for 20 minutes, and the
# installer's signatures, a few minutes later, use it too.
function Sign-Files([string]$ssign, [string]$stage) {
    $groups = @(
        @{ Name = 'Helio'; Files = @('Helio.exe', 'bin\helio.exe') },
        @{ Name = 'Helio Host'; Files = @('helio-host.exe') },
        @{ Name = $null; Files = @(Get-ChildItem $stage -Filter '*.dll' | ForEach-Object { $_.Name }) }
    )
    $signed = @()
    foreach ($group in $groups) {
        $files = @($group.Files | ForEach-Object { Join-Path $stage $_ })
        $named = if ($group.Name) { @('-n', $group.Name) } else { @() }
        & $ssign @named -u 'https://helio.mihaicristian.dev' @files
        if ($LASTEXITCODE -ne 0) { throw "ssign failed ($LASTEXITCODE)" }
        $signed += $files
    }
    # Every program and DLL in the package, and none left out, but for
    # drivers\: ViGEmBus's installer stays as its author signed it.
    $theirs = Join-Path $stage 'drivers\*'
    $all = @(Get-ChildItem $stage -Recurse -Include '*.exe', '*.dll' |
        Where-Object { $_.FullName -notlike $theirs } | ForEach-Object { $_.FullName })
    $missed = @($all | Where-Object { $signed -notcontains $_ })
    if ($missed.Count) { throw "not signed: $($missed -join ', ')" }
}

function Inno-Compiler {
    if ($env:INNO_SETUP_COMPILER) { return $env:INNO_SETUP_COMPILER }
    $found = Get-Command ISCC -ErrorAction SilentlyContinue
    if ($found) { return $found.Source }
    foreach ($dir in ${env:ProgramFiles(x86)}, $env:ProgramFiles, (Join-Path $env:LOCALAPPDATA 'Programs')) {
        if (-not $dir) { continue }
        $iscc = Join-Path $dir 'Inno Setup 6\ISCC.exe'
        if (Test-Path $iscc) { return $iscc }
    }
    throw 'The installer needs Inno Setup 6.6 or newer (https://jrsoftware.org/isinfo.php): ISCC.exe was not found (set INNO_SETUP_COMPILER to it).'
}

# The zip and the installer, from the files in $stage.
function Package([string]$stage, [string]$ssign) {
    New-Item -ItemType Directory -Force $dist | Out-Null
    # Packages of an earlier build here, under any name, are not this one's.
    Remove-Item (Join-Path $dist '*-windows-x86_64.zip'), (Join-Path $dist '*-windows-x86_64-setup.exe') -ErrorAction SilentlyContinue

    # The files at the archive's root, so Explorer's Extract All gives one
    # folder.
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = Join-Path $dist "$name.zip"
    [IO.Compression.ZipFile]::CreateFromDirectory($stage, $archive, 'Optimal', $false)

    $iscc = Inno-Compiler
    $defines = @("/DVersion=$version", "/DSource=$stage", "/DOutput=$dist")
    if ($cargoProfile -eq 'dev') { $defines += '/DCompression=none' }
    if ($ssign) {
        # $q and $f are ISCC's: a quote, and the file to sign, quoted.
        $defines += '/DSign', ('/Sssign=$q' + $ssign + '$q -n $qHelio$q -u $qhttps://helio.mihaicristian.dev$q $f')
    }
    # ISCC prints its progress on stdout; its errors are what matter.
    & $iscc /Q @defines (Join-Path $PSScriptRoot 'windows\helio.iss')
    if ($LASTEXITCODE -ne 0) { throw "ISCC failed ($LASTEXITCODE)" }

    foreach ($file in $archive, (Join-Path $dist "$name-setup.exe")) {
        $sum = (Get-FileHash -Algorithm SHA256 $file).Hash.ToLower()
        Write-Output "$sum  $(Split-Path -Leaf $file)"
    }
}

if ($StageOnly -and $From) { throw 'Pass -StageOnly or -From, not both.' }
$ssign = if ($Sign -and -not $StageOnly) { Ssign } else { $null }
if ($From) {
    $stage = (Resolve-Path $From).Path
} else {
    $stage = Join-Path $repo 'target\stage\Helio'
    Build $stage
    if ($StageOnly) {
        Write-Output "staged $stage"
        return
    }
}
if ($ssign) { Sign-Files $ssign $stage }
Package $stage $ssign
