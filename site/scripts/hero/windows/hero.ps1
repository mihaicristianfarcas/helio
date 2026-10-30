# Helio's Hosts page on the wallpaper, as this desktop composites it: the
# taskbar and desktop icons hidden for the moment of the picture and put
# back after, the window in the middle of the screen.
param([string]$Appearance = 'dark', [string]$Out = 'C:\pp\hero\hero.png', [int]$CanvasW = 2700, [int]$CanvasH = 1800)
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices; using System.Text;
public static class U {
  public delegate bool Each(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern bool SystemParametersInfo(uint a, uint b, string c, uint d);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr FindWindow(string c, string t);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr FindWindowEx(IntPtr p, IntPtr a, string c, string t);
  [DllImport("user32.dll")] public static extern bool EnumWindows(Each e, IntPtr l);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int w, int t, uint f);
  [DllImport("user32.dll")] public static extern int GetSystemMetrics(int i);
  [DllImport("kernel32.dll")] public static extern IntPtr GetConsoleWindow();
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr SendMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern void keybd_event(byte k, byte s, uint f, UIntPtr e);
  public struct RECT { public int L, T, R, B; }
  public static IntPtr Icons() {
    IntPtr found = IntPtr.Zero;
    EnumWindows((h, l) => { var v = FindWindowEx(h, IntPtr.Zero, "SHELLDLL_DefView", null); if (v != IntPtr.Zero) { found = v; return false; } return true; }, IntPtr.Zero);
    return found;
  }
}
"@
[U]::SetProcessDPIAware() | Out-Null
[U]::ShowWindow([U]::GetConsoleWindow(), 0) | Out-Null
$tray = [U]::FindWindow('Shell_TrayWnd', $null)
# The shell's own toggle for the desktop's icons: the view that holds them
# also holds the wallpaper, so it cannot simply be hidden.
$view = [U]::Icons()
[U]::SendMessage($view, 0x111, [IntPtr]0x7402, [IntPtr]::Zero) | Out-Null
[U]::ShowWindow($tray, 0) | Out-Null
try {
  # Whatever a key opened: Start, a flyout.
  [U]::keybd_event(0x1B, 0, 0, [UIntPtr]::Zero); [U]::keybd_event(0x1B, 0, 2, [UIntPtr]::Zero)
  $data = 'C:\pp\hero\data'
  Remove-Item -Recurse -Force $data -ErrorAction SilentlyContinue
  New-Item -ItemType Directory -Force -Path "$data\host" | Out-Null
  Set-Content -Path "$data\settings.toml" -Encoding ASCII -Value 'onboarded = true'
  $env:HELIO_DATA_DIR = $data; $env:HELIO_HOST_DATA_DIR = "$data\host"; $env:HELIO_APPEARANCE = $Appearance
  $env:HELIO_UI_DEMO = 'hosts-sample,size=1120x740'
  $p = Start-Process C:\pp\hero\helio-app.exe -PassThru -WindowStyle Normal -RedirectStandardError C:\pp\hero\err.txt -RedirectStandardOutput C:\pp\hero\out.txt
  $deadline = (Get-Date).AddSeconds(40)
  do { Start-Sleep -Milliseconds 500; $p.Refresh() } while ($p.MainWindowHandle -eq 0 -and (Get-Date) -lt $deadline -and -not $p.HasExited)
  if ($p.HasExited) { throw "Helio exited with $($p.ExitCode)" }
  Start-Sleep 8
  $h = $p.MainWindowHandle
  $r = New-Object U+RECT
  [U]::GetWindowRect($h, [ref]$r) | Out-Null
  $sw = [U]::GetSystemMetrics(0); $sh = [U]::GetSystemMetrics(1)
  $w = $r.R - $r.L; $t = $r.B - $r.T
  "screen ${sw}x${sh}, window ${w}x${t}"
  [U]::SetWindowPos($h, [IntPtr]::Zero, [int](($sw - $w) / 2), [int](($sh - $t) / 2), 0, 0, 0x45) | Out-Null
  # Start, opened by whatever: its process ending closes it, and it comes back by itself.
  Stop-Process -Name StartMenuExperienceHost -Force -ErrorAction SilentlyContinue
  Start-Sleep 2
  # A key press is what lets a background task hand over the foreground:
  # without it the window stays inactive, and inactive windows lose acrylic.
  [U]::keybd_event(0x12, 0, 0, [UIntPtr]::Zero); [U]::keybd_event(0x12, 0, 2, [UIntPtr]::Zero)
  [U]::SetForegroundWindow($h) | Out-Null
  Start-Sleep 3
  $bmp = New-Object Drawing.Bitmap $CanvasW, $CanvasH
  $g = [Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen([int](($sw - $CanvasW) / 2), [int](($sh - $CanvasH) / 2), 0, 0, $bmp.Size)
  $bmp.Save($Out, [Drawing.Imaging.ImageFormat]::Png)
  "saved $Out"
  Stop-Process -Id $p.Id -Force
} finally {
  [U]::ShowWindow($tray, 5) | Out-Null
  [U]::SendMessage($view, 0x111, [IntPtr]0x7402, [IntPtr]::Zero) | Out-Null
}
