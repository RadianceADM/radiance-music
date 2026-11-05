# Radiance Music for Windows, the native beta (WinUI 3, issue #45). On ribs03 only (the owner's PC):
# everything here runs at Idle priority, with test data in C:\rb\beta-test (never the owner's) and
# no sound (RADIANCE_SILENT). The source is C:\rb\beta\src (branch winui-stage1, from GitHub).
#   powershell -NoProfile -ExecutionPolicy Bypass -File C:\rb\beta\src\windows\build.ps1 -Step rust|app|dist|all
# rust: the core/engine/ffi tests and radiance_ffi.dll, in MSYS2 UCRT64 (the GNU toolchain and
#       MSYS2's GStreamer, as today's Windows app: one C runtime, UCRT, for the DLL and GStreamer)
# app:  the C# binding (uniffi-bindgen-cs), the WinUI app and its tests (.NET 10)
# dist: C:\rb\beta\dist (the app, the DLL, GStreamer, the tools) and its self-check
param([ValidateSet('rust', 'app', 'dist', 'all')][string]$Step = 'all')
$ErrorActionPreference = 'Stop'
(Get-Process -Id $PID).PriorityClass = 'Idle'   # what it starts gets the same
$B = 'C:\rb\beta'
$Src = "$B\src"
$Test = 'C:\rb\beta-test'
$env:DOTNET_CLI_TELEMETRY_OPTOUT = '1'
$env:DOTNET_NOLOGO = '1'
$env:RADIANCE_SILENT = '1'
foreach ($d in 'data', 'config', 'cache') { New-Item -ItemType Directory -Force "$Test\$d" | Out-Null }
$env:XDG_DATA_HOME = "$Test\data"; $env:XDG_CONFIG_HOME = "$Test\config"; $env:XDG_CACHE_HOME = "$Test\cache"

function Step($what, [scriptblock]$run) {
    Write-Output "== $what"
    & $run
    if ($LASTEXITCODE -ne 0) { throw "$what failed ($LASTEXITCODE)" }
}

# windows/msys.sh in MSYS2's UCRT64 shell (no quotes to pass through: PowerShell would mangle them)
function Msys([string]$what) {
    $env:MSYSTEM = 'UCRT64'; $env:CHERE_INVOKING = '1'
    & C:\msys64\usr\bin\bash.exe -l /c/rb/beta/src/windows/msys.sh $what
}

if ($Step -in 'rust', 'all') {
    Step 'core, engine and ffi tests, then radiance_ffi.dll' { Msys rust }
    Step 'what the DLL needs' { Msys deps }
}
