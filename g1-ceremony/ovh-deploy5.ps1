$out = 'c:\cosmos-node\node-data\config\CascadeProjects\windsurf-project\junoclaw-chain\g1-ceremony\ovh-deploy5.out.txt'
$key = Join-Path $env:USERPROFILE '.ssh\ovh_g1'
$bin = 'c:\cosmos-node\node-data\config\CascadeProjects\windsurf-project\junoclaw-chain\g1-ceremony\slay3rd-rc6-glibc235'
$seed = 'c:\cosmos-node\node-data\config\CascadeProjects\windsurf-project\junoclaw-chain\g1-ceremony\fdf4ab2e.bin'
$log = @()

if (-not (Test-Path $bin)) { $log += 'BINARY MISSING'; $log | Out-File $out; exit 1 }
if (-not (Test-Path $seed)) { $log += 'SEED MISSING'; $log | Out-File $out; exit 1 }
$log += ('binary: ' + (Get-Item $bin).Length + ' bytes  sha256: ' + (Get-FileHash $bin -Algorithm SHA256).Hash)
$log += ('seed:   ' + (Get-Item $seed).Length + ' bytes  sha256: ' + (Get-FileHash $seed -Algorithm SHA256).Hash)

$log += 'scp binary+seed...'
$log += (& scp -i $key -o BatchMode=yes -o ConnectTimeout=10 -o IdentitiesOnly=yes $bin 'ubuntu@51.195.201.74:~/slay3rd.rc6' 2>&1)
$log += (& scp -i $key -o BatchMode=yes -o ConnectTimeout=10 -o IdentitiesOnly=yes $seed 'ubuntu@51.195.201.74:~/fdf4ab2e.bin' 2>&1)

# install seed dir, verify hashes, swap binary, restart
$remote = @'
set -e
sha256sum ~/slay3rd.rc6 ~/fdf4ab2e.bin
sudo -n mkdir -p /data/seed
sudo -n install -m 644 /home/ubuntu/fdf4ab2e.bin /data/seed/fdf4ab2e.bin
sha256sum /data/seed/fdf4ab2e.bin 2>/dev/null || sudo -n sha256sum /data/seed/fdf4ab2e.bin
sudo -n systemctl stop junoclaw-g1
sleep 3
sudo -n cp /home/ubuntu/slay3rd /home/ubuntu/slay3rd.rc5.bak
sudo -n install -m 755 -o ubuntu -g ubuntu /home/ubuntu/slay3rd.rc6 /home/ubuntu/slay3rd
sudo -n systemctl start junoclaw-g1
sleep 10
systemctl is-active junoclaw-g1
journalctl -u junoclaw-g1 -n 15 --no-pager
'@
$log += '=== remote ==='
$log += (& ssh -i $key -o BatchMode=yes -o ConnectTimeout=10 -o IdentitiesOnly=yes ubuntu@51.195.201.74 $remote 2>&1)
$log | Out-File -Encoding ascii $out
