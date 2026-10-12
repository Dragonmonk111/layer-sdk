$key = Join-Path $env:USERPROFILE '.ssh\ovh_g1'
$cmd = 'BIN=$(systemctl show -p ExecStart --value junoclaw-g1 | grep -oE "/[^ ;]+" | head -1); echo "bin path: $BIN"; sha256sum "$BIN"; echo "---latest tip---"; journalctl -u junoclaw-g1 -n 40 --no-pager | grep -iE "tip_height|executed|invalid|fetch|serve" | tail -12'
$b64 = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($cmd))
$r = & ssh -i $key -o BatchMode=yes -o ConnectTimeout=10 -o IdentitiesOnly=yes ubuntu@51.195.201.74 "echo $b64 | base64 -d | bash" 2>&1
$r -join "`n"
