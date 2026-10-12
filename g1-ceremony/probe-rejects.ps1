param([int]$Tail = 4000)
$t = Get-Content -LiteralPath '\\wsl$\Ubuntu-24.04\opt\g1-node\slay3rd.log' -Tail $Tail
"SPAN: $($t.Count) lines"
"--- reject reasons ---"
($t | Select-String 'peer payload rejected' | ForEach-Object { ($_.Line -split 'reason=')[1] -split ' ' | Select-Object -First 3 | ForEach-Object { $_ } } | Group-Object | Sort-Object Count -Descending | ForEach-Object { "$($_.Count)x $($_.Name)" })
"--- reject count total ---"
($t | Select-String 'peer payload rejected').Count
"--- last 8 rejections (height + reason) ---"
($t | Select-String 'peer payload rejected' | Select-Object -Last 8 | ForEach-Object { $_.Line.Substring(0,[Math]::Min(160,$_.Line.Length)) })
"--- inbound heights (last 10) ---"
($t | Select-String 'received from peer' | Select-Object -Last 10 | ForEach-Object { if ($_.Line -match 'height=(\d+)') { $Matches[1] } })
"--- last 3 unavail lines ---"
($t | Select-String 'finalized payload unavailable' | Select-Object -Last 3 | ForEach-Object { $_.Line.Substring(0,[Math]::Min(200,$_.Line.Length)) })
"--- UNAVAIL count ---"
($t | Select-String 'finalized payload unavailable').Count
"--- exec count + newest ---"
"EXEC: " + ($t | Select-String 'certificate stored by Reporter').Count
"NEWEST: " + (($t | Select-String 'certificate stored by Reporter' | Select-Object -Last 1).Line.Substring(0,60))
