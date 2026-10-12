$out = 'c:\cosmos-node\node-data\config\CascadeProjects\windsurf-project\junoclaw-chain\g1-ceremony\ovh-deploy4.out.txt'
$key = Join-Path $env:USERPROFILE '.ssh\ovh_g1'
$bin = 'c:\cosmos-node\node-data\config\CascadeProjects\windsurf-project\junoclaw-chain\g1-ceremony\slay3rd-rc5-glibc235'
$log = @()

if (-not (Test-Path $bin)) { $log += 'BINARY MISSING'; $log | Out-File $out; exit 1 }
$log += ('local binary: ' + (Get-Item $bin).Length + ' bytes')
$log += ('local sha256: ' + (Get-FileHash $bin -Algorithm SHA256).Hash)

# scp to ovh
$log += 'scp...'
$scp = & scp -i $key -o BatchMode=yes -o ConnectTimeout=10 -o IdentitiesOnly=yes $bin 'ubuntu@51.195.201.74:~/slay3rd.rc5g' 2>&1
$log += ($scp -join "`n")

# stop -> verify hash -> swap -> start
$remote = 'set -e; sha256sum ~/slay3rd.rc5g; sudo -n systemctl stop junoclaw-g1; sleep 3; sudo -n cp /home/ubuntu/slay3rd /home/ubuntu/slay3rd.rc1.bak2; sudo -n install -m 755 -o ubuntu -g ubuntu /home/ubuntu/slay3rd.rc5g /home/ubuntu/slay3rd; sudo -n systemctl start junoclaw-g1; sleep 8; systemctl is-active junoclaw-g1; journalctl -u junoclaw-g1 -n 12 --no-pager'
$log += '=== remote ==='
$r = & ssh -i $key -o BatchMode=yes -o ConnectTimeout=10 -o IdentitiesOnly=yes ubuntu@51.195.201.74 $remote 2>&1
$log += ($r -join "`n")
$log | Out-File -Encoding ascii $out
