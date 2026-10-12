$key = Join-Path $env:USERPROFILE '.ssh\ovh_g1'
$cmd = 'journalctl -u junoclaw-g1 -n 500 --no-pager | grep -iE "peer|connect|fetch|invalid|height" | tail -40'
$b64 = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($cmd))
$r = & ssh -i $key -o BatchMode=yes -o ConnectTimeout=10 -o IdentitiesOnly=yes ubuntu@51.195.201.74 "echo $b64 | base64 -d | bash" 2>&1
$r -join "`n"
