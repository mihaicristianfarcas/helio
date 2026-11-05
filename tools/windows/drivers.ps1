# The drivers Helio Host installs as hosting is turned on that do not come
# inside helio-host.exe (helio-host/src/hosting/drivers.rs): ViGEmBus's
# installer, as its author signed it, pinned by its SHA-256 (the same as
# drivers.rs's VIGEMBUS_SHA256: the engine checks its copy against that),
# and the notices of both drivers. Dot-sourced by tools\package-windows.ps1
# and tools\build-helio-win.ps1:
#
#   . (Join-Path $PSScriptRoot 'windows\drivers.ps1'); Add-Drivers $repo $stage
#
# -> $stage\drivers\ViGEmBus_setup.exe, ViGEmBus LICENSE.txt, SudoVDA.md.
# The download is kept in target\downloads and fetched again only if it is
# not the one pinned.
$ViGEmBus = @{
    Url    = 'https://github.com/nefarius/ViGEmBus/releases/download/v1.22.0/ViGEmBus_1.22.0_x64_x86_arm64.exe'
    Sha256 = '89220a7865076b342892f98865f3499fb7c4cfd673159e89d352c360fd014c6a'
}

function Add-Drivers([string]$repo, [string]$stage) {
    $cache = Join-Path $repo 'target\downloads'
    New-Item -ItemType Directory -Force $cache | Out-Null
    $file = Join-Path $cache (Split-Path -Leaf $ViGEmBus.Url)
    $pinned = { (Test-Path $file) -and (Get-FileHash -Algorithm SHA256 $file).Hash -eq $ViGEmBus.Sha256 }
    if (-not (& $pinned)) {
        # Windows PowerShell's progress bar slows the download tenfold.
        $ProgressPreference = 'SilentlyContinue'
        Invoke-WebRequest -UseBasicParsing $ViGEmBus.Url -OutFile $file
    }
    if (-not (& $pinned)) {
        Remove-Item $file -ErrorAction SilentlyContinue
        throw "$($ViGEmBus.Url) is not the ViGEmBus installer Helio pins (SHA-256 $($ViGEmBus.Sha256))."
    }
    $drivers = Join-Path $stage 'drivers'
    New-Item -ItemType Directory -Force $drivers | Out-Null
    Copy-Item $file (Join-Path $drivers 'ViGEmBus_setup.exe')
    # BSD-3-Clause: its notice goes with it. SudoVDA's README says where it
    # is from and under which licenses.
    Copy-Item (Join-Path $repo 'vendor\vigembus\LICENSE.txt') (Join-Path $drivers 'ViGEmBus LICENSE.txt')
    Copy-Item (Join-Path $repo 'vendor\sudovda\README.md') (Join-Path $drivers 'SudoVDA.md')
}
