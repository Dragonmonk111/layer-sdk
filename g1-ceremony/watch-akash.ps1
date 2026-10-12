$out = 'c:\cosmos-node\node-data\config\CascadeProjects\windsurf-project\junoclaw-chain\g1-ceremony\watch-akash.out.txt'
$log = @()
for ($i = 1; $i -le 4; $i++) {
  $log += "=== probe round $i $(Get-Date -Format HH:mm:ss) ==="
  # TCP probe
  $tcp = New-Object Net.Sockets.TcpClient
  $task = $tcp.ConnectAsync('provider.dl2klab.co.za', 30823)
  if ($task.Wait(6000) -and $tcp.Connected) { $log += 'TCP 30823: CONNECTED' } else { $log += 'TCP 30823: TIMEOUT' }
  $tcp.Close()
  # resync progress via lease-logs tail (tip_height + finalized)
  $tail = & node 'c:\cosmos-node\node-data\config\CascadeProjects\windsurf-project\junoclaw\tools\akash\g1-lease-logs.mjs' 2>&1
  $tip = $tail | Select-String 'tip_height' | Select-Object -Last 1
  $fin = $tail | Select-String 'Block finalized|finaliz' | Select-Object -Last 1
  $inv = ($tail | Select-String 'invalid data received' | Measure-Object).Count
  $log += "last tip: $tip"
  $log += "last fin: $fin"
  $log += "invalid-data count in tail: $inv"
  if ($i -lt 4) { Start-Sleep -Seconds 120 }
}
$log | Out-File -Encoding utf8 $out
$log
