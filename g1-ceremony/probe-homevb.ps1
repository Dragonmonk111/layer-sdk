param([int]$Tail = 3000)
$t = Get-Content -LiteralPath '\\wsl$\Ubuntu-24.04\opt\g1-node\slay3rd.log' -Tail $Tail
"LINES: $($t.Count)"
"--- last broadcast peer-counts ---"
($t | Select-String 'broadcast to' | Select-Object -Last 6 | ForEach-Object { $_.Line.Substring(0,[Math]::Min(120,$_.Line.Length)) })
"--- last peer/dial/connect lines ---"
($t | Select-String -Pattern 'peer|dial|connect' -CaseSensitive:$false | Select-Object -Last 6 | ForEach-Object { $_.Line.Substring(0,[Math]::Min(140,$_.Line.Length)) })
"--- unavail count + last ---"
"UNAVAIL: " + ($t | Select-String 'finalized payload unavailable').Count
"LAST-UNAVAIL: " + (($t | Select-String 'finalized payload unavailable' | Select-Object -Last 1).Line)
"--- exec newest ---"
"EXEC-NEWEST: " + (($t | Select-String 'certificate stored by Reporter' | Select-Object -Last 1).Line.Substring(0,90))
"EXEC-COUNT: " + ($t | Select-String 'certificate stored by Reporter').Count
"INBOUND: " + ($t | Select-String 'received from peer').Count
"LAST-INBOUND: " + (($t | Select-String 'received from peer' | Select-Object -Last 1).Line.Substring(0,[Math]::Min(150,200)))
"REQ-SEND-FAIL: " + ($t | Select-String 'request send failed').Count
