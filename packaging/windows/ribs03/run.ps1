# ribs03: runs a command in MSYS2's UCRT64 shell at idle priority (the owner's music on this PC keeps
# playing smoothly), prints what it said and passes its exit code on. Radiance Music's Windows builds, #39.
param([Parameter(Mandatory = $true)][string]$Cmd)
$env:MSYSTEM = 'UCRT64'; $env:CHERE_INVOKING = '1'
$p = Start-Process C:\msys64\usr\bin\bash.exe -ArgumentList @('-lc', "`"$Cmd`"") -NoNewWindow -PassThru `
    -RedirectStandardOutput C:\rb\run.out -RedirectStandardError C:\rb\run.err
try { $p.PriorityClass = 'Idle' } catch {}   # what it starts gets the same
$p.WaitForExit()
Get-Content C:\rb\run.out, C:\rb\run.err -ErrorAction SilentlyContinue
exit $p.ExitCode
