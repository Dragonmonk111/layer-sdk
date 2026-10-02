# C9 24h soak: chaos events (leader kill / partition / restart-recreate) on a
# rotating validator while logging chain liveness. Paired with monitor-s4.ps1.
# NOTE: byzantine-proposer leg requires a patched image — not covered here.
param(
    [int]$DurationHours = 24,
    [int]$ChaosEveryMin = 60   # chaos event every ~60-120 min (randomized)
)
$net  = 'devnet_junoclaw-devnet'
$log  = 'soak-c9.log'
$deadline = (Get-Date).AddHours($DurationHours)
$nextChaos = (Get-Date).AddMinutes(20)  # settle period first
$rng = [System.Random]::new()
$eventNo = 0

function Tip {
    $tip = 0
    foreach ($i in 0..3) {
        $l = docker logs "junoclaw-node-$i" --tail 30 2>&1 |
             Where-Object { $_ -match 'certificate stored' } | Select-Object -Last 1
        if ($l -and "$l" -match 'height') {
            foreach ($t in (("$l" -replace "$([char]27)\[[0-9;]*m", '') -split '\s+')) {
                if ($t.StartsWith('height=')) {
                    $h = [int64]$t.Substring(7); if ($h -gt $tip) { $tip = $h }
                }
            }
        }
    }
    $tip
}

Add-Content $log "=== C9 soak start $(Get-Date -Format u) tip=$(Tip) ==="
while ((Get-Date) -lt $deadline) {
    if ((Get-Date) -ge $nextChaos) {
        $eventNo++
        $victim = $rng.Next(0, 4)
        $kind = $rng.Next(0, 4)  # 0=kill-restart, 1=partition, 2=recreate, 3=byzantine-proposer
        $tipBefore = Tip
        $start = Get-Date -Format u
        switch ($kind) {
            0 { Add-Content $log "$start EVENT#$eventNo kill node-$victim (tip=$tipBefore)"
                docker kill "junoclaw-node-$victim" | Out-Null
                Start-Sleep (30 + $rng.Next(0, 60))
                docker start "junoclaw-node-$victim" | Out-Null }
            1 { $dur = 90 + $rng.Next(0, 90)
                $pinIP = "172.28.0.$($victim + 10)"   # compose pins node-N to 172.28.0.(10+N)
                Add-Content $log "$start EVENT#$eventNo partition node-$victim for ${dur}s (tip=$tipBefore)"
                docker network disconnect $net "junoclaw-node-$victim" 2>$null | Out-Null
                Start-Sleep $dur
                # MUST restore the pinned IP — bare `docker network connect` reassigns
                # a fresh IP, breaking the static [[peers]] addressing on all nodes
                # (observed 2026-10-02: node-3 got 172.28.0.2, wedged silent 38min).
                docker network connect --ip $pinIP $net "junoclaw-node-$victim" 2>$null | Out-Null }
            2 { Add-Content $log "$start EVENT#$eventNo recreate node-$victim (tip=$tipBefore)"
                docker compose -f devnet/docker-compose.yml up -d --force-recreate "node-$victim" 2>&1 | Out-Null }
            3 { # Byzantine-proposer leg: victim proposes payloads with a
                # corrupted state_root. Honest validators' verify() must
                # reject them; those views time out and honest leaders keep
                # the chain live. REQUIRES image with fault_inject support
                # (unknown toml keys are ignored otherwise — leg degrades to
                # a plain restart on the old image).
                $cfg = "devnet/config/node-$victim.toml"
                Add-Content $log "$start EVENT#$eventNo byzantine node-$victim fault_inject=bad_state_root (tip=$tipBefore)"
                Add-Content $cfg "`nfault_inject = `"bad_state_root`""
                docker restart "junoclaw-node-$victim" | Out-Null
                Start-Sleep (120 + $rng.Next(0, 60))
                (Get-Content $cfg) | Where-Object { $_ -notmatch '^\s*fault_inject\s*=' } | Set-Content $cfg
                docker restart "junoclaw-node-$victim" | Out-Null }
        }
        # measure recovery: victim's height resumes advancing within 5 min
        Start-Sleep 60
        $tipAfter = Tip
        Add-Content $log "$(Get-Date -Format u) EVENT#$eventNo done tip=$tipAfter (delta=$($tipAfter - $tipBefore))"
        $nextChaos = (Get-Date).AddMinutes($ChaosEveryMin + $rng.Next(0, 60))
    }
    Add-Content $log "$(Get-Date -Format u) heartbeat tip=$(Tip)"
    Start-Sleep 300
}
Add-Content $log "=== C9 soak end $(Get-Date -Format u) events=$eventNo tip=$(Tip) ==="
