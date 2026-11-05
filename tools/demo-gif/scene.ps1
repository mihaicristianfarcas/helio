# The desktop before the demo take: a shortcut to the game, Notepad open and
# empty beside it, nothing else on screen.
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class D {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int w, int t, uint f);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("kernel32.dll")] public static extern IntPtr GetConsoleWindow();
}
"@
[D]::SetProcessDPIAware() | Out-Null
[D]::ShowWindow([D]::GetConsoleWindow(), 0) | Out-Null
$shell = New-Object -ComObject Shell.Application
foreach ($w in $shell.Windows()) { $w.Quit() }
Get-Process notepad, msedge -ErrorAction SilentlyContinue | Stop-Process -Force
Stop-Process -Name StartMenuExperienceHost -Force -ErrorAction SilentlyContinue
$edge = Join-Path ${env:ProgramFiles(x86)} 'Microsoft\Edge\Application\msedge.exe'
$desktop = [Environment]::GetFolderPath('Desktop')
$link = (New-Object -ComObject WScript.Shell).CreateShortcut((Join-Path $desktop 'Night Run.lnk'))
$link.TargetPath = $edge
$link.Arguments = '--kiosk file:///C:/pp/demo/game.html --edge-kiosk-type=fullscreen --no-first-run --no-default-browser-check --user-data-dir=C:\pp\edge-demo'
$link.Save()
# Notepad brings its last tabs back unless their state goes first.
Remove-Item "$env:LOCALAPPDATA\Packages\Microsoft.WindowsNotepad_8wekyb3d8bbwe\LocalState\TabState\*" -Force -ErrorAction SilentlyContinue
Remove-Item "$env:LOCALAPPDATA\Packages\Microsoft.WindowsNotepad_8wekyb3d8bbwe\LocalState\WindowState\*" -Force -ErrorAction SilentlyContinue
$p = Start-Process notepad -PassThru
Start-Sleep 3
$p = Get-Process notepad | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if ($p) { [D]::SetWindowPos($p.MainWindowHandle, [IntPtr]::Zero, 360, 110, 900, 560, 0x44) | Out-Null; [D]::SetForegroundWindow($p.MainWindowHandle) | Out-Null }
[D]::SetCursorPos(1100, 760) | Out-Null
"notepad: $($p.MainWindowHandle) desktop: $desktop"
