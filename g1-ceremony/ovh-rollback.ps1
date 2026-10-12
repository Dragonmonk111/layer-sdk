$out = 'c:\cosmos-node\node-data\config\CascadeProjects\windsurf-project\junoclaw-chain\g1-ceremony\ovh-rollback.out.txt'
$key = Join-Path $env:USERPROFILE '.ssh\ovh_g1'
$remote = 'sudo -n install -m 755 -o ubuntu -g ubuntu /home/ubuntu/slay3rd.rc1.bak /home/ubuntu/slay3rd; sudo -n systemctl restart junoclaw-g1; sleep 6; systemctl is-active junoclaw-g1; journalctl -u junoclaw-g1 -n 10 --no-pager'
& ssh -i $key -o BatchMode=yes -o ConnectTimeout=10 -o IdentitiesOnly=yes ubuntu@51.195.201.74 $remote 2>&1 | Out-File -Encoding ascii $out
