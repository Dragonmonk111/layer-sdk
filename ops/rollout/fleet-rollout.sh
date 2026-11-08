#!/usr/bin/env bash
# fleet-rollout.sh — per-seat RC18 upgrade for the 2026-11-09 10:00 UTC window.
# Run on the validator host, inside the maintenance window only.
# Idempotent: safe to re-run; already-installed seats skip the swap.
#
#   ./fleet-rollout.sh [--yes]
#
# Env overrides for non-standard setups:
#   BINARY_PATH=/path/to/slay3rd        installed binary location (required)
#   STOP_CMD='systemctl stop junoclaw-g1'        (required)
#   START_CMD='systemctl start junoclaw-g1'      (required)
#   LOG_CMD='journalctl -u junoclaw-g1 -n 400 --no-pager'   readiness log source
#   SMOKE_CONFIG=/path/to/smoke.toml    optional side-port config for smoke-run
#   EXPECT_SHA256=<hex>               override expected checksum (defaults to RC18)
set -euo pipefail

TAG="slay3rd-rc18v2"
EXPECT_SHA256="7e821f922b8812cea970923278899f710a70a9a419ba5cf3c7c790091bf482d2"
RELEASE_URL="https://github.com/Dragonmonk111/layer-sdk/releases/download/${TAG}"
WORK="$(mktemp -d /tmp/slay3rd-rollout.XXXXXX)"
ASSUME_YES=0
[ "${1:-}" = "--yes" ] && ASSUME_YES=1

: "${BINARY_PATH:?set BINARY_PATH to your installed slay3rd location}"
: "${STOP_CMD:?set STOP_CMD for this seat}"
: "${START_CMD:?set START_CMD for this seat}"
LOG_CMD="${LOG_CMD:-journalctl -u junoclaw-g1 -n 400 --no-pager}"

fail() { echo "ROLLOUT-FAIL: $*" >&2; exit 1; }
step() { echo "==> $*"; }
confirm() {
    [ "$ASSUME_YES" = 1 ] && return 0
    read -r -p "$1 [y/N] " r
    [ "$r" = "y" ] || [ "$r" = "Y" ] || fail "aborted by operator"
}

trap 'rm -rf "$WORK"' EXIT

# ---------- 1. download + verify ----------
step "1/7 download $TAG binary + checksum"
curl -fsSL "$RELEASE_URL/slay3rd"          -o "$WORK/slay3rd"         || fail "binary download"
curl -fsSL "$RELEASE_URL/slay3rd.sha256"   -o "$WORK/slay3rd.sha256"  || fail "checksum download"

GOT_SHA256="$(sha256sum "$WORK/slay3rd" | awk '{print $1}')"
[ "$GOT_SHA256" = "$EXPECT_SHA256" ] || fail "sha256 mismatch: got $GOT_SHA256 want $EXPECT_SHA256"
grep -q "$GOT_SHA256" "$WORK/slay3rd.sha256" || \
    echo "note: release .sha256 file content differs from computed hash ($(cat "$WORK/slay3rd.sha256")) — using pinned EXPECT_SHA256"
step "   sha256 verified: $GOT_SHA256"
chmod +x "$WORK/slay3rd"

# ---------- 2. smoke run ----------
step "2/7 smoke-run new binary"
"$WORK/slay3rd" --version >/dev/null 2>&1 || fail "binary does not execute (arch/glibc?)"
"$WORK/slay3rd" --version | head -2 || true
if [ -n "${SMOKE_CONFIG:-}" ]; then
    timeout 15 "$WORK/slay3rd" "$SMOKE_CONFIG" >"$WORK/smoke.log" 2>&1 || true
    grep -aqE 'Consensus engine started|listening|INFO' "$WORK/smoke.log" || \
        fail "smoke-run produced no startup output — check $WORK/smoke.log"
    step "   side-port smoke ok"
else
    step "   --version ok (set SMOKE_CONFIG for a side-port boot test)"
fi

# ---------- 3. backup ----------
step "3/7 backup current binary"
[ -f "$BINARY_PATH" ] || fail "installed binary not found at $BINARY_PATH"
BACKUP="${BINARY_PATH}.bak-rc17"
if [ ! -f "$BACKUP" ]; then
    cp -p "$BINARY_PATH" "$BACKUP" || fail "backup copy failed"
fi
step "   backup: $BACKUP ($(sha256sum "$BACKUP" | cut -c1-16)…)"

# ---------- 4. already installed? ----------
CUR_SHA="$(sha256sum "$BINARY_PATH" | awk '{print $1}')"
if [ "$CUR_SHA" = "$EXPECT_SHA256" ]; then
    step "RC18 already installed at $BINARY_PATH — skipping swap"
else
    confirm "4/7 ready to STOP the node and swap the binary. Proceed?"

    # ---------- 5. stop / install / start ----------
    step "5/7 stop -> install -> start"
    $STOP_CMD || fail "stop failed"
    install -m 0755 "$WORK/slay3rd" "$BINARY_PATH" || fail "install failed"
    $START_CMD || fail "start failed — binary still at $BINARY_PATH, backup at $BACKUP"
fi

# ---------- 6. readiness ----------
step "6/7 readiness check (90s)"
READY=0
for i in $(seq 1 30); do
    if $LOG_CMD 2>/dev/null | grep -aqE 'Consensus engine started|peer mesh'; then READY=1; break; fi
    sleep 3
done
[ "$READY" = 1 ] || fail "no 'Consensus engine started'/'peer mesh' in logs after 90s"
$LOG_CMD 2>/dev/null | grep -aE 'peer mesh' | tail -2 || true

# ---------- 7. report ----------
step "7/7 report lines — paste ALL of these into the fleet thread:"
SEAT="${SEAT_NAME:-$(hostname -s 2>/dev/null || hostname)}"
$LOG_CMD 2>/dev/null \
  | sed -E 's/\x1b\[[0-9;]*m//g' \
  | grep -a 'Block finalized' \
  | sed -nE 's/.*height[^0-9]*([0-9]{4,}).*digest=([0-9a-f]{16}).*/\1 \2/p' \
  | tail -5 \
  | while read -r h d; do echo "SEAT $SEAT $h $d"; done
echo "ROLLOUT-OK $SEAT"
