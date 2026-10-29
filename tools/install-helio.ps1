# Install Helio for whoever runs this: Helio.exe, the host's engine (helio-host.exe)
# and FFmpeg's DLLs in %LOCALAPPDATA%\Programs\Helio, Helio in the Start menu,
# and Helio in Windows' list of installed apps (Settings > Apps), whose
# Uninstall runs this script, copied there as install.ps1, with -Uninstall.
# Helio is per user, as its data is: no administrator is needed. Hosting
# stays off: Helio's switch turns it on, which copies helio-host.exe to
# C:\Program Files\Helio Host and runs it as HelioHost (Windows asks for an
# administrator then).
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File install-helio.ps1 [-Source DIR] [-NoStart]
#   powershell -NoProfile -ExecutionPolicy Bypass -File install-helio.ps1 -Uninstall
#
# In a release archive (tools/package-windows.ps1) this script is
# install.ps1, beside Helio.exe, and takes the files from there; in a
# checkout, from target\Helio (tools/build-helio-win.ps1 -Install runs it).
# Helio's own update (helio-update) replaces the files itself and then runs
# the new archive's install.ps1 with -Register FOLDER: when FOLDER is the one
# this installs into, the script there and the entry in the list are brought
# up to date; a copy anywhere else has no entry and is left as it is. Helio
# runs the install.ps1 beside it the same way when it starts and finds its
# entry missing or naming another version (an update by an older release,
# which runs no installer).
#
# Uninstalling turns hosting off first, if it is on (`helio-host disable`: the
# service, its firewall rules and the engine's copy in Program Files go;
# Windows asks for an administrator). It leaves Helio's hosts and settings
# (%APPDATA%\Helio), the host's identity and pairings (C:\ProgramData\Helio Host),
# and the Windows Firewall rules Windows made when it asked about Helio.exe:
# only an administrator can remove those, and they let nothing in once the
# program is gone.
param(
    [string]$Source = $(if (Test-Path (Join-Path $PSScriptRoot 'Helio.exe')) { $PSScriptRoot }
        else { Join-Path $PSScriptRoot '..\target\Helio' }),
    [switch]$NoStart,
    [switch]$Uninstall,
    [string]$Register
)
$ErrorActionPreference = 'Stop'

$dest = Join-Path $env:LOCALAPPDATA 'Programs\Helio'
$link = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\Helio.lnk'
# The entry in Settings > Apps, named as the Mac app's bundle identifier is:
# no other program's entry has that name.
$entry = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\dev.mihaicristian.helio'

# Helio's own files, as helio-update names them (`ours`): only these are
# copied, replaced or removed.
function Test-Ours([string]$Name) {
    $Name -match '^(helio\.exe|helio-host\.exe|ffmpeg license\.txt|(avcodec|avutil|swresample)-\d+\.dll)$'
}

function Test-SameFolder([string]$A, [string]$B) {
    [IO.Path]::GetFullPath($A).TrimEnd('\') -eq [IO.Path]::GetFullPath($B).TrimEnd('\')
}

# The Helio that runs from the installed folder, not a copy run from
# anywhere else (the unpacked download, a build): the path tells them apart.
function Stop-Helio {
    $running = @(Get-Process Helio -ErrorAction SilentlyContinue |
        Where-Object { $_.Path -and $_.Path.StartsWith("$dest\", [StringComparison]::OrdinalIgnoreCase) })
    if ($running.Count -eq 0) { return }
    $running | Stop-Process -Force -ErrorAction SilentlyContinue
    $running | Wait-Process -Timeout 15 -ErrorAction SilentlyContinue
}

# Hosting off, while it is on (HelioHost is there): with the installed
# engine, else the service's own copy. Only an administrator can, so Windows
# asks; a refusal or a failure stops the uninstall here, with Helio in place
# to turn hosting off from.
function Disable-Hosting {
    if (-not (Get-Service HelioHost -ErrorAction SilentlyContinue)) { return }
    $engine = Join-Path $dest 'helio-host.exe'
    if (-not (Test-Path $engine)) { $engine = Join-Path $env:ProgramFiles 'Helio Host\helio-host.exe' }
    try {
        $disable = Start-Process -FilePath $engine -ArgumentList 'disable' -Verb RunAs -WindowStyle Hidden -Wait -PassThru
    } catch {
        throw "Hosting is on, and turning it off needs an administrator ($($_.Exception.Message)). Uninstall again and choose Yes, or turn hosting off in Helio first."
    }
    if ($disable.ExitCode -ne 0) {
        throw "helio-host disable failed (exit code $($disable.ExitCode)): turn hosting off in Helio, then uninstall again."
    }
}

# Windows holds a program's files for a moment after it quits, and a copy or
# delete straight after fails ("being used by another process"): a few tries.
function Invoke-Retrying([scriptblock]$Do) {
    for ($try = 1; ; $try++) {
        try { & $Do; return }
        catch { if ($try -ge 20) { throw }; Start-Sleep -Milliseconds 250 }
    }
}

# This script beside Helio.exe, for the entry's Uninstall to run.
function Copy-Script {
    if (-not (Test-SameFolder $PSScriptRoot $dest)) {
        Copy-Item -Force $PSCommandPath (Join-Path $dest 'install.ps1')
    }
    # An update by 0.9.4 or older copies the whole archive, Install Helio.cmd
    # too, which has no use in the installed folder.
    Remove-Item (Join-Path $dest 'Install Helio.cmd') -Force -ErrorAction SilentlyContinue
}

# The entry in Settings > Apps, from what is in the folder now.
function Write-Entry {
    $exe = Join-Path $dest 'Helio.exe'
    $powershell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $size = (Get-ChildItem $dest -File | Measure-Object -Property Length -Sum).Sum
    New-Item -Path $entry -Force | Out-Null
    $strings = [ordered]@{
        DisplayName     = 'Helio'
        DisplayVersion  = "$((Get-Item $exe).VersionInfo.ProductVersion)"
        Publisher       = 'Helio'
        DisplayIcon     = $exe
        InstallLocation = $dest
        InstallDate     = Get-Date -Format 'yyyyMMdd'
        URLInfoAbout    = 'https://helio.mihaicristian.dev'
        # Its window stays open to show how it went.
        UninstallString = '"{0}" -NoProfile -ExecutionPolicy Bypass -NoExit -File "{1}" -Uninstall' -f
            $powershell, (Join-Path $dest 'install.ps1')
    }
    foreach ($name in $strings.Keys) {
        New-ItemProperty -Path $entry -Name $name -Value $strings[$name] -PropertyType String -Force | Out-Null
    }
    $numbers = [ordered]@{ EstimatedSize = [int]($size / 1KB); NoModify = 1; NoRepair = 1 }
    foreach ($name in $numbers.Keys) {
        New-ItemProperty -Path $entry -Name $name -Value $numbers[$name] -PropertyType DWord -Force | Out-Null
    }
}

if ($Uninstall) {
    Disable-Hosting
    Stop-Helio
    Remove-Item $link -Force -ErrorAction SilentlyContinue
    Remove-Item $entry -Recurse -Force -ErrorAction SilentlyContinue
    # Not from inside the folder that goes.
    Set-Location $env:TEMP
    if (Test-Path $dest) { Invoke-Retrying { Remove-Item $dest -Recurse -Force } }
    "Helio is uninstalled. Your hosts and settings stay in $env:APPDATA\Helio, and the host's identity and pairings in $env:ProgramData\Helio Host: delete those folders to remove them too."
    return
}

if ($Register) {
    if (Test-SameFolder $Register $dest) {
        Copy-Script
        Write-Entry
    }
    return
}

$from = (Resolve-Path $Source).Path
$files = @(Get-ChildItem $from -File | Where-Object { Test-Ours $_.Name })
if (-not ($files | Where-Object { $_.Name -eq 'Helio.exe' })) {
    throw "There is no Helio.exe in $from. Run this from the unpacked download, or pass -Source with Helio's folder."
}
New-Item -ItemType Directory -Force $dest | Out-Null
if (-not (Test-SameFolder $from $dest)) {
    Stop-Helio
    foreach ($file in $files) {
        Invoke-Retrying { Copy-Item -Force $file.FullName $dest }
    }
    # What an older release left: FFmpeg's DLLs of another version, and
    # files its update renamed aside (Helio.exe.old, Helio.exe.2.old).
    Get-ChildItem $dest -File | Where-Object {
        ((Test-Ours $_.Name) -and ($files.Name -notcontains $_.Name)) -or
            (($_.Name -match '\.old$') -and (Test-Ours ($_.Name -replace '(\.\d+)?\.old$', '')))
    } | Remove-Item -Force
}
# A download's files carry the internet's mark, and SmartScreen would
# question Helio at its first start: the copies are trusted as this
# installer is.
Get-ChildItem $dest -File | Unblock-File
Copy-Script

$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($link)
$shortcut.TargetPath = Join-Path $dest 'Helio.exe'
$shortcut.WorkingDirectory = $dest
$shortcut.Save()
Write-Entry

"Helio {0} is installed in {1}: open it from the Start menu. Settings > Apps uninstalls it." -f
    (Get-Item (Join-Path $dest 'Helio.exe')).VersionInfo.ProductVersion, $dest
if (-not $NoStart) { Start-Process (Join-Path $dest 'Helio.exe') -WorkingDirectory $dest }
