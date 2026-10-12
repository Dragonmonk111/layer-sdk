# Deploy rc8 to ovh-uk1 donor — binary swap + restart. rc8 adds the
# block() payload-only fallback so the seat's halted-chain tip fetch
# succeeds. Seed already installed from rc6 deploy.
$out = 'c:\cosmos-node\node-data\config\CascadeProjects\windsurf-project\junoclaw-chain\g1-ceremony\ovh-deploy8.out.txt'
$key = Join-Path $env:USERPROFILE '.ssh\ovh_g1'
$bin = 'c:\cosmos-node\node-data\config\CascadeProjects\windsurf-project\junoclaw-chain\g1-ceremony\slay3rd-rc8-glibc235'
$log = @()

if (-not (Test-Path $bin)) { $log += 'BINARY MISSING'; $log | Out-File $out; exit 1 }
$log += ('binary: ' + (Get-Item $bin).Length + ' bytes  sha256: ' + (Get-FileHash $bin -Algorithm SHA256).Hash)

$log += 'scp binary...'
$log += (& scp -i $key -o BatchMode=yes -o ConnectTimeout=10 -o IdentitiesOnly=yes $bin 'ubuntu@51.195.201.74:~/slay3rd.rc8' 2>&1)

# verify hash, swap binary, restart
$remote = @'
set -e
sha256sum ~/slay3rd.rc8
sudo -n systemctl stop junoclaw-g1
sleep 3
sudo -n cp /home/ubuntu/slay3rd /home/ubuntu/slay3rd.rc7.bak 2>/dev/null || true
sudo -n install -m 755 -o ubuntu -g ubuntu /home/ubuntu/slay3rd.rc8 /home/ubuntu/slay3rd
sudo -n systemctl start junoclaw-g1
sleep 10
systemctl is-active junoclaw-g1
journalctl -u junoclaw-g1 -n 15 --no-pager
'@
$log += '=== remote ==='
$log += (& ssh -i $key -o BatchMode=yes -o ConnectTimeout=10 -o IdentitiesOnly=yes ubuntu@51.195.201.74 $remote 2>&1)
$log | Out-File -Encoding ascii $out
