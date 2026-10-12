$out = 'c:\cosmos-node\node-data\config\CascadeProjects\windsurf-project\junoclaw-chain\g1-ceremony\ovh-ssh.out.txt'
$key = Join-Path $env:USERPROFILE '.ssh\ovh_g1'
$log = @()
$log += 'key exists: ' + (Test-Path $key)
try {
    $r = & ssh -i $key -o BatchMode=yes -o ConnectTimeout=8 -o StrictHostKeyChecking=accept-new -o IdentitiesOnly=yes root@51.195.201.74 "hostname; systemctl is-active junoclaw-g1; ls -la /root/g1-node/ 2>/dev/null; ls -la /opt/g1-node/ 2>/dev/null; which docker; docker ps --format '{{.Names}} {{.Image}} {{.Status}}' 2>/dev/null" 2>&1
    $log += ($r -join "`n")
} catch { $log += 'ssh fail: ' + $_.Exception.Message }
$log | Out-File -Encoding ascii $out
