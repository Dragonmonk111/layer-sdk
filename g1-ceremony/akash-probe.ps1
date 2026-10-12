$out = 'c:\cosmos-node\node-data\config\CascadeProjects\windsurf-project\junoclaw-chain\g1-ceremony\akash-probe.out.txt'
$log = @()
# 1. SDL non-secret fields
$sdl = Get-Content '\\wsl$\Ubuntu-24.04\root\..\wsl.localhost' -ErrorAction SilentlyContinue
$sdlPath = 'c:\cosmos-node\node-data\config\CascadeProjects\windsurf-project\junoclaw-chain\g1-ceremony\_secrets\deploy-akash-seat.yaml'
$log += '=== SDL non-secret lines ==='
if (Test-Path $sdlPath) {
    Get-Content $sdlPath | Where-Object { $_ -notmatch 'B64|KEYS|KEY' -and $_ -match 'image:|expose|port:|to:|accept|denom|amount|cpu|memory|storage|profile|service|dseq|provider|version|placement' } | ForEach-Object { $log += $_ }
} else { $log += 'SDL not found' }
# 2. search workspace for akash1 addresses / dseq references
$log += '=== akash1/dseq refs in workspace ==='
$hits = Get-ChildItem -Recurse 'c:\cosmos-node\node-data\config\CascadeProjects\windsurf-project\junoclaw-chain' -Include '*.md','*.txt','*.json','*.mjs','*.yaml','*.toml' -File -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -notmatch '_secrets|keys\.json|\.tar\.gpg' -and $_.Length -lt 2000000 } |
    Select-String -Pattern 'akash1[a-z0-9]{38}|dseq|deployment.*akash|akash.*deploy' -List -ErrorAction SilentlyContinue
foreach ($h in $hits) { $log += ($h.Path -split 'junoclaw-chain\\')[-1] + ' :: ' + ($h.Line.Trim().Substring(0,[Math]::Min(120,$h.Line.Trim().Length))) }
$log | Out-File -Encoding ascii $out
