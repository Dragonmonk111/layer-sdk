$out = 'c:\cosmos-node\node-data\config\CascadeProjects\windsurf-project\junoclaw-chain\g1-ceremony\ovh-sysinfo.out.txt'
$key = Join-Path $env:USERPROFILE '.ssh\ovh_g1'
$remote = 'lsb_release -a 2>/dev/null; ldd --version | head -1; docker --version 2>/dev/null || echo "no docker"; nproc; free -h | head -2; df -h / | tail -1'
& ssh -i $key -o BatchMode=yes -o ConnectTimeout=10 -o IdentitiesOnly=yes ubuntu@51.195.201.74 $remote 2>&1 | Out-File -Encoding ascii $out
