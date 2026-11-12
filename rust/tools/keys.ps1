# Type text then optionally Enter into the wtv window: keys.ps1 <text> [enter]
param([string]$text, [string]$enter = "")
Add-Type @"
using System; using System.Runtime.InteropServices;
public class K { [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr h, uint m, IntPtr w, IntPtr l); }
"@
$h = (Get-Process wtv | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1).MainWindowHandle
foreach ($c in $text.ToCharArray()) {
  $vk = [int][char]([string]$c).ToUpper()
  [K]::SendMessage($h, 0x100, [IntPtr]$vk, [IntPtr]1) | Out-Null
  [K]::SendMessage($h, 0x102, [IntPtr][int]$c, [IntPtr]1) | Out-Null
  [K]::SendMessage($h, 0x101, [IntPtr]$vk, [IntPtr]0xC0000001) | Out-Null
}
if ($enter) { [K]::SendMessage($h, 0x100, [IntPtr]0x0D, [IntPtr]0x001C0001) | Out-Null; [K]::SendMessage($h, 0x102, [IntPtr]0x0D, [IntPtr]1) | Out-Null; [K]::SendMessage($h, 0x101, [IntPtr]0x0D, [IntPtr]0xC0000001) | Out-Null }
"typed"
