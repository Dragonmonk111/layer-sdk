$key = Join-Path $env:USERPROFILE '.ssh\ovh_g1'
$cmd = @'
echo "=== last Block finalized ==="
journalctl -u junoclaw-g1 --no-pager | grep "Block finalized" | tail -3
echo "=== payload unavailable stalls ==="
journalctl -u junoclaw-g1 --no-pager | grep -c "finalized payload unavailable"
journalctl -u junoclaw-g1 --no-pager | grep "finalized payload unavailable" | tail -3
echo "=== first parent-not-executed since Oct 09 19:00 with context ==="
journalctl -u junoclaw-g1 --since "2026-10-09 19:00" --no-pager | grep -m1 -B8 "parent not executed"
echo "=== around wedge onset 07:40-08:00 today ==="
journalctl -u junoclaw-g1 --since "2026-10-10 07:30" --until "2026-10-10 08:05" --no-pager | grep -vE "nullif|vote|notar" | tail -30
'@
$b64 = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($cmd))
$r = & ssh -i $key -o BatchMode=yes -o ConnectTimeout=10 -o IdentitiesOnly=yes ubuntu@51.195.201.74 "echo $b64 | base64 -d | bash" 2>&1
($r -join "`n") | Out-File -Encoding utf8 (Join-Path $PSScriptRoot 'ovh-wedge.txt')
