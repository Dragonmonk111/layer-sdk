# rc9 image: thin layer over the rc1 base (debian bookworm runtime, glibc fine).
# Binary must be the DOCKER build (linux, glibc 2.35) ??? NOT the WSL binary.
$ErrorActionPreference = 'Continue'
$ceremony = 'c:\cosmos-node\node-data\config\CascadeProjects\windsurf-project\junoclaw-chain\g1-ceremony'
$bin = Join-Path $ceremony 'slay3rd-rc9-glibc235'
$img = 'ghcr.io/dragonmonk111/junoclaw-chain:v0.6.0-rc9'
$ctx = Join-Path $ceremony '_img-rc9'
$out = Join-Path $ceremony 'img-rc9-push.out.txt'

$log = @()
if (-not (Test-Path $bin)) { $log += 'BINARY MISSING (docker glibc235 build not done)'; $log | Out-File $out; exit 1 }

New-Item -ItemType Directory -Force $ctx | Out-Null
Copy-Item $bin (Join-Path $ctx 'slay3rd') -Force
@'
FROM ghcr.io/dragonmonk111/junoclaw-chain:v0.6.0-rc1
COPY slay3rd /usr/local/bin/slay3rd
RUN chmod 755 /usr/local/bin/slay3rd
'@ | Set-Content (Join-Path $ctx 'Dockerfile') -Encoding ascii

$log += 'docker pull base...'
$log += (docker pull ghcr.io/dragonmonk111/junoclaw-chain:v0.6.0-rc1 2>&1 | Select-Object -Last 5)
$log += 'docker build...'
$log += (docker build -t $img $ctx 2>&1 | Select-Object -Last 8)
$log += 'verify sha inside image...'
$log += (docker run --rm --entrypoint sha256sum $img /usr/local/bin/slay3rd 2>&1)
$log += 'docker push...'
$log += (docker push $img 2>&1 | Select-Object -Last 10)
$log | Out-File -Encoding ascii $out
