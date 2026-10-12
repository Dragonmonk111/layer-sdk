$key = Join-Path $env:USERPROFILE '.ssh\ovh_g1'
$cmd = @'
echo "=== first parent-not-executed occurrence ==="
journalctl -u junoclaw-g1 --no-pager | grep -m1 -B2 "parent not executed"
echo "=== last executed/tip_height ==="
journalctl -u junoclaw-g1 --no-pager | grep -iE "tip_height|executed block" | tail -5
echo "=== invalid data count + last 3 ==="
journalctl -u junoclaw-g1 --no-pager | grep -ci "invalid data"
journalctl -u junoclaw-g1 --no-pager | grep -i "invalid data" | tail -3
echo "=== serve/fetch lines last 3h ==="
journalctl -u junoclaw-g1 --since "3 hours ago" --no-pager | grep -iE "serve|fetch|backfill" | tail -10
echo "=== service start time / restarts ==="
systemctl show junoclaw-g1 -p ActiveEnterTimestamp -p NRestarts
'@
$b64 = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($cmd))
$r = & ssh -i $key -o BatchMode=yes -o ConnectTimeout=10 -o IdentitiesOnly=yes ubuntu@51.195.201.74 "echo $b64 | base64 -d | bash" 2>&1
$r -join "`n"
