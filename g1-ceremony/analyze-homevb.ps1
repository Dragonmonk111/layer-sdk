param([int]$Tail = 5000)
$t = Get-Content -LiteralPath '\\wsl$\Ubuntu-24.04\opt\g1-node\slay3rd.log' -Tail $Tail
"LINES-IN-TAIL: $($t.Count)"
"REJECTS:"
($t | Select-String 'peer payload rejected' | ForEach-Object { ($_.Line -split 'rejected ')[1] } | Group-Object | Sort-Object Count -Descending | Select-Object -First 4 | ForEach-Object { "$($_.Count)x $($_.Name)" })
"CAPACITY-DROPS: " + ($t | Select-String 'pending payload capacity').Count
"EXEC-NEWEST: " + (($t | Select-String 'certificate stored by Reporter' | Select-Object -Last 1).Line.Substring(0,90))
"EXEC-LINES: " + ($t | Select-String 'certificate stored by Reporter').Count
"INBOUND: " + ($t | Select-String 'received from peer').Count
"UNAVAIL: " + ($t | Select-String 'finalized payload unavailable').Count
"REQ-SEND-FAIL: " + ($t | Select-String 'request send failed').Count
"LAST-UNAVAIL: " + (($t | Select-String 'finalized payload unavailable' | Select-Object -Last 1).Line)
"LAST-INBOUND: " + (($t | Select-String 'received from peer' | Select-Object -Last 1).Line.Substring(0,170))
