param([Parameter(Mandatory = $true)][string]$Image)
Add-Type @"
using System.Runtime.InteropServices;
public static class Wall { [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern bool SystemParametersInfo(uint a, uint b, string c, uint d); }
"@
"set: " + [Wall]::SystemParametersInfo(20, 0, $Image, 3)
