#!/usr/bin/env bash
# dkg-pack.sh — Phase 1 sovereign ceremony pack (coordinator side).
#
# Emits one bundle-<moniker>.tar.gz per seat containing:
#   dkg-info.json          (public)
#   shared.json            (public, if present)
#   logs/                  all dealer log-*.json        (public)
#   share-requests/        all request files            (public)
#   privs/                 dealing-priv-*-for-<ME>.json (sealed by dkg-deal;
#                                                        ciphertext — safe in
#                                                        transit, ONLY this
#                                                        seat's files packed)
#   MANIFEST.txt           sha256 of every file above
#
# Plus bundles/MANIFEST-bundles.txt: sha256 of each emitted bundle — the
# coordinator posts those hashes in the message text alongside the links
# (SOVEREIGN-CEREMONY-TRANSFER-PLAN Phase 1; hash-in-message, blob anywhere).
#
# Seat-side: dkg-pack.sh verify bundle-<moniker>.tar.gz
#   re-checks the internal manifest and lists which priv files are inside.
#
# Safety property enforced here: a bundle's privs/ may contain ONLY files
# named *-for-<that-seat>.json — mispack aborts before the tarball exists.
# The priv files are sealed ciphertext to the seat's key, but the check
# catches roster/name bugs early instead of at finalize time on a live host.
set -euo pipefail
cd "$(dirname "$0")"

die() { echo "dkg-pack: $*" >&2; exit 1; }

INFO=${INFO:-ceremony/dkg-info.json}
SHARED=${SHARED:-ceremony/shared.json}
LOGS_DIR=${LOGS_DIR:-logs}
REQUESTS_DIR=${REQUESTS_DIR:-share-requests}
PRIV_DIR=${PRIV_DIR:-privs}
OUT=${OUT:-bundles}

# ---------------------------------------------------------------- verify ---
verify_bundle() {
  local bundle=$1
  [ -f "$bundle" ] || die "no such bundle: $bundle"
  local tmp; tmp=$(mktemp -d); trap 'rm -rf "$tmp"' RETURN
  tar xzf "$bundle" -C "$tmp"
  [ -f "$tmp/MANIFEST.txt" ] || die "$bundle has no MANIFEST.txt"
  (cd "$tmp" && sha256sum -c MANIFEST.txt) || die "$bundle manifest mismatch"
  echo "== $bundle OK — privs inside:"
  ls "$tmp/privs/" 2>/dev/null || echo "   (none)"
}

if [ "${1:-}" = "verify" ]; then
  verify_bundle "${2:?usage: dkg-pack.sh verify <bundle.tar.gz>}"
  exit 0
fi

# ------------------------------------------------------------------ pack ---
[ -f "$INFO" ]            || die "missing $INFO"
[ -d "$LOGS_DIR" ]        || die "missing $LOGS_DIR/"
[ -d "$REQUESTS_DIR" ]    || die "missing $REQUESTS_DIR/"
[ -d "$PRIV_DIR" ]        || die "missing $PRIV_DIR/"
ls "$LOGS_DIR"/log-*.json >/dev/null 2>&1 || die "no dealer logs in $LOGS_DIR/"
ls "$REQUESTS_DIR"/*.json >/dev/null 2>&1 || die "no share-requests in $REQUESTS_DIR/"

SEATS=${SEATS:-}
if [ -z "$SEATS" ]; then
  for f in "$REQUESTS_DIR"/*.json; do
    SEATS="$SEATS $(basename "$f" .json)"
  done
fi

mkdir -p "$OUT"
: > "$OUT/MANIFEST-bundles.txt"

for seat in $SEATS; do
  tmp=$(mktemp -d)
  mkdir -p "$tmp/logs" "$tmp/share-requests" "$tmp/privs"
  cp "$INFO" "$tmp/dkg-info.json"
  [ -f "$SHARED" ] && cp "$SHARED" "$tmp/shared.json"
  cp "$LOGS_DIR"/log-*.json "$tmp/logs/"
  cp "$REQUESTS_DIR"/*.json "$tmp/share-requests/"

  n=0
  for f in "$PRIV_DIR"/dealing-priv-*-for-"$seat".json; do
    [ -e "$f" ] || continue
    cp "$f" "$tmp/privs/"
    n=$((n+1))
  done
  [ "$n" -gt 0 ] || echo "!! WARN: no privs for $seat — bundle will be public-only"

  # mispack guard: every priv in this bundle must be sealed for THIS seat
  for f in "$tmp"/privs/*; do
    [ -e "$f" ] || continue
    case "$(basename "$f")" in
      *-for-"$seat".json) ;;
      *) die "mispack: $(basename "$f") is not sealed for $seat — aborting" ;;
    esac
  done

  (cd "$tmp" && find . -type f ! -name MANIFEST.txt -print0 \
      | sort -z | xargs -0 sha256sum > MANIFEST.txt)
  tar czf "$OUT/bundle-$seat.tar.gz" -C "$tmp" .
  rm -rf "$tmp"

  verify_bundle "$OUT/bundle-$seat.tar.gz"
  (cd "$OUT" && sha256sum "bundle-$seat.tar.gz") >> "$OUT/MANIFEST-bundles.txt"
done

echo "== packed $(echo $SEATS | wc -w) bundle(s) into $OUT/"
echo "== bundle hashes:"
cat "$OUT/MANIFEST-bundles.txt"
