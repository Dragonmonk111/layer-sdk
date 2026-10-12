$out = 'c:\cosmos-node\node-data\config\CascadeProjects\windsurf-project\junoclaw-chain\g1-ceremony\voting-check3.out.txt'
$log = @()
$sig = '\\wsl$\Ubuntu-24.04\opt\g1-node\data\mayo_sigs.log'
$log += ('sig exists: ' + (Test-Path $sig))
if (Test-Path $sig) {
    $fi = Get-Item $sig
    $log += ('size=' + $fi.Length + ' lastwrite=' + $fi.LastWriteTime)
    $tail = Get-Content $sig -Tail 60
    $log += '=== last 30 entries ==='
    $tail | Select-Object -Last 30 | ForEach-Object { $log += $_.Substring(0,[Math]::Min(170,$_.Length)) }
}
# correlate: extract timestamps+heights of recent 'certify: payload unavailable' vs successful certifies
$fs = [System.IO.File]::Open('\\wsl$\Ubuntu-24.04\opt\g1-node\slay3rd.log','Open','Read','ReadWrite')
$len = [Math]::Min(1500000, $fs.Length); $fs.Seek(-$len,'End') | Out-Null
$buf = New-Object byte[] $len; [void]$fs.Read($buf,0,$len); $fs.Close()
$lines = ([System.Text.Encoding]::UTF8.GetString($buf)) -split "`n"
$log += '=== certify lines (last 15, with height if present) ==='
$lines | Select-String 'certify' | Select-Object -Last 15 | ForEach-Object { $log += ($_.Line -replace '\x1b\[[0-9;]*m','').Trim().Substring(0,[Math]::Min(180,(($_.Line -replace '\x1b\[[0-9;]*m','').Trim()).Length)) }
$log += '=== recent finalize successes ==='
$lines | Select-String 'Block finalized' | Select-Object -Last 6 | ForEach-Object { $log += ($_.Line -replace '\x1b\[[0-9;]*m','').Trim().Substring(0,[Math]::Min(160,(($_.Line -replace '\x1b\[[0-9;]*m','').Trim()).Length)) }
$log += ('certify-unavail count in 1.5MB tail: ' + ($lines | Select-String 'certify: payload unavailable').Count)
$log += ('finalize-unavail count: ' + ($lines | Select-String 'finalized payload unavailable').Count)
$log | Out-File -Encoding ascii $out
