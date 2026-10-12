# register-host-tasks.ps1 - one-shot registration of host-level tasks.
# Run once; re-running re-registers (-Force). No elevation needed:
# both tasks run as the logged-on user.

$ceremony = 'C:\cosmos-node\node-data\config\CascadeProjects\windsurf-project\junoclaw-chain\g1-ceremony'

$a1 = New-ScheduledTaskAction -Execute 'powershell.exe' `
    -Argument "-NoProfile -ExecutionPolicy Bypass -File `"$ceremony\homevb-ipwatch.ps1`""
$t1 = New-ScheduledTaskTrigger -Once -At (Get-Date).AddMinutes(2) `
    -RepetitionInterval (New-TimeSpan -Minutes 60)
Register-ScheduledTask -TaskName 'JunoClawHomeVbIpWatch' -Action $a1 -Trigger $t1 `
    -Description 'Hourly public-IP drift watch for home-vb (pinned 84.9.189.220)' -Force | Out-Null

$a2 = New-ScheduledTaskAction -Execute 'C:\Program Files\Oracle\VirtualBox\VBoxManage.exe' `
    -Argument 'startvm Ubuntu2204-Node --type headless'
$t2 = New-ScheduledTaskTrigger -AtLogOn
Register-ScheduledTask -TaskName 'JunoClawHomeVbAutostart' -Action $a2 -Trigger $t2 `
    -Description 'Start home-vb validator VM at user logon' -Force | Out-Null

Get-ScheduledTask -TaskName 'JunoClawHomeVb*' | Select-Object TaskName, State | Format-Table -AutoSize
