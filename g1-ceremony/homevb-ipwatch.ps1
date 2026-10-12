# homevb-ipwatch.ps1 - public-IP drift watch for the home-vb seat.
# The validator's static peer config pins 84.9.189.220; if the ISP
# rotates the address, peers dial a stale lease (the akash lesson).
# Runs hourly via scheduled task "JunoClawHomeVbIpWatch"; logs to
# homevb-ipwatch.log. ALERT on drift or on unreachable echo service.

$ErrorActionPreference = 'Continue'
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$log  = Join-Path $here 'homevb-ipwatch.log'
$expected = '84.9.189.220'

function Log($msg) {
    $stamp = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    Add-Content -Path $log -Value "$stamp $msg"
}

$ip = $null
foreach ($svc in @('https://api.ipify.org', 'https://icanhazip.com')) {
    try { $ip = (Invoke-RestMethod -Uri $svc -TimeoutSec 10).Trim(); break } catch {}
}
if (-not $ip) { Log "WARN ip-echo-unreachable (no source answered)"; exit 1 }
if ($ip -ne $expected) { Log "ALERT ip-drift pinned=$expected live=$ip - static peers stale" }
else { Log "OK ip=$ip" }
