# ribs03: MSYS2 (UCRT64) with GTK 4, libadwaita, GStreamer and Rust for Radiance Music's Windows builds (#39).
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
New-Item -ItemType Directory -Force C:\rb\dl | Out-Null
$log = 'C:\rb\setup.log'
function note($t) { $l = "$(Get-Date -Format 'HH:mm:ss') $t"; $l; Add-Content $log $l }
if (-not (Test-Path C:\msys64\usr\bin\bash.exe)) {
    $base = 'https://github.com/msys2/msys2-installer/releases/latest/download/msys2-base-x86_64-latest.sfx.exe'
    # (the "latest" release is MSYS2's rolling one: no .sha256 beside it; GitHub's own digest of it)
    $want = '38a82af9aa1eede1abd6b287460b96db63d8419f91ba42296a19afc8eac445b2'
    if (-not (Test-Path C:\rb\dl\msys2-base.sfx.exe)) { note 'downloading MSYS2'; Invoke-WebRequest $base -OutFile C:\rb\dl\msys2-base.sfx.exe }
    $got = (Get-FileHash C:\rb\dl\msys2-base.sfx.exe -Algorithm SHA256).Hash.ToLower()
    if ($want -ne $got) { note "MSYS2 download damaged: $got vs $want"; exit 1 }
    note "MSYS2 checked ($($got.Substring(0,12))), unpacking"
    & C:\rb\dl\msys2-base.sfx.exe -y -oC:\ | Out-Null
}
$bash = 'C:\msys64\usr\bin\bash.exe'
$env:MSYSTEM = 'UCRT64'; $env:CHERE_INVOKING = '1'
function msys($cmd) {
    # idle priority: ribs03's music keeps playing smoothly
    $p = Start-Process -FilePath $bash -ArgumentList @('-lc', "`"$cmd`"") -NoNewWindow -PassThru -RedirectStandardOutput C:\rb\out.txt -RedirectStandardError C:\rb\err.txt
    try { $p.PriorityClass = 'Idle' } catch {}   # what it starts gets the same
    $p.WaitForExit()
    Get-Content C:\rb\out.txt, C:\rb\err.txt -ErrorAction SilentlyContinue | Select-Object -Last 4 | ForEach-Object { note "  $_" }
    return $p.ExitCode
}
note 'first start'
msys 'true' | Out-Null
note 'package keys'
msys 'test -d /etc/pacman.d/gnupg || { pacman-key --init && pacman-key --populate msys2; }; gpgconf --kill gpg-agent' | Out-Null
note 'updating MSYS2 (twice: the core first)'
msys 'pacman -Syuu --noconfirm' | Out-Null
msys 'pacman -Syuu --noconfirm' | Out-Null
$pk = 'git mingw-w64-ucrt-x86_64-gcc mingw-w64-ucrt-x86_64-pkgconf mingw-w64-ucrt-x86_64-rust mingw-w64-ucrt-x86_64-gtk4 mingw-w64-ucrt-x86_64-libadwaita mingw-w64-ucrt-x86_64-gstreamer mingw-w64-ucrt-x86_64-gst-plugins-base mingw-w64-ucrt-x86_64-gst-plugins-good mingw-w64-ucrt-x86_64-gst-plugins-bad mingw-w64-ucrt-x86_64-gst-libav mingw-w64-ucrt-x86_64-nsis mingw-w64-ucrt-x86_64-ntldd'
note 'installing the build tools'
$code = msys "pacman -S --needed --noconfirm $pk"
note "pacman exit $code"
msys 'rustc --version; cargo --version; pkgconf --modversion gtk4 libadwaita-1 gstreamer-1.0; gcc --version | head -1; makensis -VERSION' | Out-Null
note "MSYS2 size: $([math]::Round((Get-ChildItem C:\msys64 -Recurse -File -ErrorAction SilentlyContinue | Measure-Object Length -Sum).Sum/1GB,1)) GB; C: free $([math]::Round((Get-PSDrive C).Free/1GB)) GB"
msys 'gpgconf --kill gpg-agent' | Out-Null   # (it would keep the session open)
note 'done'
