# Install (or update) the freshly built pong.exe as the PongService, and
# Pong's window (pong-app.exe, as "Pong Control.exe") with a Start menu entry
# and its icon in the notification area from the next sign-in on (for the
# user running this; "Show Pong's icon at login" in the window turns it off),
# and Pong in Windows' list of installed apps (Settings > Apps), whose
# Uninstall runs this script, copied there as install.ps1, with -Uninstall.
#
#   powershell -File host-deploy.ps1 [-Source <path to pong.exe>] [-NoStart] [-Log <file>]
#   powershell -File host-deploy.ps1 -Uninstall
#
# In a release archive (tools/package-windows.ps1) this script is
# install.ps1, beside pong.exe and Pong Control.exe, and takes them from
# there; in a checkout, from target\release. Pong's window runs the new
# archive's install.ps1 to install an update (pingpong-update), with -Log:
# why it failed, if it does, goes there for the window to show.
#
# Uninstalling removes the service, its firewall rules and the programs, and
# leaves the host's identity, pairings and settings (C:\ProgramData\Pong,
# %APPDATA%\Pong), so that installing again keeps its clients.
param(
    [string]$Source = $(if (Test-Path (Join-Path $PSScriptRoot 'pong.exe')) { Join-Path $PSScriptRoot 'pong.exe' }
        else { Join-Path $PSScriptRoot '..\target\release\pong.exe' }),
    [string]$Window = $(if (Test-Path (Join-Path $PSScriptRoot 'Pong Control.exe')) { Join-Path $PSScriptRoot 'Pong Control.exe' }
        else { Join-Path $PSScriptRoot '..\target\release\pong-app.exe' }),
    [switch]$NoStart,
    [switch]$Uninstall,
    [string]$Log
)
$ErrorActionPreference = 'Stop'
trap {
    if ($Log) { $_.Exception.Message | Out-File -Append -Encoding utf8 -FilePath $Log }
    break
}
# A service is installed and removed by an administrator. From a PowerShell
# that is not one (the release archive's "Install Pong.cmd", Uninstall in
# Settings > Apps), start again as one, which Windows asks the user to
# allow, in a window that stays open to show how it went.
$principal = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    $again = '-NoProfile -ExecutionPolicy Bypass -NoExit -File "{0}" -Source "{1}" -Window "{2}"' -f $PSCommandPath, $Source, $Window
    if ($NoStart) { $again += ' -NoStart' }
    if ($Uninstall) { $again += ' -Uninstall' }
    if ($Log) { $again += ' -Log "{0}"' -f $Log }
    Start-Process powershell -Verb RunAs -ArgumentList $again
    return
}
$dest = 'C:\Program Files\Pong'
$startMenu = Join-Path $env:ProgramData 'Microsoft\Windows\Start Menu\Programs\Pong.lnk'
$run = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
# The entry in Settings > Apps, named as the Mac app's bundle identifier is:
# no other program's entry has that name.
$entry = 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\dev.pingpong.Pong'

# Windows holds a program's file for a moment after its process is killed,
# and a copy straight after fails ("being used by another process"): the
# installer stops a program, waits for it to be gone, and copies over it
# with a few tries.
function Stop-Program([string]$Name) {
    $running = @(Get-Process $Name -ErrorAction SilentlyContinue)
    if ($running.Count -eq 0) { return }
    $running | Stop-Process -Force -ErrorAction SilentlyContinue
    $running | Wait-Process -Timeout 15 -ErrorAction SilentlyContinue
}
function Copy-Program([string]$From, [string]$To) {
    # The installed copy of this script, run again, installs what is there.
    if ([IO.Path]::GetFullPath($From) -eq [IO.Path]::GetFullPath($To)) { return }
    for ($try = 1; ; $try++) {
        try { Copy-Item -Force $From $To; return }
        catch { if ($try -ge 20) { throw }; Start-Sleep -Milliseconds 250 }
    }
}

# The entry in Settings > Apps, from what is in Program Files now.
function Write-Entry {
    $control = Join-Path $dest 'Pong Control.exe'
    # pong.exe carries no version information; Pong's window does.
    $version = if (Test-Path $control) { "$((Get-Item $control).VersionInfo.ProductVersion)" } else { '' }
    $icon = if (Test-Path $control) { $control } else { Join-Path $dest 'pong.exe' }
    $powershell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $size = (Get-ChildItem $dest -File | Measure-Object -Property Length -Sum).Sum
    New-Item -Path $entry -Force | Out-Null
    $strings = [ordered]@{
        DisplayName     = 'Pong'
        DisplayVersion  = $version
        Publisher       = 'pingpong'
        DisplayIcon     = $icon
        InstallLocation = $dest
        InstallDate     = Get-Date -Format 'yyyyMMdd'
        URLInfoAbout    = 'https://pingpong.mihaicristian.dev'
        # Started by Settings as the user: the script asks for an
        # administrator itself, as "Install Pong.cmd" does.
        UninstallString = '"{0}" -NoProfile -ExecutionPolicy Bypass -File "{1}" -Uninstall' -f
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
    Stop-Program 'Pong Control'
    if (Get-Service PongService -ErrorAction SilentlyContinue) {
        # Stops the host (it restores the displays) and removes the service
        # and its firewall rules.
        & (Join-Path $dest 'pong.exe') uninstall
        if ($LASTEXITCODE -ne 0) { throw "pong uninstall failed (exit code $LASTEXITCODE)" }
    }
    # The host child and the clipboard helper may outlive the service by a
    # moment.
    Stop-Program pong
    Remove-Item $startMenu -Force -ErrorAction SilentlyContinue
    Remove-ItemProperty -Path $run -Name 'Pong' -ErrorAction SilentlyContinue
    Remove-Item $entry -Recurse -Force -ErrorAction SilentlyContinue
    # Not from inside the folder that goes.
    Set-Location $env:SystemRoot
    for ($try = 1; Test-Path $dest; $try++) {
        try { Remove-Item $dest -Recurse -Force }
        catch { if ($try -ge 20) { throw }; Start-Sleep -Milliseconds 250 }
    }
    "Pong is uninstalled. Its identity, pairings and settings stay in C:\ProgramData\Pong and $env:APPDATA\Pong: delete those folders to remove them too."
    return
}

New-Item -ItemType Directory -Force -Path $dest | Out-Null
$svc = Get-Service PongService -ErrorAction SilentlyContinue
if ($svc -and $svc.Status -ne 'Stopped') {
    Stop-Service PongService -Force
    $svc.WaitForStatus('Stopped', '00:00:30')
}
try {
    # The host child may outlive a forced stop by a moment.
    Stop-Program pong
    Copy-Program $Source (Join-Path $dest 'pong.exe')
    if (Test-Path $Window) {
        Stop-Program 'Pong Control'
        Copy-Program $Window (Join-Path $dest 'Pong Control.exe')
    }
} catch {
    # Leave the host running, on whichever pong.exe is in place, rather than
    # stopped until someone notices.
    if ($svc) { Start-Service PongService -ErrorAction SilentlyContinue }
    throw
}
# A download's files carry the internet's mark, and SmartScreen would
# question Pong's window at its first start: the copies are trusted as this
# installer is.
Unblock-File (Join-Path $dest 'pong.exe')
if (Test-Path $Window) {
    $control = Join-Path $dest 'Pong Control.exe'
    Unblock-File $control
    # For everyone on this PC: Start menu, "Pong".
    $shell = New-Object -ComObject WScript.Shell
    $link = $shell.CreateShortcut($startMenu)
    $link.TargetPath = $control
    $link.WorkingDirectory = $dest
    $link.Save()
    # Pong's icon at sign-in, without its window: this user's Run key, the
    # value the window's own setting reads and writes. Not started from here:
    # this shell is elevated, and the icon is the signed-in user's.
    Set-ItemProperty -Path $run -Name 'Pong' -Value ('"{0}" --background' -f $control)
}
# This script beside the programs, for the entry's Uninstall to run.
Copy-Program $PSCommandPath (Join-Path $dest 'install.ps1')

& (Join-Path $dest 'pong.exe') install
if ($LASTEXITCODE -ne 0) { throw "pong install failed (exit code $LASTEXITCODE)" }
Write-Entry
if ($NoStart) { Stop-Service PongService -Force }
Get-Service PongService | Format-Table Name, Status, StartType
