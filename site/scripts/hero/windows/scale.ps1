# Sets the main display's scale at once, as Settings does (DisplayConfig's
# undocumented relative scale; a log-off is what the registry route needs).
param([int]$Percent = 200)
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class Scale {
  [DllImport("user32.dll")] public static extern int GetDisplayConfigBufferSizes(uint f, out uint p, out uint m);
  [DllImport("user32.dll")] public static extern int QueryDisplayConfig(uint f, ref uint p, byte[] paths, ref uint m, byte[] modes, IntPtr t);
  [DllImport("user32.dll")] public static extern int DisplayConfigGetDeviceInfo(byte[] b);
  [DllImport("user32.dll")] public static extern int DisplayConfigSetDeviceInfo(byte[] b);
  static int[] Steps = {100,125,150,175,200,225,250,300,350,400,450,500};
  static byte[] Head(byte[] path, int type, int size) {
    var b = new byte[size];
    Buffer.BlockCopy(BitConverter.GetBytes(type), 0, b, 0, 4);
    Buffer.BlockCopy(BitConverter.GetBytes(size), 0, b, 4, 4);
    Buffer.BlockCopy(path, 0, b, 8, 8);    // adapter id
    Buffer.BlockCopy(path, 8, b, 16, 4);   // source id
    return b;
  }
  public static string Set(int percent) {
    uint p, m;
    GetDisplayConfigBufferSizes(2, out p, out m);
    var paths = new byte[p * 72]; var modes = new byte[m * 64];
    int r = QueryDisplayConfig(2, ref p, paths, ref m, modes, IntPtr.Zero);
    if (r != 0) return "query failed " + r;
    var get = Head(paths, -3, 32);
    r = DisplayConfigGetDeviceInfo(get);
    if (r != 0) return "get failed " + r;
    int min = BitConverter.ToInt32(get, 20), cur = BitConverter.ToInt32(get, 24), max = BitConverter.ToInt32(get, 28);
    int rec = Math.Abs(min), want = Array.IndexOf(Steps, percent);
    var set = Head(paths, -4, 24);
    Buffer.BlockCopy(BitConverter.GetBytes(want - rec), 0, set, 20, 4);
    r = DisplayConfigSetDeviceInfo(set);
    return "min " + min + " cur " + cur + " max " + max + " want index " + want + ", set: " + r;
  }
}
"@
[Scale]::Set($Percent)
