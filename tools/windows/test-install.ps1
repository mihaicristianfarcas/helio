# Install Helio with its installer, turn hosting on, install it again over
# itself, uninstall it, and check after each step that it did what it says
# and, at the end, that nothing of it is left but the data Helio keeps on
# purpose. The release workflow runs it on each Windows package; it runs
# the same on a PC or a VM of your own.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools\windows\test-install.ps1 -Setup PATH [-Signed] [-UnsignedEngine PATH]
#
# As an administrator (hosting on and off need one; a GitHub runner is one,
# and Windows then asks nothing), for a user with no Helio installed and a
# PC with hosting off: it installs into that user's
# %LOCALAPPDATA%\Programs\Helio, silently, as tools\windows\helio.iss does
# for anyone. What only a person can check is not here: Windows' own UAC
# prompts as a user who is not an administrator, SmartScreen, and Settings
# > Apps' Uninstall button (which runs the same uninstaller).
#
# -Signed: every signature, the installer's, the uninstaller's and those of
# every program and DLL installed, verified with `signtool verify /pa /v`
# (the Windows SDK's, or SIGNTOOL) and timestamped. -UnsignedEngine PATH: helio-host.exe
# as built, before it was signed; with it, an engine built with
# `require-signature` must refuse to turn hosting on from an unsigned copy
# and from a signed copy changed after signing, and change nothing.
param(
    [Parameter(Mandatory)] [string]$Setup,
    [switch]$Signed,
    [string]$UnsignedEngine
)
$ErrorActionPreference = 'Stop'

$app = Join-Path $env:LOCALAPPDATA 'Programs\Helio'
$link = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\Helio.lnk'
$entry = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\dev.mihaicristian.helio_is1'
$hostDir = Join-Path $env:ProgramFiles 'Helio Host'
$work = Join-Path ([IO.Path]::GetTempPath()) "helio-test-install-$PID"

function Check([bool]$ok, [string]$what) {
    if (-not $ok) { throw "FAILED: $what" }
    Write-Output "ok: $what"
}

function Hosting-On { [bool](Get-Service HelioHost -ErrorAction SilentlyContinue) }

# A program's exit code and what it printed, stdout and stderr together.
# Not under 'Stop': Windows PowerShell makes a native program's stderr an
# error then.
function Invoke-Program([string]$exe, [string[]]$arguments) {
    $ErrorActionPreference = 'Continue'
    $said = & $exe @arguments 2>&1 | ForEach-Object { "$_" }
    [pscustomobject]@{ Code = $LASTEXITCODE; Said = ($said -join "`n") }
}

# Run an Inno Setup program silently and wait for it to be done. The
# uninstaller copies itself to the temporary folder and the copy does the
# work: waiting for the first one is not enough.
function Invoke-Silently([string]$exe) {
    $process = Start-Process -FilePath $exe -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART' -PassThru
    # Held from the start: without the handle, a process that has exited
    # has no exit code to give.
    $null = $process.Handle
    $process.WaitForExit()
    Get-Process -ErrorAction SilentlyContinue |
        Where-Object { $_.Path -and $_.Path -like '*\is-*.tmp\*' } |
        ForEach-Object { $_.WaitForExit() }
    $process.ExitCode
}

function User-Path {
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment')
    try { [string]$key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames) }
    finally { $key.Close() }
}

function Set-UserPath([string]$path) {
    $key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Environment')
    try {
        if ($path) { $key.SetValue('Path', $path, [Microsoft.Win32.RegistryValueKind]::ExpandString) }
        else { $key.DeleteValue('Path', $false) }
    } finally { $key.Close() }
}

function Signtool {
    if ($env:SIGNTOOL) { return $env:SIGNTOOL }
    $kits = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
    $found = Get-ChildItem $kits -Filter signtool.exe -Recurse -ErrorAction SilentlyContinue |
        Where-Object { $_.Directory.Name -eq 'x64' } | Sort-Object FullName -Descending | Select-Object -First 1
    if (-not $found) { throw "-Signed needs signtool.exe, the Windows SDK's (in $kits)" }
    $found.FullName
}

# A valid, trusted, timestamped Authenticode signature on $file.
function Check-Signature([string]$signtool, [string]$file) {
    $verify = Invoke-Program $signtool @('verify', '/pa', '/v', $file)
    Check ($verify.Code -eq 0 -and $verify.Said -match 'Successfully verified') "signtool verify /pa /v: $file"
    Check ($verify.Said -match 'The signature is timestamped') "timestamped: $file"
}

# The engine at $exe, asked to turn hosting on, is refused because of
# $why, and nothing changes.
function Check-Refused([string]$exe, [string]$why, [string]$what) {
    $enable = Invoke-Program $exe @('enable')
    Write-Output $enable.Said
    Check ($enable.Code -eq 1 -and $enable.Said -match [regex]::Escape($why)) "$what is refused: $why"
    Check (-not (Hosting-On)) "$what leaves hosting off"
    Check (-not (Test-Path $hostDir)) "$what leaves nothing in Program Files"
}

$principal = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Run this as an administrator: hosting on and off need one.'
}
if ((Test-Path $app) -or (Test-Path $entry) -or (Hosting-On) -or (Test-Path $hostDir)) {
    throw "Helio is installed here, or hosting is on: run this for a user and on a PC without them ($app, $hostDir)."
}
$Setup = (Resolve-Path $Setup).Path
New-Item -ItemType Directory -Force $work | Out-Null
$pathBefore = User-Path
try {
    # Installed: the files, the Start menu, Settings > Apps; hosting off.
    Check ((Invoke-Silently $Setup) -eq 0) 'the installer exits with 0'
    foreach ($file in 'Helio.exe', 'helio-host.exe', 'bin\helio.exe', 'FFmpeg LICENSE.txt', 'unins000.exe') {
        Check (Test-Path (Join-Path $app $file)) "installed: $file"
    }
    foreach ($dll in 'avcodec', 'avutil', 'swresample') {
        Check (@(Get-ChildItem $app -Filter "$dll-*.dll").Count -eq 1) "installed: $dll-N.dll"
    }
    foreach ($gone in 'install.ps1', 'Install Helio.cmd') {
        Check (-not (Test-Path (Join-Path $app $gone))) "no $gone"
    }
    Check (Test-Path $link) 'Helio in the Start menu'
    $listed = Get-ItemProperty $entry
    $version = (Get-Item (Join-Path $app 'Helio.exe')).VersionInfo.ProductVersion
    Check ($listed.DisplayName -eq 'Helio' -and $listed.Publisher -eq 'Helio' -and $listed.DisplayVersion -eq $version) `
        "listed in Settings > Apps: $($listed.DisplayName) $($listed.DisplayVersion), $($listed.Publisher)"
    Check (-not (Hosting-On)) 'hosting stays off after the install'

    if ($Signed) {
        $signtool = Signtool
        Check-Signature $signtool $Setup
        Get-ChildItem $app -Recurse -Include '*.exe', '*.dll' | ForEach-Object { Check-Signature $signtool $_.FullName }
    }

    if ($UnsignedEngine) {
        $unsigned = Join-Path $work 'unsigned\helio-host.exe'
        New-Item -ItemType Directory (Split-Path $unsigned) | Out-Null
        Copy-Item $UnsignedEngine $unsigned
        Check-Refused $unsigned 'it is not signed' 'an unsigned engine'
        # One letter of the DOS stub's message ("This program cannot be run
        # in DOS mode."): inside what the signature covers, and never run,
        # so the changed engine still starts and checks itself.
        $changed = Join-Path $work 'changed\helio-host.exe'
        New-Item -ItemType Directory (Split-Path $changed) | Out-Null
        $bytes = [IO.File]::ReadAllBytes((Join-Path $app 'helio-host.exe'))
        $stub = [Text.Encoding]::ASCII.GetString($bytes, 0, 512).IndexOf('DOS mode')
        Check ($stub -gt 0) 'the engine has a DOS stub to change'
        $bytes[$stub] = [byte][char]'d'
        [IO.File]::WriteAllBytes($changed, $bytes)
        Check-Refused $changed 'it was changed after it was signed' 'an engine changed after signing'
    }

    # Hosting on, from the installed engine, as Helio's switch does it.
    $enable = Invoke-Program (Join-Path $app 'helio-host.exe') @('enable')
    Write-Output $enable.Said
    Check ($enable.Code -eq 0 -and (Hosting-On)) 'hosting turns on'
    $engine = Join-Path $hostDir 'helio-host.exe'
    Check (Test-Path $engine) "the engine's copy is in $hostDir"
    Check ([bool](Get-NetFirewallRule -DisplayName 'Helio Host' -ErrorAction SilentlyContinue)) "the host's firewall rules are there"
    $copied = (Get-Item $engine).LastWriteTimeUtc

    # Installed again while hosting is on, as an upgrade is: the host is
    # put on the new engine (the installer runs helio-host enable again).
    Start-Sleep -Seconds 2
    Check ((Invoke-Silently $Setup) -eq 0) 'the installer, run again with hosting on, exits with 0'
    Check (Hosting-On) 'hosting stays on'
    Check ((Get-Item $engine).LastWriteTimeUtc -gt $copied) "the host's engine is copied again"

    # What Helio makes beside its files: Settings > General's PATH entry
    # and start at login, and what an update renames aside or brings.
    Set-UserPath ((@($pathBefore -split ';' | Where-Object { $_ }) + (Join-Path $app 'bin')) -join ';')
    # A new profile may have no Run key yet: Helio creates it as this does
    # (helio-ui/src/login.rs).
    $run = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
    if (-not (Test-Path $run)) { New-Item $run -Force | Out-Null }
    New-ItemProperty $run -Name Helio -Value ('"{0}" --background' -f (Join-Path $app 'Helio.exe')) `
        -PropertyType String -Force | Out-Null
    foreach ($left in 'Helio.exe.old', 'bin\helio.exe.2.old', 'avcodec-1.dll') {
        Set-Content -LiteralPath (Join-Path $app $left) -Value 'left by an update'
    }

    # Uninstalled with hosting on: hosting goes first, then the rest. What
    # unins000.exe exits with says nothing: it hands the work to a copy of
    # itself and exits with -1 once the copy says it is done (Inno Setup
    # 6.7.1). What it did is checked instead.
    $hostData = Join-Path $env:ProgramData 'Helio Host'
    $hadData = Test-Path $hostData
    $code = Invoke-Silently (Join-Path $app 'unins000.exe')
    Write-Output "the uninstaller is done (unins000.exe exited with $code)"
    Check (-not (Hosting-On)) 'the uninstall turns hosting off'
    Check (-not (Test-Path $hostDir)) "$hostDir is gone"
    Check (-not (Get-NetFirewallRule -DisplayName 'Helio Host' -ErrorAction SilentlyContinue)) "the host's firewall rules are gone"
    Check (-not (Test-Path $app)) "$app is gone"
    Check (-not (Test-Path $link)) 'the Start menu entry is gone'
    Check (-not (Test-Path $entry)) 'the Settings > Apps entry is gone'
    $pathAfter = User-Path
    Check (-not ($pathAfter -split ';' | Where-Object { $_.TrimEnd('\') -eq (Join-Path $app 'bin') })) "the command's folder is off the PATH"
    Check (-not (Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -Name Helio -ErrorAction SilentlyContinue)) `
        'Helio no longer starts at login'
    if ($hadData) { Check (Test-Path $hostData) "the host's identity and pairings stay in $hostData" }
    Write-Output 'Helio is installed, hosts, and uninstalls with nothing left behind.'
} finally {
    # What a failed run leaves is cleaned up as far as it can be.
    if (Hosting-On) {
        $engine = if (Test-Path (Join-Path $app 'helio-host.exe')) { Join-Path $app 'helio-host.exe' } else { Join-Path $hostDir 'helio-host.exe' }
        if (Test-Path $engine) { Invoke-Program $engine @('disable') | Out-Null }
    }
    if (Test-Path (Join-Path $app 'unins000.exe')) { Invoke-Silently (Join-Path $app 'unins000.exe') | Out-Null }
    if ((User-Path) -ne $pathBefore) { Set-UserPath $pathBefore }
    Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
}
