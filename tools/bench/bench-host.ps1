# bench-host.ps1: the Windows host's side of a benchmark run, driven over SSH
# by tools/bench/run-mac.
#
#   bench-host.ps1 stack helio|sunshine|apollo|none
#       Runs only that host: the others' services stop, so they cannot
#       compete for the GPU's encoder or the network.
#   bench-host.ps1 scene BASE64
#       Starts tools/bench/scene with the arguments BASE64 decodes to (UTF-8)
#       in the logged-on user's desktop,
#       NOT elevated, as a game runs: every host's injected input then reaches
#       it the same way (Windows drops input sent to a more elevated window).
#   bench-host.ps1 display-sleep
#       Puts the monitors to sleep, as the idle timer does.
#   bench-host.ps1 monitor
#       Each monitor's power as it reports it (DDC/CI): whether it is lit.
#   bench-host.ps1 unscene
#   bench-host.ps1 encryption on|off
#       Sunshine and Apollo encrypt video and audio on the LAN, or go back to
#       their default (not on the LAN). Takes effect when the service starts.
#   bench-host.ps1 mode WxH@HZ
#       Sets the primary display's mode (tools/set-resolution.ps1, copied
#       beside this script) in the user's session; for runs where the host's
#       monitor should refresh at the stream's rate.
#   bench-host.ps1 sample SECONDS OUT
#       GPU load, encoder load, power and clock every 500 ms (nvidia-smi), and
#       each host process's CPU time and memory at both ends, as CSV.
#   bench-host.ps1 rig [COMMIT]
#       What the host is and how it is set, as key=value lines, for the run's
#       manifest and the pre-flight; COMMIT is looked for in the installed Helio.
#
# The scene's arguments come base64-encoded because `powershell -File` takes
# any argument starting with a dash for a parameter of this script.
#
# SSH lands in session 0, where no desktop is drawn: the scene starts through
# a one-shot scheduled task in the user's session, as tools/host-run.ps1 does.
param(
    [Parameter(Mandatory = $true, Position = 0)][string]$Action,
    [Parameter(Position = 1)][string]$Arg1 = '',
    [Parameter(Position = 2)][string]$Arg2 = ''
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$scene = Join-Path $here 'scene\target\release\bench-scene.exe'
$services = [ordered]@{ helio = 'HelioHost'; sunshine = 'SunshineService'; apollo = 'ApolloService' }
# Sunshine's and Apollo's settings files, in their installers' default places.
$confs = 'C:\Program Files\Sunshine\config\sunshine.conf', 'C:\Program Files\Apollo\config\sunshine.conf'
# The processes each host runs, for CPU and memory.
$hostProcesses = 'helio-host', 'sunshine', 'sunshinesvc', 'bench-scene', 'dwm'

# Each monitor's power as the monitor itself reports it over DDC/CI (VCP
# 0xD6: 1 on, 2 standby, 3 suspend, 4 off by its own button, 5 off by its
# switch), so a run says whether the host's monitor was lit: Windows lists a
# monitor switched off at its button as it lists a lit one. Read in the
# user's session, where the monitors are; a display that does not answer
# (a virtual one, say) gives "no reply".
function Get-MonitorPower {
    $user = (Get-CimInstance Win32_ComputerSystem).UserName
    $ps1 = Join-Path $here 'monitor-power.ps1'
    $txt = Join-Path $here 'runs\monitor-power.txt'
    Remove-Item $txt -ErrorAction SilentlyContinue
    Set-Content -Path $ps1 -Value @'
param($Out)
Add-Type -TypeDefinition @"
using System; using System.Collections.Generic; using System.Runtime.InteropServices;
public static class Ddc {
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    public struct PhysicalMonitor { public IntPtr Handle; [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)] public string Name; }
    delegate bool EnumProc(IntPtr monitor, IntPtr dc, IntPtr rect, IntPtr data);
    [DllImport("user32.dll")] static extern bool EnumDisplayMonitors(IntPtr dc, IntPtr clip, EnumProc proc, IntPtr data);
    [DllImport("dxva2.dll")] static extern bool GetNumberOfPhysicalMonitorsFromHMONITOR(IntPtr monitor, out uint count);
    [DllImport("dxva2.dll")] static extern bool GetPhysicalMonitorsFromHMONITOR(IntPtr monitor, uint count, [Out] PhysicalMonitor[] monitors);
    [DllImport("dxva2.dll")] static extern bool GetVCPFeatureAndVCPFeatureReply(IntPtr handle, byte code, IntPtr type, out uint current, out uint max);
    [DllImport("dxva2.dll")] static extern bool DestroyPhysicalMonitors(uint count, PhysicalMonitor[] monitors);
    public static string Query() {
        var lines = new List<string>();
        EnumDisplayMonitors(IntPtr.Zero, IntPtr.Zero, (m, dc, r, d) => {
            uint n;
            if (!GetNumberOfPhysicalMonitorsFromHMONITOR(m, out n) || n == 0) return true;
            var a = new PhysicalMonitor[n];
            if (!GetPhysicalMonitorsFromHMONITOR(m, n, a)) return true;
            foreach (var p in a) {
                uint cur, max;
                lines.Add(p.Name + (GetVCPFeatureAndVCPFeatureReply(p.Handle, 0xD6, IntPtr.Zero, out cur, out max) ? ": power " + cur : ": no reply"));
            }
            DestroyPhysicalMonitors(n, a);
            return true;
        }, IntPtr.Zero);
        return string.Join("; ", lines);
    }
}
"@
[Ddc]::Query() | Set-Content -Path $Out
'@
    $taskAction = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument "-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File `"$ps1`" -Out `"$txt`""
    $principal = New-ScheduledTaskPrincipal -UserId $user -LogonType Interactive -RunLevel Limited
    Register-ScheduledTask -TaskName 'bench-monitor-power' -Action $taskAction -Principal $principal -Force | Out-Null
    Start-ScheduledTask -TaskName 'bench-monitor-power'
    # A task says nothing when it ends: look at its state.
    Start-Sleep -Milliseconds 300
    foreach ($i in 1..50) {
        if ((Get-ScheduledTask -TaskName 'bench-monitor-power').State -ne 'Running') { break }
        Start-Sleep -Milliseconds 200
    }
    if (Test-Path $txt) { (Get-Content $txt) -join '; ' } else { 'unknown (no answer from the user session)' }
}

switch ($Action) {
    'stack' {
        $want = $services[$Arg1]
        if (-not $want -and $Arg1 -ne 'none') { throw "unknown stack $Arg1" }
        foreach ($s in $services.Values) {
            if ($s -ne $want) { Stop-Service $s -Force -ErrorAction SilentlyContinue }
        }
        if ($want) { Start-Service $want }
        ($services.Values | ForEach-Object { "$_ $((Get-Service $_).Status)" }) -join ', '
    }
    'encryption' {
        # Sunshine and Apollo encrypt video and audio on the LAN only when
        # asked (lan_encryption_mode = 2: required); Helio always does.
        if ($Arg1 -notin 'on', 'off') { throw 'encryption on|off' }
        foreach ($conf in $confs) {
            $lines = @(Get-Content $conf | Where-Object { $_ -notmatch '^\s*lan_encryption_mode\s*=' })
            if ($Arg1 -eq 'on') { $lines += 'lan_encryption_mode = 2' }
            Set-Content -Path $conf -Value $lines
            "$conf`: $(@(Select-String -Path $conf -Pattern '^lan_encryption_mode').Line)"
        }
    }
    'scene' {
        Get-Process bench-scene -ErrorAction SilentlyContinue | Stop-Process -Force
        $user = (Get-CimInstance Win32_ComputerSystem).UserName
        if (-not $user) { throw 'nobody is logged on to the console' }
        $sceneArgs = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($Arg1))
        $taskAction = New-ScheduledTaskAction -Execute $scene -Argument $sceneArgs
        $principal = New-ScheduledTaskPrincipal -UserId $user -LogonType Interactive -RunLevel Limited
        $settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries `
            -ExecutionTimeLimit (New-TimeSpan -Hours 0) -MultipleInstances IgnoreNew
        Register-ScheduledTask -TaskName 'bench-scene' -Action $taskAction -Principal $principal -Settings $settings -Force | Out-Null
        Start-ScheduledTask -TaskName 'bench-scene'
        # The task has no event for "the window is up": look for the process.
        foreach ($i in 1..50) {
            if (Get-Process bench-scene -ErrorAction SilentlyContinue) { 'scene up'; return }
            Start-Sleep -Milliseconds 100
        }
        throw 'the scene did not start'
    }
    'mode' {
        if ($Arg1 -notmatch '^(\d+)x(\d+)@(\d+)$') { throw 'mode is WxH@HZ' }
        $w, $h, $hz = $Matches[1], $Matches[2], $Matches[3]
        $log = Join-Path $here 'runs\mode.log'
        Remove-Item $log -ErrorAction SilentlyContinue
        $user = (Get-CimInstance Win32_ComputerSystem).UserName
        $set = Join-Path $here 'set-resolution.ps1'
        $taskAction = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument (
            "-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -Command `"& '$set' -Width $w -Height $h -RefreshHz $hz *> '$log'`"")
        $principal = New-ScheduledTaskPrincipal -UserId $user -LogonType Interactive -RunLevel Limited
        Register-ScheduledTask -TaskName 'bench-mode' -Action $taskAction -Principal $principal -Force | Out-Null
        Start-ScheduledTask -TaskName 'bench-mode'
        # A task says nothing when it ends: look at its state.
        Start-Sleep -Milliseconds 300
        foreach ($i in 1..100) {
            if ((Get-ScheduledTask -TaskName 'bench-mode').State -ne 'Running') { break }
            Start-Sleep -Milliseconds 200
        }
        if (Test-Path $log) { Get-Content $log | Select-Object -Last 1 }
    }
    'monitor' { Get-MonitorPower }
    'display-sleep' {
        # Puts the monitors to sleep as Windows' idle timer does
        # (SC_MONITORPOWER 2), from the user's session: broadcast from
        # session 0 it reaches no desktop. Posted, not sent: a hung window
        # would hold a sent broadcast.
        $user = (Get-CimInstance Win32_ComputerSystem).UserName
        $ps1 = Join-Path $here 'display-sleep.ps1'
        Set-Content -Path $ps1 -Value @'
Add-Type -Namespace B -Name U -MemberDefinition '[DllImport("user32.dll")] public static extern bool PostMessage(System.IntPtr h, uint m, System.IntPtr w, System.IntPtr l);'
[B.U]::PostMessage([System.IntPtr]0xFFFF, 0x0112, [System.IntPtr]0xF170, [System.IntPtr]2) | Out-Null
'@
        $taskAction = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument "-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File `"$ps1`""
        $principal = New-ScheduledTaskPrincipal -UserId $user -LogonType Interactive -RunLevel Limited
        Register-ScheduledTask -TaskName 'bench-display-sleep' -Action $taskAction -Principal $principal -Force | Out-Null
        Start-ScheduledTask -TaskName 'bench-display-sleep'
        'display sleep posted'
    }
    'unscene' {
        Get-Process bench-scene -ErrorAction SilentlyContinue | Stop-Process -Force
        'scene down'
    }
    'sample' {
        $seconds = [int]$Arg1
        $out = $Arg2
        $cpu = {
            Get-Process -Name $hostProcesses -ErrorAction SilentlyContinue |
                ForEach-Object { "proc,$($_.Name),$($_.Id),$($_.SessionId),$($_.CPU),$($_.WorkingSet64),$($_.PrivateMemorySize64)" }
        }
        $t0 = Get-Date
        $before = & $cpu | ForEach-Object { $_ -replace '^proc', 'before' }
        $smi = Join-Path $env:TEMP 'bench-smi.csv'
        $p = Start-Process -FilePath nvidia-smi -PassThru -WindowStyle Hidden -RedirectStandardOutput $smi -ArgumentList @(
            '--query-gpu=timestamp,utilization.gpu,utilization.encoder,utilization.decoder,power.draw,clocks.gr,memory.used',
            '--format=csv,noheader,nounits', '-lms', '500')
        Start-Sleep -Seconds $seconds
        Stop-Process -Id $p.Id -Force
        $after = & $cpu | ForEach-Object { $_ -replace '^proc', 'after' }
        $elapsed = ((Get-Date) - $t0).TotalSeconds
        $lines = @("elapsed,$elapsed,$([Environment]::ProcessorCount)") + $before + $after +
            (Get-Content $smi | ForEach-Object { "gpu,$_" })
        Set-Content -Path $out -Value $lines -Encoding ASCII
        "sampled $elapsed s"
    }
    'rig' {
        # Read from the machine each time, never written down.
        $os = Get-CimInstance Win32_OperatingSystem
        $ubr = (Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion').UBR
        "host_os=$($os.Caption) $($os.Version).$ubr"
        "host_cpu=$((Get-CimInstance Win32_Processor | Select-Object -First 1).Name.Trim())"
        "host_memory_gb=$([math]::Round($os.TotalVisibleMemorySize / 1MB))"
        $smi = (& nvidia-smi --query-gpu=name,driver_version --format=csv,noheader 2>$null) -split ',\s*'
        "host_gpu=$($smi[0])"
        "host_driver=$($smi[1])"
        $hags = (Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\GraphicsDrivers' -ErrorAction SilentlyContinue).HwSchMode
        "host_hags=$(if ($hags -eq 2) { 'on' } elseif ($hags -eq 1) { 'off' } else { 'default' })"
        Get-CimInstance Win32_VideoController | Where-Object { $_.CurrentHorizontalResolution } | ForEach-Object {
            "host_display=$($_.Name): $($_.CurrentHorizontalResolution)x$($_.CurrentVerticalResolution)@$($_.CurrentRefreshRate)"
        }
        "host_monitor_power=$(Get-MonitorPower)"
        "host_power_plan=$(((powercfg /getactivescheme) -replace '^.*\((.*)\).*$', '$1').Trim())"
        Get-NetAdapter -Physical | ForEach-Object { "host_link=$($_.Name): $($_.Status) $($_.LinkSpeed)" }
        foreach ($s in $services.Values) {
            $svc = Get-Service $s -ErrorAction SilentlyContinue
            "host_service=$s $($svc.Status) $($svc.StartType)"
        }
        $keys = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*', 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*',
            'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*'
        Get-ItemProperty $keys -ErrorAction SilentlyContinue | Where-Object { $_.DisplayName -match '^(Helio|Apollo|Sunshine)$' } |
            ForEach-Object { "host_version_$($_.DisplayName.ToLower())=$($_.DisplayVersion)" }
        # The app carries its commit as text (helio-update's build script);
        # the engine does not, so its hash identifies it.
        $app = Join-Path $env:LOCALAPPDATA 'Programs\Helio\Helio.exe'
        if ($Arg1 -and (Test-Path $app)) {
            "host_helio_commit_found=$(Select-String -Path $app -Pattern $Arg1 -SimpleMatch -Quiet)"
        }
        $engine = 'C:\Program Files\Helio Host\helio-host.exe'
        if (Test-Path $engine) { "host_helio_engine_sha256=$((Get-FileHash -Algorithm SHA256 $engine).Hash.ToLower())" }
        $svcEnv = (Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Services\HelioHost' -Name Environment -ErrorAction SilentlyContinue).Environment
        "host_rust_log=$([Environment]::GetEnvironmentVariable('RUST_LOG', 'User'))$(if ($svcEnv -match 'RUST_LOG') { ' service:' + ($svcEnv -join ';') })"
        $live = Get-Process -Name bench-scene, Moonlight -ErrorAction SilentlyContinue | ForEach-Object { $_.Name }
        "host_live=$($live -join ' ')"
        "host_disk_free_gb=$([math]::Round((Get-PSDrive C).Free / 1GB))"
    }
    default { throw "unknown action $Action" }
}
