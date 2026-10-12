$out = 'c:\cosmos-node\node-data\config\CascadeProjects\windsurf-project\junoclaw-chain\g1-ceremony\ovh-deploy.out.txt'
$key = Join-Path $env:USERPROFILE '.ssh\ovh_g1'
$log = @()

# 1. Copy rc5 binary out of WSL to a Windows temp staging area
$src = '\\wsl$\Ubuntu-24.04\root\junoclaw-chain\target\release\slay3rd'
$stage = Join-Path $env:TEMP 'slay3rd-rc5'
$log += 'copying binary from WSL...'
Copy-Item -LiteralPath $src -Destination $stage -Force
$log += ('staged: ' + (Get-Item $stage).Length + ' bytes')

# 2. scp to ovh
$log += 'scp to ovh...'
$scp = & scp -i $key -o BatchMode=yes -o ConnectTimeout=10 -o IdentitiesOnly=yes $stage 'ubuntu@51.195.201.74:~/slay3rd.rc5' 2>&1
$log += ($scp -join "`n")

# 3. Verify checksum on remote, then stop service, swap, start
$remote = 'set -e; ls -la ~/slay3rd.rc5; sha256sum ~/slay3rd.rc5; sha256sum /home/ubuntu/slay3rd; sudo -n cp /home/ubuntu/slay3rd /home/ubuntu/slay3rd.rc1.bak; sudo -n cp ~/slay3rd.rc5 /home/ubuntu/slay3rd; sudo -n chmod +x /home/ubuntu/slay3rd; sudo -n systemctl restart junoclaw-g1; sleep 5; systemctl is-active junoclaw-g1; journalctl -u junoclaw-g1 --since "1 min ago" --no-pager | tail -8'
$log += '=== remote deploy ==='
$r = & ssh -i $key -o BatchMode=yes -o ConnectTimeout=10 -o IdentitiesOnly=yes ubuntu@51.195.201.74 $remote 2>&1
$log += ($r -join "`n")
$log | Out-File -Encoding ascii $out
