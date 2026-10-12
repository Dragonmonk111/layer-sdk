$out = 'c:\cosmos-node\node-data\config\CascadeProjects\windsurf-project\junoclaw-chain\g1-ceremony\voting-check.out.txt'
$log = @()
$siglog = 'c:\cosmos-node\node-data\config\CascadeProjects\windsurf-project\junoclaw-chain\g1-ceremony\mayo_sigs.log'
if (-not (Test-Path $siglog)) {
    # find it — may live elsewhere
    $cands = Get-ChildItem -Recurse 'c:\cosmos-node\node-data\config\CascadeProjects\windsurf-project' -Filter 'mayo_sigs.log' -ErrorAction SilentlyContinue
    if ($cands) { $siglog = $cands[0].FullName }
}
$log += "siglog: $siglog"
if (Test-Path $siglog) {
    $tail = Get-Content $siglog -Tail 400
    $log += '=== last 10 NOTARIZE/FINALIZE for v1 (home-vb) ==='
    $tail | Where-Object { $_ -match 'NOTARIZE|FINALIZE' } | Select-Object -Last 25 | ForEach-Object { $log += $_.Substring(0,[Math]::Min(160,$_.Length)) }
    $log += '=== seat counts in tail ==='
    ($tail | Where-Object { $_ -match 'NOTARIZE' }).Count | ForEach-Object { $log += "NOTARIZE: $_" }
    ($tail | Where-Object { $_ -match 'FINALIZE' }).Count | ForEach-Object { $log += "FINALIZE: $_" }
}
# also scan main log for 'broadcast to N peers' and UNAVAIL in last 5 min window
$mainlog = 'c:\cosmos-node\node-data\slay3rd.log'
$log += '=== home-vb main log tail events ==='
$ml = $null
foreach ($p in @('c:\cosmos-node\node-data\slay3rd.log','c:\cosmos-node\node-data\logs\slay3rd.log')) { if (Test-Path $p) { $ml = $p; break } }
if (-not $ml) { $c2 = Get-ChildItem 'c:\cosmos-node\node-data' -Recurse -Filter '*.log' -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending | Select-Object -First 5; foreach ($c in $c2) { $log += 'log-candidate: ' + $c.FullName + ' ' + $c.LastWriteTime } }
if ($ml) {
    $log += "mainlog: $ml"
    $cutoff = (Get-Date).AddMinutes(-6).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ss')
    $t = Get-Content $ml -Tail 800
    $log += ('UNAVAIL last800: ' + ($t | Where-Object {$_ -match 'unavailable'}).Count)
    $log += ('REJECT last800: ' + ($t | Where-Object {$_ -match 'rejecting'}).Count)
    $log += ('EXEC last800: ' + ($t | Where-Object {$_ -match 'Block finalized'}).Count)
    $log += ('propose lines: ' + ($t | Where-Object {$_ -match 'propose: built BlockPayload'}).Count)
}
$log | Out-File -Encoding ascii $out
