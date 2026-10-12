$out = 'c:\cosmos-node\node-data\config\CascadeProjects\windsurf-project\junoclaw-chain\g1-ceremony\voting-check2.out.txt'
$log = @()
# find live mayo_sigs.log in WSL home-vb data dir
$cands = @(
    '\\wsl$\Ubuntu-24.04\opt\g1-node\data\mayo_sigs.log',
    '\\wsl$\Ubuntu-24.04\root\g1-node\data\mayo_sigs.log',
    '\\wsl$\Ubuntu-24.04\opt\g1-node\mayo_sigs.log'
)
$siglog = $null
foreach ($c in $cands) { if (Test-Path $c) { $siglog = $c; break } }
$log += "siglog: $siglog"
if ($siglog) {
    $fi = Get-Item $siglog
    $log += ('size=' + $fi.Length + ' lastwrite=' + $fi.LastWriteTime)
    $tail = Get-Content $siglog -Tail 300
    $log += '=== last voting entries ==='
    $tail | Where-Object { $_ -match 'NOTARIZE|FINALIZE|nullify|vote' } | Select-Object -Last 30 | ForEach-Object { $log += $_.Substring(0,[Math]::Min(170,$_.Length)) }
    $log += ('NOTARIZE in tail300: ' + ($tail | Where-Object {$_ -match 'NOTARIZE'}).Count)
    $log += ('FINALIZE in tail300: ' + ($tail | Where-Object {$_ -match 'FINALIZE'}).Count)
}
# also pull the last 2 min of main log for vote/propose evidence
$log += '=== main log: recent vote evidence ==='
$fs = [System.IO.File]::Open('\\wsl$\Ubuntu-24.04\opt\g1-node\slay3rd.log','Open','Read','ReadWrite')
$len = [Math]::Min(300000, $fs.Length); $fs.Seek(-$len,'End') | Out-Null
$buf = New-Object byte[] $len; [void]$fs.Read($buf,0,$len); $fs.Close()
$txt = [System.Text.Encoding]::UTF8.GetString($buf)
$lines = $txt -split "`n"
$log += ('last1000: notarize-ish lines:')
$lines | Select-String 'notarize|Notarization|vote|certify' | Select-Object -Last 12 | ForEach-Object { $log += $_.Line.Trim().Substring(0,[Math]::Min(170,$_.Line.Trim().Length)) }
$log | Out-File -Encoding ascii $out
