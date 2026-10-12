$out = 'c:\cosmos-node\node-data\config\CascadeProjects\windsurf-project\junoclaw-chain\g1-ceremony\img-rc5-push.out.txt'
$ctx = 'c:\cosmos-node\node-data\config\CascadeProjects\windsurf-project\junoclaw-chain\g1-ceremony\_img-rc5'
$bin = 'c:\cosmos-node\node-data\config\CascadeProjects\windsurf-project\junoclaw-chain\g1-ceremony\slay3rd-rc5-glibc235'
$img = 'ghcr.io/dragonmonk111/junoclaw-chain:v0.6.0-rc5'
$log = @()

Copy-Item $bin (Join-Path $ctx 'slay3rd') -Force
$log += "staged binary: $((Get-Item $bin).Length) bytes"

$log += '=== docker pull rc1 base ==='
& docker pull ghcr.io/dragonmonk111/junoclaw-chain:v0.6.0-rc1 2>&1 | Select-Object -Last 4 | ForEach-Object { $log += [string]$_ }
$log += "pull exit: $LASTEXITCODE"
$log += '=== docker build rc5 ==='
& docker build -t $img $ctx 2>&1 | Select-Object -Last 15 | ForEach-Object { $log += [string]$_ }
$log += "build exit: $LASTEXITCODE"
if ($LASTEXITCODE -eq 0) {
  $log += '=== verify binary in image ==='
  $h = & docker run --rm --entrypoint sha256sum $img /usr/local/bin/slay3rd 2>&1
  $log += "img sha256: $h"
  $log += "local sha256: $((Get-FileHash $bin -Algorithm SHA256).Hash.ToLower())"
  $log += '=== docker push ==='
  & docker push $img 2>&1 | Select-Object -Last 10 | ForEach-Object { $log += [string]$_ }
  $log += "push exit: $LASTEXITCODE"
}
$log | Out-File -Encoding utf8 $out
$log
