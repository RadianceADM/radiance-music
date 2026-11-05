# ribs03: runs the beta in the owner's desktop session through the RadianceMusicTest task (the only
# way a window shows from SSH), with its test data (C:\rb\beta-test) and no sound, then puts
# C:\rb\test-start.cmd back as it was. Waits until the beta quits (or -Seconds), prints its exit.
#   powershell -File C:\rb\beta\src\windows\test-run.ps1 -AppArgs '--shot C:\rb\beta\shots\home.png --page home' -Seconds 90
param([string]$AppArgs = '', [int]$Seconds = 120)
$ErrorActionPreference = 'Stop'
$start = 'C:\rb\test-start.cmd'
$kept = Get-Content $start -Raw
$exe = 'C:\rb\beta\dist\RadianceMusicBeta.exe'
New-Item -ItemType Directory -Force C:\rb\beta\shots | Out-Null
$cmd = @"
@echo off
set XDG_CONFIG_HOME=C:\rb\beta-test\config
set XDG_DATA_HOME=C:\rb\beta-test\data
set XDG_CACHE_HOME=C:\rb\beta-test\cache
set RADIANCE_SILENT=1
start "" /belownormal "$exe" $AppArgs
"@
try {
    [IO.File]::WriteAllText($start, $cmd)
    schtasks /run /tn RadianceMusicTest | Out-Null
    $deadline = (Get-Date).AddSeconds($Seconds)
    Start-Sleep -Seconds 3
    while ((Get-Date) -lt $deadline -and (Get-Process RadianceMusicBeta -ErrorAction SilentlyContinue)) { Start-Sleep -Milliseconds 500 }
    $left = Get-Process RadianceMusicBeta -ErrorAction SilentlyContinue
    if ($left) { $left | Stop-Process -Force; Write-Output "stopped after $Seconds s" } else { Write-Output "the beta quit" }
} finally {
    [IO.File]::WriteAllText($start, $kept)
}
