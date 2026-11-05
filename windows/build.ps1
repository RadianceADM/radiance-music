# Radiance Music for Windows, the native beta (WinUI 3, issue #45). On ribs03 only (the owner's PC):
# everything here runs at Idle priority, with test data in C:\rb\beta-test (never the owner's) and
# no sound (RADIANCE_SILENT). The source is C:\rb\beta\src (branch winui-stage1, from GitHub).
#   powershell -NoProfile -ExecutionPolicy Bypass -File C:\rb\beta\src\windows\build.ps1 -Step rust|app|dist|all
# rust: the core/engine/ffi tests and radiance_ffi.dll, in MSYS2 UCRT64 (the GNU toolchain and
#       MSYS2's GStreamer, as today's Windows app: one C runtime, UCRT, for the DLL and GStreamer)
# app:  the C# binding (uniffi-bindgen-cs), the WinUI app and its tests (.NET 10)
# dist: C:\rb\beta\dist (the app, the DLL, GStreamer, the tools) and its self-check
# installer: C:\rb\beta\RadianceMusic-Beta-Setup.exe from dist (NSIS, MSYS2's makensis) and its SHA-256
param([ValidateSet('rust', 'app', 'dist', 'installer', 'all')][string]$Step = 'all')
$ErrorActionPreference = 'Stop'
(Get-Process -Id $PID).PriorityClass = 'Idle'   # what it starts gets the same
$B = 'C:\rb\beta'
$Src = "$B\src"
$Test = 'C:\rb\beta-test'
$env:PATH = "C:\Program Files\dotnet;C:\Program Files\Git\cmd;$env:PATH"
$env:DOTNET_CLI_TELEMETRY_OPTOUT = '1'
$env:DOTNET_NOLOGO = '1'
$env:RADIANCE_SILENT = '1'
# the test data: once, a copy of the GTK test copy's songs and settings (C:\rb\test, never the owner's)
foreach ($d in 'data', 'config', 'cache') { New-Item -ItemType Directory -Force "$Test\$d" | Out-Null }
if (-not (Test-Path "$Test\data\radiance-music") -and (Test-Path 'C:\rb\test\data\radiance-music')) {
    Copy-Item -Recurse 'C:\rb\test\data\radiance-music' "$Test\data\"
    if (Test-Path 'C:\rb\test\config\radiance-music') { Copy-Item -Recurse 'C:\rb\test\config\radiance-music' "$Test\config\" }
    Remove-Item "$Test\data\radiance-music\running.port" -ErrorAction SilentlyContinue
}
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

if ($Step -in 'app', 'all') {
    Step 'the C# binding' { Msys bindgen }
    Step 'the beta''s own tests' { dotnet test "$Src\windows\RadianceMusic.Windows.Tests" -c Release --nologo -v q }
    Step 'the app' { dotnet publish "$Src\windows\RadianceMusic.Windows\RadianceMusic.Windows.csproj" -c Release -o "$B\app" --nologo -v q }
}

if ($Step -in 'dist', 'all') {
    Remove-Item -Recurse -Force "$B\dist" -ErrorAction SilentlyContinue
    Step 'the engine''s DLL, GStreamer and the tools' { Msys pack }
    Copy-Item -Recurse -Force "$B\app\*" "$B\dist\"
    $out = "$B\selfcheck.txt"
    Remove-Item $out -ErrorAction SilentlyContinue
    $p = Start-Process -FilePath "$B\dist\RadianceMusicBeta.exe" -ArgumentList @('--self-check', $out) -Wait -PassThru -NoNewWindow
    Write-Output "== self-check (exit $($p.ExitCode))"
    $lines = Get-Content $out -Encoding UTF8
    $lines
    # the owner's own library must never be what a test opens
    $data = ($lines | Where-Object { $_ -like 'data *' } | Select-Object -First 1) -replace '^data ', ''
    if (-not $data -or -not $data.StartsWith("$Test\data")) { throw "the beta opened a data folder outside $Test : $data" }
    if ($p.ExitCode -ne 0) { throw "self-check failed" }
    Write-Output "dist: $((Get-ChildItem -Recurse -File "$B\dist").Count) files, $([math]::Round((Get-ChildItem -Recurse -File "$B\dist" | Measure-Object Length -Sum).Sum / 1MB)) MB"
}

if ($Step -eq 'installer') {
    $version = ([xml](Get-Content "$Src\windows\RadianceMusic.Windows\RadianceMusic.Windows.csproj")).Project.PropertyGroup.Version | Where-Object { $_ } | Select-Object -First 1
    Step 'the installer' { Msys installer }
    $setup = "$B\RadianceMusic-Beta-Setup.exe"
    Write-Output "$((Get-FileHash $setup -Algorithm SHA256).Hash.ToLower())  RadianceMusic-Beta-Setup.exe ($version, $([math]::Round((Get-Item $setup).Length / 1MB)) MB)"
}
