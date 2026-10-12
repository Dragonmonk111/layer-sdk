$out = 'c:\cosmos-node\node-data\config\CascadeProjects\windsurf-project\junoclaw-chain\g1-ceremony\ovh-deploy2.out.txt'
$key = Join-Path $env:USERPROFILE '.ssh\ovh_g1'
$remote = 'set -e; sudo -n systemctl stop junoclaw-g1; sleep 2; cp /home/ubuntu/slay3rd /home/ubuntu/slay3rd.rc1.bak; cp /home/ubuntu/slay3rd.rc5 /home/ubuntu/slay3rd; chmod +x /home/ubuntu/slay3rd; sudo -n systemctl start junoclaw-g1; sleep 5; systemctl is-active junoclaw-g1; journalctl -u junoclaw-g1 --since "1 min ago" --no-pager | tail -10'
& ssh -i $key -o BatchMode=yes -o ConnectTimeout=10 -o IdentitiesOnly=yes ubuntu@51.195.201.74 $remote 2>&1 | Out-File -Encoding ascii $out
