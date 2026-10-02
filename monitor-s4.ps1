# S4 monitoring: app_hash agreement + liveness across all 4 validators.
# Polls docker logs every $IntervalSec, writes monitor-s4.log.
# ALERT lines: app_hash mismatch at same height, height lag > $LagMax, stall > $StallSec.
param(
    [int]$IntervalSec = 30,
    [int]$LagMax = 10,
    [int]$StallSec = 90,
    [int]$DurationHours = 26
)
$esc = [char]27
$nodes = 0..3 | ForEach-Object { "junoclaw-node-$_" }
$log = "monitor-s4.log"
$deadline = (Get-Date).AddHours($DurationHours)
$lastMaxHeight = 0
$lastAdvance = Get-Date
$alerts = 0

function Get-Latest($node) {
    $line = docker logs $node --tail 60 2>&1 |
        Where-Object { $_ -match 'certificate stored' } | Select-Object -Last 1
    if (-not $line) { return $null }
    $clean = "$line" -replace "$esc\[[0-9;]*m", ''
    $h = $null; $ah = $null
    foreach ($t in ($clean -split '\s+')) {
        if ($t.StartsWith('height='))  { $h  = $t.Substring(7) }
        if ($t.StartsWith('app_hash=')) { $ah = $t.Substring(9) }
    }
    if ($h -and $ah) { @{ height = [int64]$h; apphash = $ah } } else { $null }
}

Add-Content $log "=== S4 monitor start $(Get-Date -Format u) ==="
while ((Get-Date) -lt $deadline) {
    $now = Get-Date -Format u
    $states = @{}
    foreach ($n in $nodes) { $states[$n] = Get-Latest $n }

    $alive = @($states.GetEnumerator() | Where-Object { $_.Value })
    if ($alive.Count -eq 0) {
        Add-Content $log "$now ALERT no node reporting"
        Start-Sleep $IntervalSec; continue
    }

    $maxH = ($alive | ForEach-Object { $_.Value.height } | Measure-Object -Max).Maximum
    if ($maxH -gt $lastMaxHeight) { $lastMaxHeight = $maxH; $lastAdvance = Get-Date }

    # Liveness stall
    $stalled = ((Get-Date) - $lastAdvance).TotalSeconds -gt $StallSec
    if ($stalled) {
        $alerts++
        Add-Content $log "$now ALERT stall: no finalized height > ${StallSec}s (tip=$lastMaxHeight)"
    }

    # Per-node lag + app_hash agreement vs majority
    $line = "$now tip=$maxH"
    foreach ($n in $nodes) {
        $s = $states[$n]
        if (-not $s) { $line += " | $n DOWN"; continue }
        $lag = $maxH - $s.height
        if ($lag -gt $LagMax) {
            $alerts++
            Add-Content $log "$now ALERT lag: $n at $($s.height), tip $maxH (lag=$lag)"
        }
        $line += " | $n h=$($s.height)"
    }
    # app_hash check at min common height
    $minH = ($alive | ForEach-Object { $_.Value.height } | Measure-Object -Min).Minimum
    # compare latest app_hash of nodes sitting at the SAME height
    $byH = @{}
    foreach ($kv in $alive) {
        $k = "$($kv.Value.height)"
        if (-not $byH[$k]) { $byH[$k] = @{} }
        $byH[$k][$kv.Value.apphash] = $true
    }
    foreach ($h in $byH.Keys) {
        if ($byH[$h].Count -gt 1) {
            $alerts++
            Add-Content $log "$now ALERT APP_HASH MISMATCH at h$h : $($byH[$h].Keys -join ' vs ')"
        }
    }
    Add-Content $log $line
    Start-Sleep $IntervalSec
}
Add-Content $log "=== S4 monitor end $(Get-Date -Format u) alerts=$alerts ==="
