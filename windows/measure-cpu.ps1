# ribs03: the stage-1 checkpoint (issue #45). CPU of the beta, or of today's GTK app, while a song plays
# (silent) on Now Playing at 1280x800 in front; then minimized; then (beta) hidden to the tray. Both
# use the same test library (C:\rb\beta-test) and play its latest song. Started in the owner's desktop
# session through the RadianceMusicTest task; C:\rb\test-start.cmd is put back after.
#   powershell -File C:\rb\beta\src\windows\measure-cpu.ps1 -App beta|gtk [-Seconds 60]
# -Video FILE (beta): FILE playing alone in the beta's video check (--video-probe), in front, then minimized.
param([ValidateSet('beta', 'gtk')][string]$App = 'beta', [int]$Seconds = 60, [switch]$Short, [string]$Video = '')
$ErrorActionPreference = 'Stop'
Add-Type @'
using System; using System.Runtime.InteropServices; using System.Collections.Generic;
public static class Win {
    public delegate bool EnumProc(IntPtr h, IntPtr l);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr l);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint msg, IntPtr w, IntPtr l);
    public static IntPtr Find(uint pid) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((h, l) => { uint p; GetWindowThreadProcessId(h, out p); if (p == pid && IsWindowVisible(h)) { found = h; return false; } return true; }, IntPtr.Zero);
        return found;
    }
}
'@
$start = 'C:\rb\test-start.cmd'
# the task's own script, kept aside while a test uses it (a run cut off earlier left it here: put back first)
$orig = 'C:\rb\test-start.cmd.orig'
if (Test-Path $orig) { Copy-Item -Force $orig $start; Remove-Item $orig }
$kept = Get-Content $start -Raw
[IO.File]::WriteAllText($orig, $kept)
$data = @"
set XDG_CONFIG_HOME=C:\rb\beta-test\config
set XDG_DATA_HOME=C:\rb\beta-test\data
set XDG_CACHE_HOME=C:\rb\beta-test\cache
set RADIANCE_SILENT=1
"@
if ($App -eq 'beta') {
    $name = 'RadianceMusicBeta'
    $cmd = "@echo off`r`n$data`r`nstart `"`" `"C:\rb\beta\dist\RadianceMusicBeta.exe`" --play-test --page playing --size 1280x800"
    if ($Video) { $cmd = "@echo off`r`n$data`r`nstart `"`" `"C:\rb\beta\dist\RadianceMusicBeta.exe`" --video-probe `"$Video`" --size 1280x720" }
} else {
    $name = 'radiance-music'
    $exe = "$env:LOCALAPPDATA\Programs\Radiance Music\bin\radiance-music.exe"
    # (a screenshot copy: never saves where you are; it quits after RADIANCE_SHOT_WAIT seconds)
    # (its screenshot copy lives 18 s at most, main.rs: so 4 s to start, then 12 s measured, in front only)
    $cmd = "@echo off`r`n$data`r`nset RADIANCE_SHOT=C:\rb\beta\shots\gtk-measure.png`r`nset RADIANCE_SHOT_PAGE=playing`r`nset RADIANCE_SHOT_PLAY=1`r`nset RADIANCE_SHOT_SIZE=1280x800`r`nset RADIANCE_SHOT_WAIT=18`r`nstart `"`" `"$exe`""
    $Seconds = 12
}
function Cpu($p, $label) {
    $p.Refresh(); $a = $p.TotalProcessorTime.TotalSeconds; $t = Get-Date
    Start-Sleep -Seconds $Seconds
    $p.Refresh()
    if ($p.HasExited) { Write-Output "$App, ${label}: the app quit while measured"; return }
    $b = $p.TotalProcessorTime.TotalSeconds
    $pct = [math]::Round(100 * ($b - $a) / ((Get-Date) - $t).TotalSeconds, 1)
    Write-Output ("{0,-34} {1,6} % of one core" -f "$App, $label", $pct)
}
# (the owner's own Radiance Music may be running: the one measured is the one this starts)
$before = @(Get-Process $name -ErrorAction SilentlyContinue | ForEach-Object { $_.Id })
$p = $null
try {
    New-Item -ItemType Directory -Force C:\rb\beta\shots | Out-Null
    [IO.File]::WriteAllText($start, $cmd)
    schtasks /run /tn RadianceMusicTest | Out-Null
    # -Short: as today's app is measured (4 s to start, 12 s measured, in front only), to compare the two
    if ($Short) { $Seconds = 12 }
    Start-Sleep -Seconds $(if ($App -eq 'gtk' -or $Short) { 4 } else { 15 })
    # (the new one with the most memory: the app's window, not a short-lived helper of the same name)
    $p = Get-Process $name -ErrorAction SilentlyContinue | Where-Object { $before -notcontains $_.Id } | Sort-Object WorkingSet64 -Descending | Select-Object -First 1
    Write-Output "measuring $name, process $($p.Id), $([math]::Round($p.WorkingSet64 / 1MB)) MB"
    if (-not $p) { throw "$name didn't start" }
    Cpu $p $(if ($Video) { 'a video in front' } else { 'Now Playing in front' })
    if ($App -eq 'gtk' -or $Short) { return }
    $h = [Win]::Find([uint32]$p.Id)
    [Win]::ShowWindow($h, 6) | Out-Null   # SW_MINIMIZE
    Start-Sleep -Seconds 3
    Cpu $p 'minimized'
    if ($App -eq 'beta' -and -not $Video) {
        [Win]::ShowWindow($h, 9) | Out-Null   # SW_RESTORE
        Start-Sleep -Seconds 2
        [Win]::PostMessage($h, 0x10, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null   # WM_CLOSE: playing, so it hides to the tray
        Start-Sleep -Seconds 3
        Cpu $p 'hidden to the tray'
    }
    $p.Refresh()
    Write-Output "memory: $([math]::Round($p.WorkingSet64 / 1MB)) MB"
} finally {
    if ($p) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue }
    [IO.File]::WriteAllText($start, $kept)
    Remove-Item $orig -ErrorAction SilentlyContinue
}
