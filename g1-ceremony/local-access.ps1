$out = 'c:\cosmos-node\node-data\config\CascadeProjects\windsurf-project\junoclaw-chain\g1-ceremony\local-access.out.txt'
$log = @()
$log += '=== ~/.junoclaw (wallet store) ==='
$jdir = Join-Path $env:USERPROFILE '.junoclaw'
if (Test-Path $jdir) { Get-ChildItem $jdir -Recurse -File | ForEach-Object { $log += $_.FullName + ' (' + $_.Length + ')' } } else { $log += 'none' }
$log += '=== ~/.ssh ==='
$ssh = Join-Path $env:USERPROFILE '.ssh'
if (Test-Path $ssh) { Get-ChildItem $ssh | ForEach-Object { $log += $_.Name } } else { $log += 'none' }
$log += '=== ssh config (host blocks) ==='
$cfg = Join-Path $ssh 'config'
if (Test-Path $cfg) { Get-Content $cfg | ForEach-Object { $log += $_ } } else { $log += 'no config' }
$log += '=== ssh to ovh test (BatchMode, 8s) ==='
try {
    $r = & ssh -o BatchMode=yes -o ConnectTimeout=8 -o StrictHostKeyChecking=accept-new root@51.195.201.74 'hostname; systemctl is-active junoclaw-g1' 2>&1
    $log += ($r -join "`n")
} catch { $log += 'ssh fail: ' + $_.Exception.Message }
$log | Out-File -Encoding ascii $out
