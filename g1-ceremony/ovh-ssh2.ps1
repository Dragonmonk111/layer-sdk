$out = 'c:\cosmos-node\node-data\config\CascadeProjects\windsurf-project\junoclaw-chain\g1-ceremony\ovh-ssh2.out.txt'
$sshDir = Join-Path $env:USERPROFILE '.ssh'
$keys = @('ovh_g1','id_ed25519','id_rsa')
$users = @('root','ubuntu','debian','admin','junoclaw')
$log = @()
foreach ($k in $keys) {
    $kp = Join-Path $sshDir $k
    if (-not (Test-Path $kp)) { $log += "key ${k}: missing"; continue }
    foreach ($u in $users) {
        $r = & ssh -i $kp -o BatchMode=yes -o ConnectTimeout=6 -o StrictHostKeyChecking=accept-new -o IdentitiesOnly=yes "$u@51.195.201.74" "echo SSH_OK; hostname" 2>&1
        $ok = ($r -join ' ') -match 'SSH_OK'
        $log += "$k $u : " + $(if ($ok) { 'OK -> ' + ($r -join ' ') } else { 'denied (' + (($r | Select-Object -First 1) -join ' ') + ')' })
        if ($ok) { break }
    }
}
$log | Out-File -Encoding ascii $out
