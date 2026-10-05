param(
    [string]$Grpc = '127.0.0.1:9090',
    [string]$WasmDir = '..\junoclaw\contracts\target\wasm32-unknown-unknown\release',
    [string]$Out = 'snapshot\agent-stack-deployed.json',
    [string]$Denom = 'ujclaw'
)

$ErrorActionPreference = 'Continue'
$TX = '.\target\release\tx-sender.exe'
$D = 'juno1dz875zg8p78anpjv3f0qt4gu5a3awpjfhtw992'
$MsgDir = Join-Path $env:TEMP 'agent-stack-msgs'
New-Item -ItemType Directory -Force -Path $MsgDir | Out-Null

$state = @{}
if (Test-Path $Out) {
    (Get-Content $Out -Raw | ConvertFrom-Json).PSObject.Properties | ForEach-Object { $state[$_.Name] = $_.Value }
}

function Save-State { $state | ConvertTo-Json -Depth 6 | Set-Content -Path $Out -Encoding UTF8 }

function Run-Tx {
    param([string[]]$TxArgs)
    $o = & $TX @TxArgs --grpc $Grpc 2>&1 | Out-String
    return $o
}

function Max-CodeId {
    $max = 0
    for ($i = 1; $i -le 200; $i++) {
        $o = Run-Tx @('code-info', '--code-id', "$i")
        if ($o -match 'code_id=') { $max = $i } else { break }
    }
    return $max
}

function Contracts-Of {
    param([int]$CodeId)
    $o = Run-Tx @('contracts-by-code', '--code-id', "$CodeId")
    return @([regex]::Matches($o, 'juno1[a-z0-9]{38,70}') | ForEach-Object { $_.Value } | Select-Object -Unique)
}

function Wait-Until {
    param([scriptblock]$Cond, [int]$TimeoutSec = 30)
    $sw = [Diagnostics.Stopwatch]::StartNew()
    while ($sw.Elapsed.TotalSeconds -lt $TimeoutSec) {
        if (& $Cond) { return $true }
        Start-Sleep -Milliseconds 700
    }
    return $false
}

function Store {
    param([string]$Name)
    $key = "$Name.code_id"
    if ($state.ContainsKey($key)) { Write-Host "[skip] $Name already stored: code $($state[$key])"; return [int]$state[$key] }
    $file = Join-Path $WasmDir "$Name.wasm"
    $before = Max-CodeId
    Write-Host "[store] $Name ($([Math]::Round((Get-Item $file).Length/1KB)) KB), current max code $before"
    $o = Run-Tx @('store-code', '--wasm', $file)
    Write-Host ($o.Trim() -replace '\s+', ' ')
    $ok = Wait-Until { (Max-CodeId) -gt $before } 40
    if (-not $ok) { throw "store-code for $Name did not produce a new code id" }
    $id = Max-CodeId
    $state[$key] = $id
    Save-State
    Write-Host "  -> code $id"
    return $id
}

function Instantiate {
    param([string]$Name, [int]$CodeId, $Msg)
    $key = "$Name.address"
    if ($state.ContainsKey($key)) { Write-Host "[skip] $Name already instantiated: $($state[$key])"; return $state[$key] }
    $f = Join-Path $MsgDir "$Name.init.json"
    ($Msg | ConvertTo-Json -Depth 10 -Compress) | Set-Content -Path $f -Encoding ASCII
    $before = Contracts-Of $CodeId
    Write-Host "[init] $Name code $CodeId"
    $o = Run-Tx @('instantiate', '--code-id', "$CodeId", '--label', "junoclaw-$Name", '--msg-file', $f)
    Write-Host ($o.Trim() -replace '\s+', ' ')
    $ok = Wait-Until { (Contracts-Of $CodeId).Count -gt $before.Count } 40
    if (-not $ok) { throw "instantiate for $Name produced no new contract (check msg: $f)" }
    $addr = (Contracts-Of $CodeId | Where-Object { $before -notcontains $_ } | Select-Object -First 1)
    $state[$key] = $addr
    Save-State
    Write-Host "  -> $addr"
    return $addr
}

function Execute {
    param([string]$Contract, $Msg, [string]$Tag, [string]$Amount = '')
    $f = Join-Path $MsgDir "$Tag.exec.json"
    ($Msg | ConvertTo-Json -Depth 10 -Compress) | Set-Content -Path $f -Encoding ASCII
    $a = @('execute', '--contract', $Contract, '--msg-file', $f)
    if ($Amount) { $a += @('--amount', $Amount, '--denom', $Denom) }
    Write-Host "[exec] $Tag"
    $o = Run-Tx $a
    Write-Host ($o.Trim() -replace '\s+', ' ')
    Start-Sleep -Seconds 3
}

# ---- 1. identity + work core -------------------------------------------------
$cAR = Store 'agent_registry'
$AR = Instantiate 'agent_registry' $cAR @{
    admin = $D; max_agents = 1000; registration_fee_ujuno = '1000000'; denom = $Denom; registry = $null
}

$cTL = Store 'task_ledger'
$TL = Instantiate 'task_ledger' $cTL @{
    admin = $D; agent_registry = $AR; operators = @($D); agent_company = $null
    registry = @{ agent_registry = $AR; task_ledger = $null; escrow = $null }
}

$cES = Store 'escrow'
$ES = Instantiate 'escrow' $cES @{
    admin = $D; task_ledger = $TL; timeout_blocks = 1000; denom = $Denom; registry = $null
}

if (-not $state.ContainsKey('wired.registry')) {
    Execute $AR @{ update_registry = @{ agent_registry = $AR; task_ledger = $TL; escrow = $ES } } 'registry-wire'
    $state['wired.registry'] = $true
    Save-State
}

# ---- 2. provenance + governance ---------------------------------------------
$cMB = Store 'moultbook_v0'
$MB = Instantiate 'moultbook_v0' $cMB @{
    admin = $D; whoami_contract = $null; max_size_bytes = 65536; max_refs = 16
    max_content_type_len = 64; max_group_size = 32; zk_verifier = $null
    agent_registry = $AR; membership_vk_hash = $null
    entries_per_key_per_epoch = 10; epoch_blocks = 14400
}

$cAC = Store 'agent_company'
$AC = Instantiate 'agent_company' $cAC @{
    name = 'JunoClaw Agent Company'; admin = $D; governance = $null
    wavs_operator = $null; zk_verifier = $null; jolt_verifier = $null
    moultbook = $MB; relayer = $D; sealed_signer = $null
    escrow_contract = $ES; agent_registry = $AR; task_ledger = $TL; nois_proxy = $null
    members = @(@{ addr = $D; weight = 10000; role = 'human' })
    denom = $Denom
}

# ---- 3. verification + discovery --------------------------------------------
$cTM = Store 'truth_market'
$TM = Instantiate 'truth_market' $cTM @{
    min_stake = '1000000'; slash_percent = 10; reward_percent = 5; denom = $Denom
    unstake_cooldown_secs = 86400; min_operators = 1; verification_fee = '0'
}

$cSR = Store 'skill_registry'
$SR = Instantiate 'skill_registry' $cSR @{
    admin = $D; denom = $Denom; registration_fee = '1000000'
}

$cMP = Store 'marketplace'
$MP = Instantiate 'marketplace' $cMP @{
    admin = $D; truth_market = $TM; task_ledger = $TL; skill_registry = $SR
    denom = $Denom; cancel_window_secs = 3600
}

Write-Host ''
Write-Host '=== agent stack deployed ==='
$state.GetEnumerator() | Sort-Object Name | ForEach-Object { Write-Host ("{0,-28} {1}" -f $_.Name, $_.Value) }
