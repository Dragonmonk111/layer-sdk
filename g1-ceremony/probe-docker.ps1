$out = 'c:\cosmos-node\node-data\config\CascadeProjects\windsurf-project\junoclaw-chain\g1-ceremony\probe-docker.out.txt'
$log = @()
$log += "=== docker.exe ==="
try {
  $v = & docker.exe version --format '{{.Server.Version}}' 2>&1
  $log += ($v -join "`n")
} catch { $log += "docker.exe failed: $_" }
$log += "=== docker images (local junoclaw) ==="
$imgs = & docker.exe images --format '{{.Repository}}:{{.Tag}} {{.Size}}' 2>&1 | Select-String -Pattern 'junoclaw|slay3rd'
$log += ($imgs -join "`n")
$log += "=== docker creds ==="
$cfg = "$env:USERPROFILE\.docker\config.json"
if (Test-Path $cfg) {
  $j = Get-Content $cfg -Raw | ConvertFrom-Json
  $log += ("auths: " + ($j.auths.PSObject.Properties.Name -join ', '))
  $log += ("credsStore: " + $j.credsStore)
} else { $log += "no $cfg" }
$log += "=== rc1 manifest inspect (anon) ==="
$m = & docker.exe manifest inspect ghcr.io/dragonmonk111/junoclaw-chain:v0.6.0-rc1 2>&1 | Select-Object -First 6
$log += ($m -join "`n")
$log | Out-File -Encoding utf8 $out
Get-Content $out
