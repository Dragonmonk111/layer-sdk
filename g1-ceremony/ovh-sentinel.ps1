# ovh-sentinel.ps1 - read-only health battery for ovh-uk1 (primary G2 seat).
# Runs every 30 min via scheduled task "JunoClawOvhSentinel"; logs to
# ovh-sentinel.log in this folder. Alerts on: service down, journal
# ERRORs, clock unsynced, root disk >70% used, RAM available <1 GiB,
# slay3rd RSS >6 GiB, 1-min load >2.0.
# Review cadence: daily - RSS/HWM trend (leak watch), disk growth vs
# the ~75 MB/day model; weekly - journald vacuum if >1G (pending the
# journald.conf cap in the rc10 window).
# All checks are read-only. Holds no key material beyond the ssh key.
# The remote battery (ovh-sentinel-remote.sh) is piped to `bash -s`
# over stdin - no ssh argument quoting involved.

$ErrorActionPreference = 'Continue'
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$log  = Join-Path $here 'ovh-sentinel.log'
$remoteSh = Join-Path $here 'ovh-sentinel-remote.sh'

function Log($msg) {
    $stamp = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    Add-Content -Path $log -Value "$stamp $msg"
}

$script = (Get-Content $remoteSh -Raw) -replace "`r", ""
$out = $script | ssh -i "$env:USERPROFILE\.ssh\ovh_g1" -o BatchMode=yes -o ConnectTimeout=10 -o IdentitiesOnly=yes ubuntu@51.195.201.74 "bash -s" 2>$null
if (-not $out) { Log "ALERT ssh-unreachable ovh-uk1 (no output)"; exit 1 }

$v = @{}
foreach ($line in $out) {
    if ("$line".Trim() -match '^=(\w+) (.*)$') { $v[$Matches[1]] = $Matches[2].Trim() }
}

$alerts = @()
if ($v['SVC'] -ne 'active') { $alerts += "service=$($v['SVC'])" }
if ($v['ERR'] -and [int]$v['ERR'] -gt 0) { $alerts += "journalErrors=$($v['ERR'])" }
if ($v['SYNC'] -and [int]$v['SYNC'] -lt 1) { $alerts += "clock-unsynced" }
if ($v['DISK'] -match '(\d+)%') {
    if ([int]$Matches[1] -gt 70) { $alerts += "disk=$($v['DISK'])" }
}
if ($v['MEM'] -match '^(?<t>\d+)/(?<u>\d+)/(?<a>\d+)$') {
    if ([int]$Matches['a'] -lt 1024) { $alerts += "ramAvailMB=$($Matches['a'])" }
}
if ($v['RSS']) {
    $rssMB = [math]::Round([double]$v['RSS'] / 1024)
    if ($rssMB -gt 6144) { $alerts += "rssMB=$rssMB" }
}
if ($v['LOAD'] -and [double]$v['LOAD'] -gt 2.0) { $alerts += "load=$($v['LOAD'])" }

$rssMB = if ($v['RSS']) { [math]::Round([double]$v['RSS'] / 1024) } else { '?' }
$hwmMB = if ($v['HWM']) { [math]::Round([double]$v['HWM'] / 1024) } else { '?' }
$summary = "SVC=$($v['SVC']) load=$($v['LOAD']) mem(t/u/a)=$($v['MEM']) disk(u/pct/avail)=$($v['DISK']) rssMB=$rssMB hwmMB=$hwmMB sync=$($v['SYNC']) err30m=$($v['ERR'])"
if ($alerts.Count -gt 0) { Log "ALERT $summary :: $($alerts -join ', ')" }
else { Log "OK $summary" }
