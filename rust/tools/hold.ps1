# Hold a key in the wtv window for some milliseconds: hold.ps1 <letter> <ms>   (the map must have focus: click it first)
param([string]$key, [int]$ms)
Add-Type @"
using System; using System.Runtime.InteropServices;
public class H { [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
 [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l); }
"@
$h = (Get-Process wtv | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1).MainWindowHandle
$vk = [int][char]$key.ToUpper()
$sc = @{87=0x11; 65=0x1E; 83=0x1F; 68=0x20; 82=0x13}[$vk]   # W A S D R scan codes
[H]::PostMessage($h, 0x100, [IntPtr]$vk, [IntPtr](1 -bor ($sc -shl 16))) | Out-Null
Start-Sleep -Milliseconds $ms
[H]::PostMessage($h, 0x101, [IntPtr]$vk, [IntPtr](0xC0000001 -bor ($sc -shl 16))) | Out-Null
"held $key $ms ms"
