# Install Ping for whoever runs this: Ping.exe, the host's engine (pong.exe)
# and FFmpeg's DLLs in %LOCALAPPDATA%\Programs\Ping, Ping in the Start menu,
# and Ping in Windows' list of installed apps (Settings > Apps), whose
# Uninstall runs this script, copied there as install.ps1, with -Uninstall.
# Ping is per user, as its data is: no administrator is needed. Hosting
# stays off: Ping's switch turns it on, which copies pong.exe to
# C:\Program Files\Pong and runs it as PongService (Windows asks for an
# administrator then).
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File install-ping.ps1 [-Source DIR] [-NoStart]
#   powershell -NoProfile -ExecutionPolicy Bypass -File install-ping.ps1 -Uninstall
#
# In a release archive (tools/package-windows.ps1) this script is
# install.ps1, beside Ping.exe, and takes the files from there; in a
# checkout, from target\Ping (tools/build-ping-win.ps1 -Install runs it).
# Ping's own update (pingpong-update) replaces the files itself and then runs
# the new archive's install.ps1 with -Register FOLDER: when FOLDER is the one
# this installs into, the script there and the entry in the list are brought
# up to date; a copy anywhere else has no entry and is left as it is. Ping
# runs the install.ps1 beside it the same way when it starts and finds its
# entry missing or naming another version (an update by an older release,
# which runs no installer).
#
# Uninstalling turns hosting off first, if it is on (`pong disable`: the
# service, its firewall rules and the engine's copy in Program Files go;
# Windows asks for an administrator). It leaves Ping's hosts and settings
# (%APPDATA%\Ping), the host's identity and pairings (C:\ProgramData\Pong),
# and the Windows Firewall rules Windows made when it asked about Ping.exe:
# only an administrator can remove those, and they let nothing in once the
# program is gone.
param(
    [string]$Source = $(if (Test-Path (Join-Path $PSScriptRoot 'Ping.exe')) { $PSScriptRoot }
        else { Join-Path $PSScriptRoot '..\target\Ping' }),
    [switch]$NoStart,
    [switch]$Uninstall,
    [string]$Register
)
$ErrorActionPreference = 'Stop'

$dest = Join-Path $env:LOCALAPPDATA 'Programs\Ping'
$link = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\Ping.lnk'
# The entry in Settings > Apps, named as the Mac app's bundle identifier is:
# no other program's entry has that name.
$entry = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\dev.pingpong.Ping'

# Ping's own files, as pingpong-update names them (`ours`): only these are
# copied, replaced or removed.
function Test-Ours([string]$Name) {
    $Name -match '^(ping\.exe|pong\.exe|ffmpeg license\.txt|(avcodec|avutil|swresample)-\d+\.dll)$'
}

function Test-SameFolder([string]$A, [string]$B) {
    [IO.Path]::GetFullPath($A).TrimEnd('\') -eq [IO.Path]::GetFullPath($B).TrimEnd('\')
}

# The Ping that runs from the installed folder. Windows' own ping.exe has
# the same process name: the path tells them apart.
function Stop-Ping {
    $running = @(Get-Process Ping -ErrorAction SilentlyContinue |
        Where-Object { $_.Path -and $_.Path.StartsWith("$dest\", [StringComparison]::OrdinalIgnoreCase) })
    if ($running.Count -eq 0) { return }
    $running | Stop-Process -Force -ErrorAction SilentlyContinue
    $running | Wait-Process -Timeout 15 -ErrorAction SilentlyContinue
}

# Hosting off, while it is on (PongService is there): with the installed
# engine, else the service's own copy. Only an administrator can, so Windows
# asks; a refusal or a failure stops the uninstall here, with Ping in place
# to turn hosting off from.
function Disable-Hosting {
    if (-not (Get-Service PongService -ErrorAction SilentlyContinue)) { return }
    $pong = Join-Path $dest 'pong.exe'
    if (-not (Test-Path $pong)) { $pong = Join-Path $env:ProgramFiles 'Pong\pong.exe' }
    try {
        $disable = Start-Process -FilePath $pong -ArgumentList 'disable' -Verb RunAs -WindowStyle Hidden -Wait -PassThru
    } catch {
        throw "Hosting is on, and turning it off needs an administrator ($($_.Exception.Message)). Uninstall again and choose Yes, or turn hosting off in Ping first."
    }
    if ($disable.ExitCode -ne 0) {
        throw "pong disable failed (exit code $($disable.ExitCode)): turn hosting off in Ping, then uninstall again."
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

# This script beside Ping.exe, for the entry's Uninstall to run.
function Copy-Script {
    if (-not (Test-SameFolder $PSScriptRoot $dest)) {
        Copy-Item -Force $PSCommandPath (Join-Path $dest 'install.ps1')
    }
    # An update by 0.9.4 or older copies the whole archive, Install Ping.cmd
    # too, which has no use in the installed folder.
    Remove-Item (Join-Path $dest 'Install Ping.cmd') -Force -ErrorAction SilentlyContinue
}

# The entry in Settings > Apps, from what is in the folder now.
function Write-Entry {
    $exe = Join-Path $dest 'Ping.exe'
    $powershell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $size = (Get-ChildItem $dest -File | Measure-Object -Property Length -Sum).Sum
    New-Item -Path $entry -Force | Out-Null
    $strings = [ordered]@{
        DisplayName     = 'Ping'
        DisplayVersion  = "$((Get-Item $exe).VersionInfo.ProductVersion)"
        Publisher       = 'pingpong'
        DisplayIcon     = $exe
        InstallLocation = $dest
        InstallDate     = Get-Date -Format 'yyyyMMdd'
        URLInfoAbout    = 'https://pingpong.mihaicristian.dev'
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
    Stop-Ping
    Remove-Item $link -Force -ErrorAction SilentlyContinue
    Remove-Item $entry -Recurse -Force -ErrorAction SilentlyContinue
    # Not from inside the folder that goes.
    Set-Location $env:TEMP
    if (Test-Path $dest) { Invoke-Retrying { Remove-Item $dest -Recurse -Force } }
    "Ping is uninstalled. Your hosts and settings stay in $env:APPDATA\Ping, and the host's identity and pairings in $env:ProgramData\Pong: delete those folders to remove them too."
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
if (-not ($files | Where-Object { $_.Name -eq 'Ping.exe' })) {
    throw "There is no Ping.exe in $from. Run this from the unpacked download, or pass -Source with Ping's folder."
}
New-Item -ItemType Directory -Force $dest | Out-Null
if (-not (Test-SameFolder $from $dest)) {
    Stop-Ping
    foreach ($file in $files) {
        Invoke-Retrying { Copy-Item -Force $file.FullName $dest }
    }
    # What an older release left: FFmpeg's DLLs of another version, and
    # files its update renamed aside (Ping.exe.old, Ping.exe.2.old).
    Get-ChildItem $dest -File | Where-Object {
        ((Test-Ours $_.Name) -and ($files.Name -notcontains $_.Name)) -or
            (($_.Name -match '\.old$') -and (Test-Ours ($_.Name -replace '(\.\d+)?\.old$', '')))
    } | Remove-Item -Force
}
# A download's files carry the internet's mark, and SmartScreen would
# question Ping at its first start: the copies are trusted as this
# installer is.
Get-ChildItem $dest -File | Unblock-File
Copy-Script

$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($link)
$shortcut.TargetPath = Join-Path $dest 'Ping.exe'
$shortcut.WorkingDirectory = $dest
$shortcut.Save()
Write-Entry

"Ping {0} is installed in {1}: open it from the Start menu. Settings > Apps uninstalls it." -f
    (Get-Item (Join-Path $dest 'Ping.exe')).VersionInfo.ProductVersion, $dest
if (-not $NoStart) { Start-Process (Join-Path $dest 'Ping.exe') -WorkingDirectory $dest }
