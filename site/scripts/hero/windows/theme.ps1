# Windows' own light or dark mode, for what its backdrop falls back to with no GPU.
param([int]$Light = 0)
$k = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize'
Set-ItemProperty $k AppsUseLightTheme $Light
Set-ItemProperty $k SystemUsesLightTheme $Light
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class Say {
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr SendMessageTimeout(IntPtr h, uint m, IntPtr w, string l, uint f, uint t, out IntPtr r);
}
"@
$r = [IntPtr]::Zero
[Say]::SendMessageTimeout([IntPtr]0xffff, 0x1A, [IntPtr]::Zero, 'ImmersiveColorSet', 2, 3000, [ref]$r) | Out-Null
"light=$Light"
