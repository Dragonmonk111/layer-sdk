$out = 'c:\cosmos-node\node-data\config\CascadeProjects\windsurf-project\junoclaw-chain\g1-ceremony\akash-endpoint.out.txt'
$log = @()
$log += '=== DNS ==='
try {
    $r = Resolve-DnsName provider.dl2klab.co.za -Type A -ErrorAction Stop
    $r | ForEach-Object { $log += ($_.Name + ' -> ' + $_.IPAddress) }
} catch { $log += 'DNS fail: ' + $_.Exception.Message }
$log += '=== TCP 30823 on hostname ==='
$log += (Test-NetConnection -ComputerName provider.dl2klab.co.za -Port 30823 -InformationLevel Quiet -WarningAction SilentlyContinue).ToString()
$log += '=== TCP 30823 on old IP ==='
$log += (Test-NetConnection -ComputerName 196.216.137.249 -Port 30823 -InformationLevel Quiet -WarningAction SilentlyContinue).ToString()
$log | Out-File -Encoding ascii $out
