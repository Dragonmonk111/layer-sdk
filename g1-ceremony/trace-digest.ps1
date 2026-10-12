param([string]$Digest = 'a7b52ec536d6eff2c73b1240ba58de8a6a74b775264e2b7f295eebd52368cfd1', [int]$Tail = 120000)
$t = Get-Content -LiteralPath '\\wsl$\Ubuntu-24.04\opt\g1-node\slay3rd.log' -Tail $Tail
"LOG-LINES-SCANNED: $($t.Count)"
"--- give-ups (unavail) in window ---"
($t | Select-String 'finalized payload unavailable').Count
"--- digest lifecycle ($Digest) ---"
($t | Select-String $Digest | ForEach-Object { $_.Line.Substring(0, [Math]::Min(150, $_.Line.Length)) })
"--- current exec tip ---"
(($t | Select-String 'certificate stored by Reporter' | Select-Object -Last 1).Line.Substring(0,90))
