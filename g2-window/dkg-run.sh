#!/usr/bin/env bash
# dkg-run.sh — Phase 3 ceremony orchestrator (coordinator side).
#
# Drives one reshare round over the r3-style layout, keeps a hash-chained
# transcript of every artifact that lands, and emits the exact commands
# humans still need to run. Human surface after this: dealers run ONE
# command (dkg-deal), players run ONE command (dkg-ack / dkg-finalize),
# the coordinator approves phase transitions.
#
# Layout (created by `init`, all relative to RUN dir):
#   share-requests/<player>.json              — every player
#   dealing-pubs/dealing-pub-<dealer>.json    — every dealer
#   privs/dealing-priv-<dealer>-for-<player>.json  (sealed; per dealer×player)
#   acks/ack-<player>-to-<dealer>.json        — per player×dealer
#   logs/log-<dealer>.json                    — per dealer
#   ceremony/dkg-info.json  ceremony/shared.json
#   bundles/bundle-<player>.tar.gz + MANIFEST-bundles.txt (via dkg-pack.sh)
#   witness/<player>.txt                      — posted observe prefixes
#   run.env          — CHAIN_ID, ROUND, GTK
#   transcript.jsonl — hash-chained artifact log (anchorable evidence)
#
# Subcommands:
#   init --chain-id <id> --round <n> [--gtk <path>]
#   status / collect      — expected-vs-present matrix + transcript tip
#   record                — hash every new artifact into the transcript
#   observe               — run dkg-observe, store expected group prefix
#   witness <player> <prefix> — record a seat's posted prefix, flag mismatch
#   pack [--publish <relay> --put-token T] — bundles (+ optional jc-share)
#   finalize              — emit exact per-seat finalize commands
set -euo pipefail
cd "$(dirname "$0")"
RUN=${RUN:-.}
die() { echo "dkg-run: $*" >&2; exit 1; }

env_load() { [ -f "$RUN/run.env" ] && . "$RUN/run.env" || true; }
env_load
GTK=${GTK:-/root/junoclaw-chain/tools/generate-testnet-keys/target/release/generate-testnet-keys}
JCSHARE=${JCSHARE:-jc-share}
CHAIN_ID=${CHAIN_ID:-junoclaw-g1}
ROUND=${ROUND:-0}

TRANSCRIPT="$RUN/transcript.jsonl"

# python3 on some dev boxes resolves to the Windows Store stub (silent
# nonzero exit) — probe before trusting it. On Linux hosts python3 wins.
PY_BIN=""
py() {
  if [ -z "$PY_BIN" ]; then
    if python3 -c 'pass' 2>/dev/null; then PY_BIN=python3;
    elif python -c 'pass' 2>/dev/null; then PY_BIN=python;
    else die "no working python (python3/python)"; fi
  fi
  "$PY_BIN" "$@" | tr -d '\r'
}

# Paths handed to python must be Windows-form on dev boxes (cygpath exists
# in git-bash, absent on Linux — falls back to the POSIX path).
wpath() { cygpath -w "$1" 2>/dev/null || echo "$1"; }

sha() { sha256sum "$1" | awk '{print $1}'; }

players() { ls "$RUN"/share-requests/*.json 2>/dev/null | while read -r f; do basename "$f" .json; done; }
# Artifact files are named by ed25519 key-prefix, not moniker — derive
# each player's prefix from its share-request.
player_prefixes() {
  py - "$(wpath "$RUN/share-requests")" <<'PY'
import glob, json, os, sys
for f in sorted(glob.glob(os.path.join(sys.argv[1], "*.json"))):
    print(json.load(open(f))["ed25519_public_hex"][:8])
PY
}
dealers() { ls "$RUN"/dealing-pubs/dealing-pub-*.json 2>/dev/null | while read -r f; do basename "$f" .json | sed 's/^dealing-pub-//'; done; }

tip() { [ -f "$TRANSCRIPT" ] && tail -1 "$TRANSCRIPT" | py -c 'import json,sys;print(json.load(sys.stdin)["entry"])' || echo "none"; }

# hash-chained append: entry = sha256(prev_entry + canonical payload)
record_entry() { # $1=op $2=artifact $3=sha256|- $4=note
  local op=$1 art=$2 s=$3 note=$4
  mkdir -p "$(dirname "$TRANSCRIPT")"
  local prev="genesis" seq=0
  if [ -f "$TRANSCRIPT" ]; then
    prev=$(tail -1 "$TRANSCRIPT" | py -c 'import json,sys;print(json.load(sys.stdin)["entry"])')
    seq=$(tail -1 "$TRANSCRIPT" | py -c 'import json,sys;print(json.load(sys.stdin)["seq"]+1)')
  fi
  py - "$(wpath "$TRANSCRIPT")" "$op" "$art" "$s" "$note" "$prev" "$seq" <<'PY'
import hashlib, json, sys, time
path, op, art, s, note, prev, seq = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4], sys.argv[5], sys.argv[6], int(sys.argv[7])
payload = {"seq": seq, "ts": int(time.time()), "op": op, "artifact": art,
           "sha256": s, "note": note, "prev": prev}
entry = hashlib.sha256((prev + "|" + json.dumps(payload, sort_keys=True)).encode()).hexdigest()
payload["entry"] = entry
with open(path, "a") as f:
    f.write(json.dumps(payload) + "\n")
PY
}

record_new() { # scan known artifact dirs, record any not yet in transcript
  local dir pat
  for dir in share-requests dealing-pubs privs acks logs witness; do
    [ -d "$RUN/$dir" ] || continue
    find "$RUN/$dir" -type f -name '*.json' -o -type f -name '*.txt' | while read -r f; do
      grep -qF "\"artifact\": \"${f#$RUN/}\"" "$TRANSCRIPT" 2>/dev/null && continue
      record_entry "artifact" "${f#$RUN/}" "$(sha "$f")" ""
    done
  done
}

cmd=${1:-}; shift || true
case "$cmd" in
init)
  CHAIN_ID=""; ROUND=""; GTK_ARG=""
  while [ $# -gt 0 ]; do case $1 in
    --chain-id) CHAIN_ID=$2; shift 2;; --round) ROUND=$2; shift 2;;
    --gtk) GTK_ARG=$2; shift 2;; *) shift;;
  esac; done
  [ -n "$CHAIN_ID" ] && [ -n "$ROUND" ] || die "init needs --chain-id and --round"
  mkdir -p "$RUN"/{share-requests,dealing-pubs,privs,acks,logs,ceremony,bundles,witness}
  {
    echo "CHAIN_ID=$CHAIN_ID"; echo "ROUND=$ROUND"
    [ -n "$GTK_ARG" ] && echo "GTK=$GTK_ARG"
  } > "$RUN/run.env"
  env_load
  record_entry "init" "run.env" "$(sha "$RUN/run.env")" "chain=$CHAIN_ID round=$ROUND"
  echo "initialized round $ROUND ($CHAIN_ID) — transcript tip: $(tip)"
  ;;
status|collect)
  env_load
  P=$(players | sort); D=$(dealers | sort); PP=$(player_prefixes | sort)
  np=$(echo "$P" | grep -c .); nd=$(echo "$D" | grep -c .)
  echo "== round $ROUND ($CHAIN_ID): $nd dealers -> $np players"
  echo "-- players: $(echo $P)"
  echo "-- dealers: $(echo $D)"
  miss=0
  for p in $PP; do
    for d in $D; do
      [ -f "$RUN/privs/dealing-priv-$d-for-$p.json" ] || { echo "MISS priv $d->$p"; miss=$((miss+1)); }
      [ -f "$RUN/acks/ack-$p-to-$d.json" ]        || { echo "MISS ack $p->$d";  miss=$((miss+1)); }
    done
  done
  for d in $D; do
    [ -f "$RUN/logs/log-$d.json" ] || { echo "MISS log $d"; miss=$((miss+1)); }
  done
  exp=$(( np*nd*2 + nd ))
  have=$(( exp - miss ))
  echo "== artifacts: $have/$exp present, $miss missing"
  echo "== transcript tip: $(tip)"
  ;;
record)
  env_load; record_new; echo "tip: $(tip)";;
observe)
  env_load
  [ -f "$RUN/ceremony/dkg-info.json" ] || die "no ceremony/dkg-info.json"
  out=$("$GTK" dkg-observe --info "$RUN/ceremony/dkg-info.json" \
        --logs-dir "$RUN/logs" --requests "$RUN/share-requests" 2>&1 | tee /dev/stderr)
  prefix=$(echo "$out" | grep -oE '[0-9a-f]{8,}' | head -1)
  [ -n "$prefix" ] || die "could not parse observed prefix"
  echo "$prefix" > "$RUN/ceremony/observed-prefix.txt"
  record_entry "observe" "ceremony/observed-prefix.txt" "$(sha "$RUN/ceremony/observed-prefix.txt")" "prefix=$prefix"
  ;;
witness)
  env_load; seat=${1:?player}; pref=${2:?prefix}
  exp=$(cat "$RUN/ceremony/observed-prefix.txt" 2>/dev/null || true)
  echo "$pref" > "$RUN/witness/$seat.txt"
  if [ -n "$exp" ] && [ "$pref" != "$exp" ]; then
    record_entry "witness-mismatch" "witness/$seat.txt" "$(sha "$RUN/witness/$seat.txt")" "got=$pref expected=$exp"
    die "MISMATCH: $seat posted $pref, expected $exp — do NOT proceed"
  fi
  record_entry "witness" "witness/$seat.txt" "$(sha "$RUN/witness/$seat.txt")" "prefix=$pref"
  echo "witness $seat: $pref OK"
  ;;
pack)
  env_load
  relay=""; ptok=""
  while [ $# -gt 0 ]; do case $1 in
    --publish) relay=$2; shift 2;; --put-token) ptok=$2; shift 2;; *) shift;;
  esac; done
  INFO="$RUN/ceremony/dkg-info.json" SHARED="$RUN/ceremony/shared.json" \
    LOGS_DIR="$RUN/logs" REQUESTS_DIR="$RUN/share-requests" \
    PRIV_DIR="$RUN/privs" OUT="$RUN/bundles" \
    bash ./dkg-pack.sh
  record_new
  record_entry "pack" "bundles/MANIFEST-bundles.txt" "$(sha "$RUN/bundles/MANIFEST-bundles.txt")" ""
  if [ -n "$relay" ]; then
    : > "$RUN/SEAT-LINKS.txt"
    for b in "$RUN"/bundles/bundle-*.tar.gz; do
      seat=$(basename "$b" .tar.gz | sed 's/^bundle-//')
      link=$("$JCSHARE" publish --relay "$relay" --file "$b" ${ptok:+--put-token "$ptok"} | awk '/^link/{print $2}')
      echo "$seat $link" | tee -a "$RUN/SEAT-LINKS.txt"
      record_entry "publish" "bundles/$(basename "$b")" "$(sha "$b")" "seat=$seat"
    done
    echo "links in $RUN/SEAT-LINKS.txt — post with MANIFEST-bundles.txt hashes"
  fi
  ;;
finalize)
  env_load
  cat <<EOF
== per-seat finalize (each seat, in their kit dir):
generate-testnet-keys dkg-finalize \\
  --info ceremony/dkg-info.json --keys keys/keys.json \\
  --priv-dir privs --logs-dir logs --requests share-requests \\
  --out bls-share.json
generate-testnet-keys finalize --keys keys/keys.json --bls-share bls-share.json
== then the apply window: node.toml swap + ONE restart per seat.
== transcript tip now: $(tip) — anchor it.
EOF
  ;;
*) die "unknown subcommand: $cmd (init|status|collect|record|observe|witness|pack|finalize)";;
esac
