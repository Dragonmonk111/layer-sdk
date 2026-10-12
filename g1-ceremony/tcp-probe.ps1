# tcp-probe.ps1 - test TCP reachability of the akash forwarded endpoint
param(
    [string]$TargetHost = "provider.dl2klab.co.za",
    [int]$Port = 30823,
    [int]$TimeoutMs = 8000
)
$c = New-Object System.Net.Sockets.TcpClient
try {
    $task = $c.ConnectAsync($TargetHost, $Port)
    if ($task.Wait($TimeoutMs)) {
        Write-Output "TCP $Port OPEN"
    } else {
        Write-Output "TCP $Port TIMEOUT (${TimeoutMs}ms)"
    }
} catch {
    Write-Output "TCP $Port FAIL: $($_.Exception.InnerException.Message)"
} finally {
    $c.Close()
}
