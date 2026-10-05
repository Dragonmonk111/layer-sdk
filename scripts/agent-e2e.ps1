param(
    [ValidateSet('setup', 'agent', 'custody', 'escrow', 'probe', 'prov', 'summary', 'all')]
    [string]$Phase = 'all',
    [string]$Grpc = '127.0.0.1:9090',
    [string]$Deployed = 'snapshot\agent-stack-deployed.json',
    [string]$StateFile = 'snapshot\agent-e2e-state.json'
)

$ErrorActionPreference = 'Continue'
$TX = '.\target\release\tx-sender.exe'
$Denom = 'ujclaw'
$cfg = Get-Content $Deployed -Raw | ConvertFrom-Json
$AR = $cfg.'agent_registry.address'; $TL = $cfg.'task_ledger.address'; $ES = $cfg.'escrow.address'
$MB = $cfg.'moultbook_v0.address'; $AC = $cfg.'agent_company.address'; $TM = $cfg.'truth_market.address'
$SR = $cfg.'skill_registry.address'; $MP = $cfg.'marketplace.address'

$Seeds = [ordered]@{
    owner = 'e2e-agent-owner'; req = 'e2e-requester'
    v1 = 'e2e-verifier-1'; v2 = 'e2e-verifier-2'; v3 = 'e2e-verifier-3'
    atk = 'e2e-attacker'
}
$MsgDir = Join-Path $env:TEMP 'agent-e2e-msgs'
New-Item -ItemType Directory -Force -Path $MsgDir | Out-Null
$LogFile = Join-Path $env:TEMP 'agent-e2e.log'
$script:pass = 0; $script:fail = 0; $script:height = 0

$S = @{}
if (Test-Path $StateFile) {
    (Get-Content $StateFile -Raw | ConvertFrom-Json).PSObject.Properties | ForEach-Object { $S[$_.Name] = $_.Value }
}
function Save-S { $S | ConvertTo-Json -Depth 5 | Set-Content -Path $StateFile -Encoding UTF8 }
function Log([string]$m) { Write-Host $m; Add-Content -Path $LogFile -Value $m }

function Invoke-Tx([string]$Seed, [string[]]$TxArgs) {
    if ($Seed) { $env:TX_SENDER_KEY_SEED = $Seed } else { Remove-Item Env:TX_SENDER_KEY_SEED -ErrorAction SilentlyContinue }
    try { $o = & $TX @TxArgs '--grpc' $Grpc 2>&1 | Out-String }
    finally { Remove-Item Env:TX_SENDER_KEY_SEED -ErrorAction SilentlyContinue }
    return $o
}

function Wait-Commit([string]$Out) {
    $r = @{ ok = $false; code = -1; log = ''; height = 0; hash = '' }
    if ($Out -match 'BroadcastTx response: code=(\d+), log=([^\r\n]*)') {
        if ([int]$Matches[1] -ne 0) { $r.code = [int]$Matches[1]; $r.log = $Matches[2].Trim(); return $r }
    }
    if ($Out -match 'txhash: ([0-9A-Fa-f]{64})') { $r.hash = $Matches[1] } else { $r.log = 'no txhash'; return $r }
    $g = Invoke-Tx '' @('get-tx', '--hash', $r.hash, '--wait', '25')
    if ($g -match 'code: (\d+)') { $r.code = [int]$Matches[1] }
    if ($g -match 'height: (\d+)') { $r.height = [int]$Matches[1]; $script:height = $r.height }
    if ($g -match 'raw_log: ([^\r\n]*)') { $r.log = $Matches[1].Trim() }
    $r.ok = ($r.code -eq 0)
    return $r
}

function Invoke-Exec([string]$Seed, [string]$Contract, $Msg, [string]$Tag, [string]$Amount = '') {
    $f = Join-Path $MsgDir "$Tag.json"
    [IO.File]::WriteAllText($f, ($Msg | ConvertTo-Json -Depth 12 -Compress))
    $a = @('execute', '--contract', $Contract, '--msg-file', $f)
    if ($Amount) { $a += @('--amount', $Amount, '--denom', $Denom) }
    return Wait-Commit (Invoke-Tx $Seed $a)
}

function Invoke-Send([string]$Seed, [string]$To, [string]$Amount) {
    return Wait-Commit (Invoke-Tx $Seed @('send', '--to', $To, '--amount', $Amount, '--denom', $Denom))
}

function Invoke-Query([string]$Contract, $Msg) {
    $f = Join-Path $MsgDir 'query.json'
    [IO.File]::WriteAllText($f, ($Msg | ConvertTo-Json -Depth 12 -Compress))
    $o = Invoke-Tx '' @('query', '--contract', $Contract, '--msg-file', $f)
    if ($o -match 'SmartContractState result: ([^\r\n]+)') {
        try { return ($Matches[1].Trim() | ConvertFrom-Json) } catch { return $Matches[1].Trim() }
    }
    return $null
}

function Get-Bal([string]$Addr) {
    $o = Invoke-Tx '' @('balance', '--address', $Addr)
    $m = [regex]::Match($o, '(\d+)\s*ujclaw')
    if ($m.Success) { return [int64]$m.Groups[1].Value }
    $m = [regex]::Match($o, 'ujclaw\D{0,12}(\d+)')
    if ($m.Success) { return [int64]$m.Groups[1].Value }
    return -1
}

function Get-Addr([string]$Seed) {
    $o = Invoke-Tx $Seed @('balance')
    if ($o -match '(juno1[a-z0-9]{38,})') { return $Matches[1] }
    throw "cannot derive address for seed $Seed : $o"
}

function Check([string]$Name, $r, [bool]$ShouldPass = $true, [string]$LogHas = '') {
    $good = ($r.ok -eq $ShouldPass)
    if ($good -and (-not $ShouldPass) -and $LogHas) { $good = ($r.log -like "*$LogHas*") }
    $tag = if ($good) { 'PASS' } else { 'FAIL' }
    $l = [string]$r.log
    if ($l.Length -gt 150) { $l = $l.Substring(0, 150) }
    Log ("[{0}] {1} (code={2} h={3}) {4}" -f $tag, $Name, $r.code, $r.height, $l)
    if ($good) { $script:pass++ } else { $script:fail++ }
    return $good
}

function Assert-Eq([string]$Name, $Actual, $Expected) {
    $good = ("$Actual" -eq "$Expected")
    $tag = if ($good) { 'PASS' } else { 'FAIL' }
    Log ("[{0}] {1}: got={2} want={3}" -f $tag, $Name, $Actual, $Expected)
    if ($good) { $script:pass++ } else { $script:fail++ }
}

function Newest-Id($arr) {
    $a = @($arr)
    if ($a.Count -eq 0) { return 0 }
    return [int64]$a[0].id
}

function To-Int($v) { return [int64](([string]$v).Trim('"')) }

function Refresh-Height {
    if (-not $S['addr.d']) { $S['addr.d'] = Get-Addr '' }
    $r = Invoke-Send '' $S['addr.d'] '1'
    if (-not $r.ok) { Log ("  warn: height refresh failed: " + $r.log) }
}

function Next-TaskId {
    $st = Invoke-Query $TL @{ get_stats = @{} }
    return (To-Int $st.total_tasks) + 1
}

function Complete-Task([int64]$TaskId, [string]$OutHash, [int]$Retries = 20) {
    for ($i = 0; $i -lt $Retries; $i++) {
        $r = Invoke-Exec '' $TL @{ complete_task = @{ task_id = $TaskId; output_hash = $OutHash; cost_ujuno = $null } } "complete-$TaskId-$i"
        if ($r.ok -or (($r.log -notlike '*pre_hook*') -and $r.log)) { return $r }
        Start-Sleep -Seconds 2
    }
    return $r
}

function Submit-Task([int64]$AgentId, [string]$InputHash, $Pre, $Post, [string]$Tag) {
    $r = Invoke-Exec $Seeds.owner $TL @{ submit_task = @{
            agent_id = $AgentId; input_hash = $InputHash; execution_tier = 'local'
            pre_hooks = @($Pre); post_hooks = @($Post) } } $Tag
    return $r
}

function Phase-Setup {
    Log '== setup: actors, funding, wiring =='
    foreach ($k in $Seeds.Keys) { $S["addr.$k"] = Get-Addr $Seeds[$k]; Log ("  {0,-6} {1}" -f $k, $S["addr.$k"]) }
    $S['addr.d'] = Get-Addr ''
    Log ("  deployer balance raw: " + ((Invoke-Tx '' @('balance')) -replace '\s+', ' ').Trim())
    foreach ($k in $Seeds.Keys) {
        $b = Get-Bal $S["addr.$k"]
        if ($b -ge 10000000) { Log "  $k already funded ($b)"; continue }
        Check "fund $k" (Invoke-Send '' $S["addr.$k"] '30000000') | Out-Null
    }
    foreach ($k in $Seeds.Keys) { Log ("  bal {0,-6} {1}" -f $k, (Get-Bal $S["addr.$k"])) }
    Check 'task-ledger.agent_company = agent-company' (Invoke-Exec '' $TL @{ update_config = @{ admin = $null; agent_registry = $null; agent_company = $AC } } 'cfg-tl-ac') | Out-Null
    Check 'truth-market.min_operators = 3' (Invoke-Exec '' $TM @{ update_config = @{ min_stake = $null; slash_percent = $null; reward_percent = $null; unstake_cooldown_secs = $null; min_operators = 3; reward_mode = $null; verification_fee = $null } } 'cfg-tm-minops') | Out-Null
    Save-S
}

function Phase-Agent {
    Log '== agent: onboarding, skill, listing =='
    $own = $S['addr.owner']
    Check 'register agent without fee must fail' (Invoke-Exec $Seeds.req $AR @{ register_agent = @{ name = 'nofee'; description = 'x'; capabilities_hash = 'sha256:x'; model = 'x' } } 'reg-nofee') $false 'fee' | Out-Null
    $r = Invoke-Exec $Seeds.owner $AR @{ register_agent = @{ name = 'e2e-verifier-bot'; description = 'devnet e2e agent'; capabilities_hash = 'sha256:e2e-capabilities'; model = 'open-weight-8b' } } 'reg-agent' '1000000'
    Check 'register agent (fee 1000000)' $r | Out-Null
    $st = Invoke-Query $AR @{ get_stats = @{} }
    $found = 0
    for ($i = [int]$st.total_registered + 1; $i -ge 1 -and $found -eq 0; $i--) {
        $p = Invoke-Query $AR @{ get_agent = @{ agent_id = $i } }
        if ($p -and $p.owner -eq $own) { $found = $i }
    }
    $S['agent_id'] = $found
    Assert-Eq 'agent id resolved' ($found -gt 0) $true
    $skill = 'junoclaw-verifier-' + (Get-Date -Format 'yyyyMMddHHmmss')
    $S['skill'] = $skill
    Check 'publish skill' (Invoke-Exec $Seeds.owner $SR @{ publish_skill = @{ dapp_name = $skill; chain_id = 'junoclaw-1'; skill_uri = 'ipfs://e2e-skill'; skill_hash = 'sha256:e2e-skill' } } 'skill' '1000000') | Out-Null
    Check 'list service (price 500000)' (Invoke-Exec $Seeds.owner $MP @{ list_service = @{ skill_ref = $skill; price = '500000'; description = 'e2e verification service' } } 'list') | Out-Null
    $S['listing_id'] = Newest-Id (Invoke-Query $MP @{ list_listings_by_agent = @{ agent = $own; limit = 50 } })
    Assert-Eq 'listing id resolved' ($S['listing_id'] -gt 0) $true
    Log ("  agent_id={0} listing_id={1}" -f $S['agent_id'], $S['listing_id'])
    Save-S
}

function Phase-Custody {
    Log '== custody: task + hire + hook + verdict + release =='
    $own = $S['addr.owner']; $aid = [int64]$S['agent_id']
    Refresh-Height
    $target = $script:height + 14
    $r = Submit-Task $aid 'sha256:e2e-input-A' @{ block_height_at_least = @{ height = $target } } @() 'submit-A'
    Check "submit task A (hook height>=$target)" $r | Out-Null
    $tA = Newest-Id (Invoke-Query $TL @{ get_tasks_by_agent = @{ agent_id = $aid; limit = 50 } })
    $S['taskA'] = $tA
    Check 'complete A before hook height must fail' (Invoke-Exec '' $TL @{ complete_task = @{ task_id = $tA; output_hash = 'sha256:e2e-output-A'; cost_ujuno = $null } } 'complete-A-early') $false 'pre_hook' | Out-Null
    Check 'requester hires listing (escrow 500000)' (Invoke-Exec $Seeds.req $MP @{ hire_service = @{ listing_id = [int64]$S['listing_id']; task_id = $tA } } 'hire-A' '500000') | Out-Null
    $hire = Invoke-Query $MP @{ get_hire_by_task = @{ task_id = $tA } }
    $S['hireA'] = [int64]$hire.id
    Assert-Eq 'hire A status' $hire.status 'escrowed'
    $a0 = Invoke-Query $AR @{ get_agent = @{ agent_id = $aid } }
    $r = Complete-Task $tA 'sha256:e2e-output-A'
    Check 'complete A after hook height' $r | Out-Null
    $S['completeA_hash'] = $r.hash
    $agent = Invoke-Query $AR @{ get_agent = @{ agent_id = $aid } }
    Assert-Eq 'agent total_tasks +1' ([int64]$agent.total_tasks - [int64]$a0.total_tasks) 1
    Assert-Eq 'agent trust_score +1' ([int64]$agent.trust_score - [int64]$a0.trust_score) 1
    Assert-Eq 'task A status' (Invoke-Query $TL @{ get_task = @{ task_id = $tA } }).status 'completed'

    $tmc = Invoke-Query $TM @{ get_config = @{} }
    $stake = To-Int $tmc.min_stake
    foreach ($k in 'v1', 'v2', 'v3') {
        $opr = Invoke-Query $TM @{ get_operator = @{ address = $S["addr.$k"] } }
        if ($opr) { Log "  operator $k already registered (stake $($opr.stake))"; continue }
        Check "register operator $k (stake $stake)" (Invoke-Exec $Seeds[$k] $TM @{ register_operator = @{ fingerprint = "model-$k/host-$k" } } "regop-$k" "$stake") | Out-Null
    }
    Check 'deposit rewards 3000000' (Invoke-Exec '' $TM @{ deposit_rewards = @{} } 'deposit' '3000000') | Out-Null
    $B = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
    $S['batchGreen'] = $B
    Check 'finalize with 0 verdicts must fail' (Invoke-Exec '' $TM @{ finalize_epoch = @{ batch_height = $B; consensus_verdict = 'green'; messages_hash = 'sha256:e2e-output-A' } } 'fin-empty') $false | Out-Null
    Check 'v1 verdict green' (Invoke-Exec $Seeds.v1 $TM @{ submit_verdict = @{ batch_height = $B; verdict = 'green'; messages_hash = 'sha256:e2e-output-A' } } 'v1-green') | Out-Null
    Check 'v2 verdict green' (Invoke-Exec $Seeds.v2 $TM @{ submit_verdict = @{ batch_height = $B; verdict = 'green'; messages_hash = 'sha256:e2e-output-A' } } 'v2-green') | Out-Null
    Check 'v1 duplicate verdict must fail' (Invoke-Exec $Seeds.v1 $TM @{ submit_verdict = @{ batch_height = $B; verdict = 'green'; messages_hash = 'x' } } 'v1-dup') $false | Out-Null
    Check 'finalize with 2 of 3 min_operators must fail' (Invoke-Exec '' $TM @{ finalize_epoch = @{ batch_height = $B; consensus_verdict = 'green'; messages_hash = 'sha256:e2e-output-A' } } 'fin-2') $false | Out-Null
    Check 'v3 verdict red (diverging)' (Invoke-Exec $Seeds.v3 $TM @{ submit_verdict = @{ batch_height = $B; verdict = 'red'; messages_hash = 'sha256:e2e-output-A' } } 'v3-red') | Out-Null
    Check 'non-admin finalize must fail' (Invoke-Exec $Seeds.v1 $TM @{ finalize_epoch = @{ batch_height = $B; consensus_verdict = 'green'; messages_hash = 'sha256:e2e-output-A' } } 'fin-nonadmin') $false | Out-Null
    $poolBefore = To-Int (Invoke-Query $TM @{ get_reward_pool = @{} })
    $v3Before = To-Int (Invoke-Query $TM @{ get_operator = @{ address = $S['addr.v3'] } }).stake
    Check 'admin finalize consensus green' (Invoke-Exec '' $TM @{ finalize_epoch = @{ batch_height = $B; consensus_verdict = 'green'; messages_hash = 'sha256:e2e-output-A' } } 'fin-green') | Out-Null
    $ep = Invoke-Query $TM @{ get_epoch = @{ batch_height = $B } }
    Log ("  epoch: " + ($ep | ConvertTo-Json -Compress))
    $fee = To-Int $tmc.verification_fee
    $expSlash = if ($fee -gt 0) { [math]::Min($fee, $v3Before) } else { [int64][math]::Floor([decimal]$v3Before * $tmc.slash_percent / 100) }
    $expPool = [int64][math]::Floor([decimal]$poolBefore * $tmc.reward_percent / 100)
    $expRewards = [int64]([math]::Floor([decimal]$expPool / 2) * 2)
    Assert-Eq 'epoch matching/diverging' ("{0}/{1}" -f $ep.matching_operators, $ep.diverging_operators) '2/1'
    Assert-Eq 'epoch slashed amount' (To-Int $ep.slashed_amount) $expSlash
    if ($tmc.reward_mode -eq 'equal') { Assert-Eq 'epoch rewards distributed' (To-Int $ep.rewards_distributed) $expRewards } else { Log "  (reward_mode=$($tmc.reward_mode): exact reward assertion skipped)" }
    Assert-Eq 'v3 stake after slash' (To-Int (Invoke-Query $TM @{ get_operator = @{ address = $S['addr.v3'] } }).stake) ($v3Before - $expSlash)

    $before = Get-Bal $own
    Check 'release hire A on green verdict' (Invoke-Exec $Seeds.req $MP @{ release_on_verdict = @{ hire_id = [int64]$S['hireA']; batch_height = $B } } 'release-A') | Out-Null
    Assert-Eq 'hire A status' (Invoke-Query $MP @{ get_hire = @{ hire_id = [int64]$S['hireA'] } }).status 'released'
    Assert-Eq 'owner balance +500000' ((Get-Bal $own) - $before) 500000
    Check 'release hire A twice must fail' (Invoke-Exec $Seeds.req $MP @{ release_on_verdict = @{ hire_id = [int64]$S['hireA']; batch_height = $B } } 'release-A2') $false | Out-Null
    Save-S
}

function Phase-Escrow {
    Log '== escrow: reputation + payment-gated task =='
    $own = $S['addr.owner']; $aid = [int64]$S['agent_id']
    $tB = Next-TaskId
    $S['taskB'] = $tB
    $hooks = @(@{ agent_trust_at_least = @{ agent_id = $aid; min_score = 1 } }, @{ escrow_obligation_confirmed = @{ escrow = $ES; task_id = $tB; payer = $S['addr.req']; payee = $own; min_amount = '250000' } })
    $r = Invoke-Exec $Seeds.owner $TL @{ submit_task = @{ agent_id = $aid; input_hash = 'sha256:e2e-input-B'; execution_tier = 'local'; pre_hooks = $hooks; post_hooks = @() } } 'submit-B'
    Check "submit task B (trust>=1 + pinned escrow hook, id=$tB)" $r | Out-Null
    Assert-Eq 'task B id' (Newest-Id (Invoke-Query $TL @{ get_tasks_by_agent = @{ agent_id = $aid; limit = 50 } })) $tB
    Check 'requester authorizes obligation 250000' (Invoke-Exec $Seeds.req $ES @{ authorize = @{ task_id = $tB; payee = $own; amount = '250000' } } 'auth-B') | Out-Null
    Check 'complete B before payment confirmed must fail' (Invoke-Exec '' $TL @{ complete_task = @{ task_id = $tB; output_hash = 'sha256:e2e-output-B'; cost_ujuno = $null } } 'complete-B-early') $false 'pre_hook' | Out-Null
    $before = Get-Bal $own
    $pay = Invoke-Send $Seeds.req $own '250000'
    Check 'requester pays owner off-contract (bank send)' $pay | Out-Null
    Assert-Eq 'owner balance +250000' ((Get-Bal $own) - $before) 250000
    Check 'attacker confirm must fail (not payer)' (Invoke-Exec $Seeds.atk $ES @{ confirm = @{ task_id = $tB; tx_hash = 'forged' } } 'confirm-atk') $false | Out-Null
    Check 'requester confirms with tx hash' (Invoke-Exec $Seeds.req $ES @{ confirm = @{ task_id = $tB; tx_hash = $pay.hash } } 'confirm-B') | Out-Null
    $a0 = Invoke-Query $AR @{ get_agent = @{ agent_id = $aid } }
    Check 'complete B (hooks satisfied)' (Complete-Task $tB 'sha256:e2e-output-B') | Out-Null
    $a1 = Invoke-Query $AR @{ get_agent = @{ agent_id = $aid } }
    Assert-Eq 'agent trust_score +1' ([int64]$a1.trust_score - [int64]$a0.trust_score) 1
    Assert-Eq 'obligation B status' (Invoke-Query $ES @{ get_obligation_by_task = @{ task_id = $tB } }).status 'confirmed'
    Save-S
}

function Phase-Probe {
    Log '== regression F2: escrow task_id squatting is rejected and cannot spoof the payment hook =='
    $own = $S['addr.owner']; $aid = [int64]$S['agent_id']; $atk = $S['addr.atk']; $req = $S['addr.req']
    $tC = Next-TaskId
    $S['taskC'] = $tC
    $hook = @{ escrow_obligation_confirmed = @{ escrow = $ES; task_id = $tC; payer = $req; payee = $own; min_amount = '250000' } }
    Check "submit task C (pinned escrow hook, id=$tC)" (Submit-Task $aid 'sha256:e2e-input-C' $hook @() 'submit-C') | Out-Null
    Check 'attacker squat of task C obligation must fail' (Invoke-Exec $Seeds.atk $ES @{ authorize = @{ task_id = $tC; payee = $atk; amount = '1' } } 'squat-C') $false 'Unauthorized' | Out-Null
    Check 'attacker authorize for a task that does not exist must fail' (Invoke-Exec $Seeds.atk $ES @{ authorize = @{ task_id = ($tC + 1000); payee = $atk; amount = '1' } } 'squat-future') $false 'carries escrow key' | Out-Null
    Check 'requester authorize with a non-pinned payee must fail' (Invoke-Exec $Seeds.req $ES @{ authorize = @{ task_id = $tC; payee = $atk; amount = '250000' } } 'auth-C-badpayee') $false 'escrow pin' | Out-Null
    Check 'requester authorize below the pinned minimum must fail' (Invoke-Exec $Seeds.req $ES @{ authorize = @{ task_id = $tC; payee = $own; amount = '1' } } 'auth-C-underpay') $false 'escrow pin' | Out-Null
    Check 'requester authorizes the pinned obligation (not blocked by the squat attempts)' (Invoke-Exec $Seeds.req $ES @{ authorize = @{ task_id = $tC; payee = $own; amount = '250000' } } 'auth-C') | Out-Null
    Check 'attacker confirm must fail (not payer)' (Invoke-Exec $Seeds.atk $ES @{ confirm = @{ task_id = $tC; tx_hash = 'self' } } 'confirm-C-atk') $false | Out-Null
    Check 'complete C before payment confirmed must fail' (Invoke-Exec '' $TL @{ complete_task = @{ task_id = $tC; output_hash = 'sha256:e2e-output-C'; cost_ujuno = $null } } 'complete-C-early') $false 'pre_hook' | Out-Null
    $pay = Invoke-Send $Seeds.req $own '250000'
    Check 'requester pays owner off-contract (bank send)' $pay | Out-Null
    Check 'requester confirms with tx hash' (Invoke-Exec $Seeds.req $ES @{ confirm = @{ task_id = $tC; tx_hash = $pay.hash } } 'confirm-C') | Out-Null
    Check 'complete C (hook satisfied by the real obligation)' (Complete-Task $tC 'sha256:e2e-output-C') | Out-Null
    Assert-Eq 'obligation C payee is the owner' (Invoke-Query $ES @{ get_obligation_by_task = @{ task_id = $tC } }).payee $own

    Log '== regression F1: marketplace verdict must be bound to the hire/task =='
    $tA2 = Next-TaskId
    Check 'submit task A2' (Submit-Task $aid 'sha256:e2e-input-A2' @() @() 'submit-A2') | Out-Null
    Check 'requester hires listing for A2 (500000)' (Invoke-Exec $Seeds.req $MP @{ hire_service = @{ listing_id = [int64]$S['listing_id']; task_id = $tA2 } } 'hire-A2' '500000') | Out-Null
    $hire = Invoke-Query $MP @{ get_hire_by_task = @{ task_id = $tA2 } }
    Check 'complete A2' (Complete-Task $tA2 'sha256:e2e-output-A2') | Out-Null
    $redBatch = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds() + 1000
    foreach ($k in 'v1', 'v2', 'v3') {
        Check "$k verdict red on unrelated batch $redBatch" (Invoke-Exec $Seeds[$k] $TM @{ submit_verdict = @{ batch_height = $redBatch; verdict = 'red'; messages_hash = 'sha256:UNRELATED' } } "red-$k") | Out-Null
    }
    Check 'finalize unrelated batch as red' (Invoke-Exec '' $TM @{ finalize_epoch = @{ batch_height = $redBatch; consensus_verdict = 'red'; messages_hash = 'sha256:UNRELATED' } } 'fin-red') | Out-Null
    $before = Get-Bal $S['addr.req']
    Check 'refund of a completed hire via an unrelated red epoch must fail' (Invoke-Exec $Seeds.req $MP @{ release_on_verdict = @{ hire_id = [int64]$hire.id; batch_height = $redBatch } } 'release-A2-red') $false 'did not verify the output' | Out-Null
    Assert-Eq 'hire A2 still escrowed' (Invoke-Query $MP @{ get_hire = @{ hire_id = [int64]$hire.id } }).status 'escrowed'
    Assert-Eq 'requester balance did not increase' ([bool](((Get-Bal $S['addr.req']) - $before) -le 0)) $true
    $greenBatch = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds() + 2000
    foreach ($k in 'v1', 'v2', 'v3') {
        Check "$k verdict green on the batch that verified A2's output" (Invoke-Exec $Seeds[$k] $TM @{ submit_verdict = @{ batch_height = $greenBatch; verdict = 'green'; messages_hash = 'sha256:e2e-output-A2' } } "green2-$k") | Out-Null
    }
    Check 'finalize the A2 batch as green' (Invoke-Exec '' $TM @{ finalize_epoch = @{ batch_height = $greenBatch; consensus_verdict = 'green'; messages_hash = 'sha256:e2e-output-A2' } } 'fin-green2') | Out-Null
    $before = Get-Bal $S['addr.owner']
    Check 'release hire A2 on the epoch that verified its output' (Invoke-Exec $Seeds.req $MP @{ release_on_verdict = @{ hire_id = [int64]$hire.id; batch_height = $greenBatch } } 'release-A2') | Out-Null
    Assert-Eq 'hire A2 status' (Invoke-Query $MP @{ get_hire = @{ hire_id = [int64]$hire.id } }).status 'released'
    Assert-Eq 'owner balance +500000' ((Get-Bal $S['addr.owner']) - $before) 500000
    Save-S
}

function Get-Commit([string]$Text) {
    return [Convert]::ToBase64String([Security.Cryptography.SHA256]::Create().ComputeHash([Text.Encoding]::UTF8.GetBytes($Text)))
}

function Get-EntryIds([string]$Author) {
    $q = Invoke-Query $MB @{ list_by_author = @{ author = $Author; limit = 100 } }
    if ($q -and $q.entries) { foreach ($x in @($q.entries)) { [string]$x.id } }
}

function Phase-Prov {
    Log '== provenance: moultbook cite-only-real-entries + credit score =='
    $own = $S['addr.owner']
    $att = @{ bridge = @{ source_chain = 'junoclaw-1'; tx_hash = 'e2e-complete-A' } }
    $ct = 'application/vnd.junoclaw.task-output'
    Check 'post citing unknown ref must fail' (Invoke-Exec $Seeds.owner $MB @{ post = @{ commitment = (Get-Commit 'e2e-output-A'); content_type = $ct; size_bytes = 128; attestation_ref = $att; visibility = 'public'; refs = @("task:$($S['taskA'])") } } 'mb-badref') $false 'Invalid ref id' | Out-Null
    $before = @(Get-EntryIds $own)
    Check 'post task output (attested)' (Invoke-Exec $Seeds.owner $MB @{ post = @{ commitment = (Get-Commit 'e2e-output-A'); content_type = $ct; size_bytes = 128; attestation_ref = $att; visibility = 'public'; refs = @() } } 'mb-post1') | Out-Null
    $id1 = @(Get-EntryIds $own) | Where-Object { $before -notcontains $_ } | Select-Object -First 1
    Assert-Eq 'task output entry id resolved' ([bool]$id1) $true
    if (-not $id1) { return }
    $S['moult_entry1'] = $id1
    $before = @(Get-EntryIds $own)
    Check 'post receipt citing real entry' (Invoke-Exec $Seeds.owner $MB @{ post = @{ commitment = (Get-Commit 'e2e-receipt-A'); content_type = 'application/vnd.junoclaw.receipt'; size_bytes = 64; attestation_ref = $att; visibility = 'public'; refs = @($id1) } } 'mb-post2') | Out-Null
    $id2 = @(Get-EntryIds $own) | Where-Object { $before -notcontains $_ } | Select-Object -First 1
    Assert-Eq 'receipt entry id resolved' ([bool]$id2) $true
    $cited = Invoke-Query $MB @{ list_by_ref = @{ ref_id = $id1; limit = 10 } }
    $citedIds = @()
    if ($cited -and $cited.entries) { $citedIds = @($cited.entries | ForEach-Object { [string]$_.id }) }
    Assert-Eq 'list_by_ref returns the receipt' ([bool]($id2 -and ($citedIds -contains $id2))) $true
    $score = Invoke-Query $MB @{ query_credit_score = @{ author = $own } }
    Log ("  credit: " + ($score | ConvertTo-Json -Compress))
    Assert-Eq 'credit score (all entries attested)' $score.score 100
    $cfgAc = Invoke-Query $AC @{ get_config = @{} }
    Log ("  agent-company config: " + ($cfgAc | ConvertTo-Json -Depth 4 -Compress))
    Save-S
}

function Phase-Summary {
    Log '== summary: contract stats =='
    foreach ($p in @(@('agent-registry', $AR), @('task-ledger', $TL), @('escrow', $ES), @('truth-market', $TM), @('marketplace', $MP), @('moultbook', $MB), @('skill-registry', $SR))) {
        Log ("  {0,-15} {1}" -f $p[0], ((Invoke-Query $p[1] @{ get_stats = @{} }) | ConvertTo-Json -Compress))
    }
    Log ("  reward pool: " + (Invoke-Query $TM @{ get_reward_pool = @{} }))
}

$phases = if ($Phase -eq 'all') { @('setup', 'agent', 'custody', 'escrow', 'probe', 'prov', 'summary') } else { @($Phase) }
foreach ($p in $phases) {
    switch ($p) {
        'setup' { Phase-Setup } 'agent' { Phase-Agent } 'custody' { Phase-Custody }
        'escrow' { Phase-Escrow } 'probe' { Phase-Probe } 'prov' { Phase-Prov } 'summary' { Phase-Summary }
    }
}
Log ("== done: pass={0} fail={1} (log: {2}) ==" -f $script:pass, $script:fail, $LogFile)
