param(
    [Parameter(Mandatory = $true)][string[]]$Contracts,
    [string]$Grpc = '127.0.0.1:9090',
    [string]$WasmDir = '..\junoclaw\contracts\target\wasm32-unknown-unknown\release',
    [string]$Deployed = 'snapshot\agent-stack-deployed.json'
)

$ErrorActionPreference = 'Continue'
$TX = '.\target\release\tx-sender.exe'

$state = [ordered]@{}
(Get-Content $Deployed -Raw | ConvertFrom-Json).PSObject.Properties | ForEach-Object { $state[$_.Name] = $_.Value }

function Run-Tx([string[]]$TxArgs) { return (& $TX @TxArgs --grpc $Grpc 2>&1 | Out-String) }

function Has-Code([int]$Id, [string]$Hash = '') {
    $o = Run-Tx @('code-info', '--code-id', "$Id")
    if ($Hash) { return ($o -match "data_hash=$Hash") }
    return ($o -match 'code_id=')
}

function Wait-Committed([string]$Out) {
    if ($Out -match 'BroadcastTx response: code=(\d+), log=([^\r\n]*)' -and [int]$Matches[1] -ne 0) {
        return @{ code = [int]$Matches[1]; log = $Matches[2].Trim() }
    }
    if ($Out -notmatch 'txhash: ([0-9A-Fa-f]{64})') { return @{ code = -1; log = ($Out.Trim() -replace '\s+', ' ') } }
    $g = Run-Tx @('get-tx', '--hash', $Matches[1], '--wait', '25')
    $r = @{ code = -1; log = '' }
    if ($g -match 'code: (\d+)') { $r.code = [int]$Matches[1] }
    if ($g -match 'raw_log: ([^\r\n]*)') { $r.log = $Matches[1].Trim() }
    return $r
}

foreach ($name in $Contracts) {
    $addr = $state["$name.address"]
    if (-not $addr) { throw "$Deployed has no $name.address" }
    $file = Join-Path $WasmDir "$name.wasm"
    $hash = (Get-FileHash $file -Algorithm SHA256).Hash.ToLower()

    $id = 1
    while (Has-Code $id) { $id++ }
    Write-Host "[store] $name ($((Get-Item $file).Length) bytes, sha256 $hash) -> code $id"
    $null = Run-Tx @('store-code', '--wasm', $file)
    $sw = [Diagnostics.Stopwatch]::StartNew()
    while (-not (Has-Code $id) -and $sw.Elapsed.TotalSeconds -lt 45) { Start-Sleep -Milliseconds 700 }
    if (-not (Has-Code $id $hash)) { throw "code $id was not stored from $file" }

    Write-Host "[migrate] $name $addr -> code $id"
    $r = Wait-Committed (Run-Tx @('migrate', '--contract', $addr, '--code-id', "$id"))
    if ($r.code -ne 0) { throw "migrate of $name to code $id failed (code $($r.code)): $($r.log)" }

    $state["$name.previous_code_id"] = $state["$name.code_id"]
    $state["$name.code_id"] = $id
    $state | ConvertTo-Json -Depth 6 | Set-Content -Path $Deployed -Encoding UTF8
    Write-Host "  -> $name on code $id (was $($state["$name.previous_code_id"]))"
}
