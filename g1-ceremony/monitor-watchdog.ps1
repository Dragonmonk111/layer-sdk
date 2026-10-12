# monitor-watchdog.ps1 - keeps g1-monitor.mjs alive on the coordinator.
# Looks for a node.exe running g1-monitor.mjs; if absent, restarts it
# detached (hidden window) with the soak args and logs to watchdog.log.
# Registered as scheduled task "JunoClawG1MonitorWatchdog" (5-min cadence).
# At soak close-out: schtasks /Change /TN JunoClawG1MonitorWatchdog /DISABLE
# BEFORE stopping the monitor, or the watchdog will resurrect it.
$ErrorActionPreference = 'Stop'

$here   = Split-Path -Parent $MyInvocation.MyCommand.Path
$log    = Join-Path $here 'watchdog.log'
$monitor = Join-Path $here 'g1-monitor.mjs'
$soakLog = Join-Path $here 'soak-t12.log'
$console = Join-Path $here 'soak-monitor-console.log'
$errLog  = Join-Path $here 'soak-monitor-err.log'

function Log($msg) {
    $stamp = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    Add-Content -Path $log -Value "$stamp $msg"
}

$alive = Get-CimInstance Win32_Process -Filter "name='node.exe'" |
    Where-Object { $_.CommandLine -like '*g1-monitor.mjs*' }
if ($alive) { exit 0 }

Log "monitor not running - restarting (detached, hidden)"
try {
    Start-Process -FilePath 'node' -WindowStyle Hidden `
        -ArgumentList "`"$monitor`"", '--log', "`"$soakLog`"", '--interval', '30', '--stall-seconds', '90' `
        -RedirectStandardOutput $console -RedirectStandardError $errLog -PassThru | Out-Null
    Log "monitor restarted ok"
} catch {
    Log "restart FAILED: $($_.Exception.Message)"
}
