$out = 'c:\cosmos-node\node-data\config\CascadeProjects\windsurf-project\junoclaw-chain\g1-ceremony\ovh-inspect.out.txt'
$key = Join-Path $env:USERPROFILE '.ssh\ovh_g1'
$remote = 'hostname; echo "--- systemd ---"; systemctl status junoclaw-g1 --no-pager -l 2>&1 | head -12; echo "--- docker ---"; docker ps -a 2>/dev/null; echo "--- dirs ---"; ls -la /home/ubuntu/g1-node/ 2>/dev/null; ls -la /opt/g1-node/ 2>/dev/null; ls -la ~/g1-node/ 2>/dev/null; echo "--- slay3rd procs ---"; pgrep -a slay3rd; echo "--- node.toml location ---"; find / -name node.toml -not -path "*/proc/*" 2>/dev/null | head -5'
& ssh -i $key -o BatchMode=yes -o ConnectTimeout=10 -o IdentitiesOnly=yes ubuntu@51.195.201.74 $remote 2>&1 | Out-File -Encoding ascii $out
