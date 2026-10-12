$key = Join-Path $env:USERPROFILE '.ssh\ovh_g1'
$cmd = 'journalctl -u junoclaw-g1 --no-pager | grep -m3 "genesis file loaded"; echo "---genesis-sha-direct---"; sha256sum $(grep -oE "genesis_path *= *\"[^\"]+" /home/ubuntu/junoclaw-g1/node-*.toml 2>/dev/null | head -1 | cut -d\" -f2) 2>/dev/null; echo "---toml-genesis-path---"; grep -n "genesis" /home/ubuntu/junoclaw-g1/node-*.toml 2>/dev/null | head -5'
$b64 = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($cmd))
$r = & ssh -i $key -o BatchMode=yes -o ConnectTimeout=10 -o IdentitiesOnly=yes ubuntu@51.195.201.74 "echo $b64 | base64 -d | bash" 2>&1
$r -join "`n"
