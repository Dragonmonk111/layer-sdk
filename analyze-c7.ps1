# C7 analysis: join bench CSV heights with node-0 exec_ms, print percentiles.
# node0.log = `docker logs junoclaw-node-0` (stderr). Lines carry ANSI color
# escapes (tracing fmt), so strip escapes then split on whitespace tokens —
# regex against raw lines misses because field names are wrapped in escapes.
$esc = [char]27
$map = @{}
Get-Content node0.log | ForEach-Object {
    $line = $_ -replace "$esc\[[0-9;]*m", ''
    if ($line -notmatch 'exec_ms') { return }
    $h = $null; $ms = $null
    foreach ($tok in ($line -split '\s+')) {
        if ($tok.StartsWith('height='))  { $h  = $tok.Substring(7) }
        if ($tok.StartsWith('exec_ms=')) { $ms = $tok.Substring(8) }
    }
    if ($h -and $ms) { $map[[int64]$h] = [int]$ms }
}
Write-Output ("mapped heights: {0}" -f $map.Count)

foreach ($leg in 'bank','bud2','bud5') {
    $hs = @(Get-Content "bench-$leg.csv" |
            Where-Object { $_ -match '^\w+,\d+,' } |
            ForEach-Object { [int64](($_ -split ',')[1]) })
    $vals = @($hs | Where-Object { $map.ContainsKey($_) } |
              ForEach-Object { $map[$_] } | Sort-Object)
    if ($vals.Count -eq 0) {
        Write-Output ("{0,-5} no exec_ms matches ({1} heights)" -f $leg, $hs.Count)
        continue
    }
    $n = $vals.Count
    Write-Output ("{0,-5} n={1} p50={2}ms p95={3}ms p99={4}ms max={5}ms" -f
        $leg, $n, $vals[[int]($n*0.5)], $vals[[int]($n*0.95)],
        $vals[[int]($n*0.99)], $vals[-1])
}
