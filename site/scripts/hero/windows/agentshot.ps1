# The real desktop the sample agent session talks about: File Explorer on a
# Downloads folder holding the files it names, the two it picked selected.
# The big files are sparse: they show their size and take no disk.
param([string]$Out = 'C:\pp\hero\agent-screen.png')
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class V {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int w, int t, uint f);
  [DllImport("user32.dll")] public static extern int GetSystemMetrics(int i);
  [DllImport("user32.dll")] public static extern void keybd_event(byte k, byte s, uint f, UIntPtr e);
  [DllImport("kernel32.dll")] public static extern IntPtr GetConsoleWindow();
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
}
"@
[V]::SetProcessDPIAware() | Out-Null
[V]::ShowWindow([V]::GetConsoleWindow(), 0) | Out-Null
$dl = 'C:\Users\demo\Downloads'
Get-ChildItem $dl -Force -ErrorAction SilentlyContinue | Where-Object { $_.Name -ne 'desktop.ini' } | ForEach-Object {
  New-Item -ItemType Directory -Force -Path 'C:\pp\downloads-kept' | Out-Null
  Move-Item -LiteralPath $_.FullName -Destination 'C:\pp\downloads-kept' -Force
}
function Sample([string]$Name, [long]$Bytes, [string]$When) {
  $p = Join-Path $dl $Name
  Set-Content -LiteralPath $p -Value '' -NoNewline
  & fsutil sparse setflag $p | Out-Null
  $f = [IO.File]::OpenWrite($p); $f.SetLength($Bytes); $f.Close()
  (Get-Item -LiteralPath $p).LastWriteTime = [datetime]$When
}
$gb = 1024L * 1024 * 1024; $mb = 1024L * 1024
Sample 'ubuntu-24.04-desktop-amd64.iso' ([long](5.8 * $gb)) '2026-09-22 21:14'
Sample 'cyberpunk-patch-2.3.zip' ([long](3.1 * $gb)) '2026-09-18 19:02'
New-Item -ItemType Directory -Force -Path "$dl\OBS recordings" | Out-Null
(Get-Item "$dl\OBS recordings").LastWriteTime = [datetime]'2026-09-27 23:40'
Sample 'SteamSetup.exe' ([long](2.3 * $mb)) '2026-09-02 10:11'
Sample 'controller-mapping.png' ([long](1.9 * $mb)) '2026-09-25 16:45'
Sample 'manual.pdf' ([long](1.2 * $mb)) '2026-09-12 08:30'
Sample 'notes.txt' 4096L '2026-09-29 12:05'
Stop-Process -Name StartMenuExperienceHost -Force -ErrorAction SilentlyContinue
Start-Process explorer.exe $dl
Start-Sleep 5
$shell = New-Object -ComObject Shell.Application
$win = $null
foreach ($w in $shell.Windows()) { if ($w.LocationURL -match 'Downloads') { $win = $w } }
if (-not $win) { throw 'no Explorer window on Downloads' }
$doc = $win.Document
$doc.CurrentViewMode = 4
$doc.GroupBy = 'System.Null'
$doc.SortColumns = 'prop:-System.Size;'
Start-Sleep 1
$h = [IntPtr]$win.HWND
[V]::ShowWindow($h, 9) | Out-Null
[V]::SetWindowPos($h, [IntPtr]::Zero, 90, 22, 2000, 1380, 0x44) | Out-Null
[V]::SetCursorPos(2400, 1200) | Out-Null
Start-Sleep 1
$items = $doc.Folder.Items()
foreach ($i in 0..($items.Count - 1)) {
  $item = $items.Item($i)
  if ($item.Name -match '^(ubuntu|cyberpunk)') { $doc.SelectItem($item, 1) }
}
[V]::keybd_event(0x12, 0, 0, [UIntPtr]::Zero); [V]::keybd_event(0x12, 0, 2, [UIntPtr]::Zero)
[V]::SetForegroundWindow($h) | Out-Null
Start-Sleep 1
# Alt puts the focus on the command bar; Escape takes it back to the list,
# where Ctrl+Numpad Plus sizes the columns to fit.
[V]::keybd_event(0x1B, 0, 0, [UIntPtr]::Zero); [V]::keybd_event(0x1B, 0, 2, [UIntPtr]::Zero)
Start-Sleep 1
$doc.SelectItem($items.Item(0), 8 + 1) 
foreach ($i in 0..($items.Count - 1)) {
  $item = $items.Item($i)
  if ($item.Name -match '^(ubuntu|cyberpunk)') { $doc.SelectItem($item, 1) } else { $doc.SelectItem($item, 0) }
}
Start-Sleep 1
[V]::keybd_event(0x11, 0, 0, [UIntPtr]::Zero); [V]::keybd_event(0x6B, 0, 0, [UIntPtr]::Zero); [V]::keybd_event(0x6B, 0, 2, [UIntPtr]::Zero); [V]::keybd_event(0x11, 0, 2, [UIntPtr]::Zero)
Start-Sleep 3
$sw = [V]::GetSystemMetrics(0); $sh = [V]::GetSystemMetrics(1)
$bmp = New-Object Drawing.Bitmap $sw, $sh
$g = [Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen(0, 0, 0, 0, $bmp.Size)
$bmp.Save($Out, [Drawing.Imaging.ImageFormat]::Png)
"saved $Out ${sw}x${sh}"
