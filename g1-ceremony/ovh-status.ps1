$out = 'c:\cosmos-node\node-data\config\CascadeProjects\windsurf-project\junoclaw-chain\g1-ceremony\ovh-status.out.txt'
$key = Join-Path $env:USERPROFILE '.ssh\ovh_g1'
$remote = 'systemctl is-active junoclaw-g1; pgrep -a slay3rd; journalctl -u junoclaw-g1 -n 15 --no-pager'
& ssh -i $key -o BatchMode=yes -o ConnectTimeout=10 -o IdentitiesOnly=yes ubuntu@51.195.201.74 $remote 2>&1 | Out-File -Encoding ascii $out
