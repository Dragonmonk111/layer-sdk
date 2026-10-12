param([int]$TailBytes = 600000)
$fs = [System.IO.File]::Open('\\wsl$\Ubuntu-24.04\opt\g1-node\slay3rd.log','Open','Read','ReadWrite')
try {
    $len = [Math]::Min($TailBytes, $fs.Length)
    $fs.Seek(-$len, 'End') | Out-Null
    $buf = New-Object byte[] $len
    $read = $fs.Read($buf, 0, $len)
    $txt = [System.Text.Encoding]::UTF8.GetString($buf, 0, $read)
} finally { $fs.Close() }
$lines = $txt -split "`n"
"SPAN: $($lines.Count) lines"
# distinct missing digests in window + first/last occurrence time
$unav = $lines | Select-String 'finalized payload unavailable'
"UNAVAIL lines: $($unav.Count)"
$dg = @{}
foreach ($l in $unav) {
    if ($l.Line -match 'missing=([0-9a-f]{64})') {
        $d = $Matches[1]
        if (-not $dg.ContainsKey($d)) { $dg[$d] = @{first=$l.Line.Substring(0,23); last=''; count=0} }
        $dg[$d].last = $l.Line.Substring(0,23)
        $dg[$d].count++
    }
}
"--- distinct missing digests (first..last, count) ---"
foreach ($k in $dg.Keys) { "$k  $($dg[$k].first)..$($dg[$k].last)  x$($dg[$k].count)" }
# did each missing digest ever arrive?
"--- arrival check ---"
foreach ($k in $dg.Keys) {
    $arr = ($lines | Select-String $k | Select-String 'inserted into pending_payloads').Count
    "$k arrivals=$arr"
}
"--- exec tip now ---"
$exec = $lines | Select-String 'certificate stored by Reporter' | Select-Object -Last 1
if ($exec) { $exec.Line.Substring(0,[Math]::Min(140,$exec.Line.Length)) }
$execs = ($lines | Select-String 'certificate stored by Reporter').Count
"EXEC lines: $execs"
