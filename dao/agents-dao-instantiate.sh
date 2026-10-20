#!/usr/bin/env bash
# agents-dao-instantiate.sh — instantiate the JunoClaw Agents DAO (tokenless cw4).
#
# Usage: ./agents-dao-instantiate.sh <addr1> <addr2> [<addr3> ...]
#   Each arg is a member juno1... address. All members get weight 1.
#   Requires: bls-relayer (built), python3, network access to gRPC.
#
# Code IDs on junoclaw-g1 (verified by on-chain outcome 2026-10-20 —
# the earlier comment block was off by one for core/proposal):
#   11  cw4_group          (cw-plus v2.0.0,   4604a284…)
#   12  dao_voting_cw4     (dao-contracts v2.7.1, 52afbd7f…)
#    9  dao_dao_core       (dao-contracts v2.7.1, 976ccfb8…)
#    8  dao_proposal_single(dao-contracts v2.7.1, 149599fb…)
#    5  cw_admin_factory   (dao-contracts v2.7.1, cc6914b5…)
#    7  cw20_stake         (NOT proposal_single — an instantiate pointed
#                        at it fails DeliverTx with a cw20_stake parse error)
#
# Flow: ONE tx — dao_dao_core instantiates voting (spawns cw4_group with
# admin = core) and the proposal module in parallel. After it lands,
# membership changes go through DAO proposals — never by hand.

set -euo pipefail

RELAYER=${RELAYER:-./target/release/bls-relayer}
GRPC=${GRPC:-51.195.201.74:9090}
CODE_CW4_GROUP=11
CODE_VOTING_CW4=12
CODE_CORE=9
CODE_PROPOSAL_SINGLE=8

# Governance knobs (ops DAO: 5 seats)
VOTING_PERIOD_SECS=${VOTING_PERIOD_SECS:-604800}   # 7d
QUORUM=${QUORUM:-0.60}                             # 60% turnout floor
# threshold = majority of votes cast — hardcoded below.

if [ $# -lt 2 ]; then
  echo "usage: $0 <juno1-addr> <juno1-addr> [...]" >&2
  exit 1
fi

# --- build msgs -------------------------------------------------------------
PY=$(command -v python || command -v python3)  # python first: python3 may be the Windows Store stub
MEMBERS_JSON=$("$PY" - "$@" <<'PY'
import json, sys
print(json.dumps([{"addr": a, "weight": 1} for a in sys.argv[1:]]))
PY
)

VOTING_B64=$("$PY" - "$MEMBERS_JSON" <<'PY'
import base64, json, sys
msg = {"group_contract": {"new": {
    "cw4_group_code_id": 11,
    "cw4_group_salt": None,
    "initial_members": json.loads(sys.argv[1]),
}}}
print(base64.b64encode(json.dumps(msg).encode()).decode())
PY
)

PROPOSAL_B64=$("$PY" - "$VOTING_PERIOD_SECS" "$QUORUM" <<'PY'
import base64, json, sys
msg = {
  "threshold": {"threshold_quorum": {
    "threshold": {"majority": {}},
    "quorum": {"percent": sys.argv[2]}}},
  "max_voting_period": {"time": int(sys.argv[1])},
  "min_voting_period": None,
  "only_members_execute": True,
  "allow_revoting": False,
  "pre_propose_info": {"anyone_may_propose": {}},
  "close_proposal_on_execution_failure": True,
}
print(base64.b64encode(json.dumps(msg).encode()).decode())
PY
)

CORE_MSG=$("$PY" - "$VOTING_B64" "$PROPOSAL_B64" "$CODE_VOTING_CW4" "$CODE_PROPOSAL_SINGLE" <<'PY'
import json, sys
print(json.dumps({
  "name": "JunoClaw Agents DAO",
  "description": "Tokenless operations DAO — cw4 membership, one seat "
                 "per validator. Membership changes via governance; "
                 "relay allowlist mirrors cw4 state.",
  "automatically_add_cw20s": True,
  "automatically_add_cw721s": True,
  "voting_module_instantiate_info": {
    "admin": {"core_module": {}},
    "code_id": int(sys.argv[3]),
    "label": "agents-voting-cw4",
    "msg": sys.argv[1]},
  "proposal_modules_instantiate_info": [{
    "admin": {"core_module": {}},
    "code_id": int(sys.argv[4]),
    "label": "agents-proposal-single",
    "msg": sys.argv[2]}],
}, indent=1))
PY
)

MSG_FILE="$(cd "$(dirname "$0")" && pwd)/.agents-dao-instantiate.generated.json"
echo "=== Agents DAO instantiate ==="
echo "members: $*"
echo "quorum=$QUORUM voting_period=${VOTING_PERIOD_SECS}s threshold=majority"
echo "$CORE_MSG" > "$MSG_FILE"
echo "msg written to $MSG_FILE"
echo
read -rp "Broadcast? [y/N] " yn
[ "$yn" = y ] || { echo "aborted (msg saved)"; exit 0; }

$RELAYER jc-instantiate --layer-grpc "$GRPC" --code-id "$CODE_CORE" \
  --label "JunoClaw Agents DAO" --msg "@$MSG_FILE"

echo
echo "verify:  $RELAYER jc-contracts --layer-grpc $GRPC --code-id $CODE_CORE"
echo "expect 4 contracts: core + voting + proposal + cw4_group"
