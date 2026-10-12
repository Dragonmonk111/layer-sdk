param([int]$TailBytes = 400000, [int]$Show = 30)
# Read the last ~TailBytes bytes of the log without Get-Content's line-splitting quirks
$fs = [System.IO.File]::Open('\\wsl$\Ubuntu-24.04\opt\g1-node\slay3rd.log','Open','Read','ReadWrite')
try {
    $len = [Math]::Min($TailBytes, $fs.Length)
    $fs.Seek(-$len, 'End') | Out-Null
    $buf = New-Object byte[] $len
    $read = $fs.Read($buf, 0, $len)
    $txt = [System.Text.Encoding]::UTF8.GetString($buf, 0, $read)
} finally { $fs.Close() }
$lines = $txt -split "`n"
"SPAN: $($lines.Count) lines (tail $len bytes)"
$hits = $lines | Select-String 'unavailable|received from peer|broadcast to|certificate stored|peer payload rejected'
$hits | Select-Object -Last $Show | ForEach-Object { $_.Line.Trim().Substring(0,[Math]::Min(150,$_.Line.Trim().Length)) }
"--- counts in tail ---"
"UNAVAIL: " + ($lines | Select-String 'finalized payload unavailable').Count
"REJECTED: " + ($lines | Select-String 'peer payload rejected').Count
"EXEC: " + ($lines | Select-String 'certificate stored').Count
"INBOUND: " + ($lines | Select-String 'received from peer').Count
