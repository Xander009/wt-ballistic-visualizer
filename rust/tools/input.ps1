# Send mouse messages to the wtv window (client px): input.ps1 click|rclick|wheel x y [delta]
param([string]$what, [int]$x, [int]$y, [int]$delta = 120)
Add-Type @"
using System; using System.Runtime.InteropServices;
public class M { [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
 [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref POINT p);
 public struct POINT { public int X, Y; } }
"@
$h = (Get-Process wtv | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1).MainWindowHandle
$l = [IntPtr](($y -shl 16) -bor $x)
[M]::SendMessage($h, 0x200, [IntPtr]0, $l) | Out-Null  # move
switch ($what) {
  "click"  { [M]::SendMessage($h, 0x201, [IntPtr]1, $l) | Out-Null; [M]::SendMessage($h, 0x202, [IntPtr]0, $l) | Out-Null }
  "rclick" { [M]::SendMessage($h, 0x204, [IntPtr]2, $l) | Out-Null; [M]::SendMessage($h, 0x205, [IntPtr]0, $l) | Out-Null }
  "wheel"  { $p = New-Object M+POINT; $p.X = $x; $p.Y = $y; [M]::ClientToScreen($h, [ref]$p) | Out-Null
             [M]::SendMessage($h, 0x20A, [IntPtr]([int]($delta -shl 16)), [IntPtr](($p.Y -shl 16) -bor ($p.X -band 0xffff))) | Out-Null }
}
"sent $what"
