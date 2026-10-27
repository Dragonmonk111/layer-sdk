#!/usr/bin/env python3
# transcript-anchor.py — compute or verify a Merkle root over a Buzz
# channel message window. Pure: reads the normalized `buzz-cli messages get`
# JSON array on stdin, no network.
#
#   compute [--max-size N]   → prints the anchor package JSON (see below)
#   verify --root HEX        → recomputes and prints OK / MISMATCH + details
#
# Anchor format (JC-TRANSCRIPT-ANCHOR-v1)
# ---------------------------------------
# Windows are half-open CLOCK ranges [from_ts, to_ts) of event `created_at`
# seconds — no message-id cursors, so a verifier needs nothing beyond the
# window bounds the daemon publishes in the channel receipt:
#
#   buzz-cli messages get --channel <uuid> --since <from_ts> --before <to_ts-1>
#
# (the CLI maps --since/--before to Nostr since/until, both inclusive, so the
# exclusive upper bound is `to_ts - 1`). Events created inside an already-
# anchored window that arrive late are simply not covered — the anchor binds
# what the relay serves, it cannot rewrite it.
#
#   1. Keep the events the fetch returned for the window, dedupe by id.
#   2. Sort by (created_at ASC, id ASC).
#   3. leaf_i = SHA256( 0x00 || event_id_bytes )  — the 32-byte Nostr event
#      id already commits to pubkey, created_at, kind, tags and content
#      (id = SHA256 of the NIP-01 serialization), so binding the id binds
#      the whole message. `sig` is relay-stripped and NOT committed.
#   4. Pair adjacent leaves left-to-right:
#      node = SHA256( 0x01 || left || right );  an odd node is promoted
#      unchanged to the next level (domain separation, no duplication).
#   5. root = the single remaining node (hex). One-message window: root=leaf.
#
# Anchor package printed by `compute`:
#   { format, root_hex, root_b64, count, size_bytes, from_ts, to_ts,
#     first_id, last_id, truncated }
#   size_bytes = sum of UTF-8 byte lengths of every event's `content`.
#   --max-size N: if size_bytes would exceed N, truncate at the largest
#   whole-second prefix that fits, set to_ts to the next event's created_at
#   and truncated=true. (The moultbook contract rejects size_bytes above
#   max_size_bytes; the remaining events fall into the next window.)
#   --to-ts T: the claimed exclusive upper bound of the window (the daemon
#   passes its requested window end so the anchor covers quiet tail time,
#   not just up to the last message). Truncation overrides it.
#
import base64
import hashlib
import json
import sys

FORMAT_TAG = "JC-TRANSCRIPT-ANCHOR-v1"


def leaf_hash(event_id_hex: str) -> bytes:
    return hashlib.sha256(b"\x00" + bytes.fromhex(event_id_hex)).digest()


def merkle_root(leaves):
    level = list(leaves)
    while len(level) > 1:
        nxt = []
        for i in range(0, len(level) - 1, 2):
            nxt.append(hashlib.sha256(b"\x01" + level[i] + level[i + 1]).digest())
        if len(level) % 2:
            nxt.append(level[-1])  # odd node promoted unchanged
        level = nxt
    return level[0]


def anchor_package(events, max_size=None, claimed_to=None):
    uniq = {e["id"]: e for e in events}
    ordered = sorted(uniq.values(), key=lambda e: (e.get("created_at", 0), e["id"]))
    truncated = False
    if max_size is not None:
        # largest prefix fitting max_size that ends on a created_at boundary
        total, cut = 0, 0
        for i, e in enumerate(ordered):
            nxt = total + len(e.get("content", "").encode("utf-8"))
            if nxt > max_size:
                break
            total = nxt
            if i + 1 == len(ordered) or ordered[i + 1].get("created_at", 0) != e.get(
                "created_at", 0
            ):
                cut = i + 1
        if cut < len(ordered):
            truncated = True
            if cut == 0:
                sys.stderr.write(
                    "first second of window alone exceeds --max-size; refusing\n"
                )
                sys.exit(1)
            ordered = ordered[:cut]
    ordered_to = ordered[-1].get("created_at", 0) + 1
    if truncated:
        # exclusive bound = created_at of the first excluded event
        kept = {o["id"] for o in ordered}
        ordered_to = min(e.get("created_at", 0) for e in events if e["id"] not in kept)
    elif claimed_to is not None and claimed_to > ordered_to:
        ordered_to = claimed_to
    leaves = [leaf_hash(e["id"]) for e in ordered]
    root = merkle_root(leaves)
    return {
        "format": FORMAT_TAG,
        "root_hex": root.hex(),
        "root_b64": base64.b64encode(root).decode(),
        "count": len(ordered),
        "size_bytes": sum(len(e.get("content", "").encode("utf-8")) for e in ordered),
        "from_ts": ordered[0].get("created_at", 0),
        "to_ts": ordered_to,  # exclusive bound of THIS anchored window
        "first_id": ordered[0]["id"],
        "last_id": ordered[-1]["id"],
        "truncated": truncated,
    }


def main():
    if len(sys.argv) < 2 or sys.argv[1] not in ("compute", "verify"):
        sys.stderr.write(__doc__)
        sys.exit(2)
    mode = sys.argv[1]
    max_size = None
    if "--max-size" in sys.argv:
        i = sys.argv.index("--max-size")
        max_size = int(sys.argv[i + 1])
    claimed_to = None
    if "--to-ts" in sys.argv:
        i = sys.argv.index("--to-ts")
        claimed_to = int(sys.argv[i + 1])
    try:
        events = json.load(sys.stdin)
    except json.JSONDecodeError as e:
        sys.stderr.write(f"bad message JSON on stdin: {e}\n")
        sys.exit(2)
    if not isinstance(events, list):
        sys.stderr.write("expected a JSON array of messages on stdin\n")
        sys.exit(2)
    if not events:
        sys.stderr.write("empty window — nothing to anchor\n")
        sys.exit(1)
    for e in events:
        if not isinstance(e.get("id"), str) or len(e["id"]) != 64:
            sys.stderr.write(f"message missing/invalid 64-hex id: {e.get('id')!r}\n")
            sys.exit(2)

    pkg = anchor_package(events, max_size, claimed_to)
    if mode == "compute":
        print(json.dumps(pkg))
        return
    want = None
    if "--root" in sys.argv:
        i = sys.argv.index("--root")
        want = sys.argv[i + 1].strip().lower() if i + 1 < len(sys.argv) else None
    if not want or len(want) != 64:
        sys.stderr.write("verify needs --root <64-hex>\n")
        sys.exit(2)
    got = pkg["root_hex"]
    if got == want:
        print(f"OK root={got} count={pkg['count']} window={pkg['from_ts']}..{pkg['to_ts']}")
        return
    print(
        f"MISMATCH want={want} got={got} count={pkg['count']} "
        f"window={pkg['from_ts']}..{pkg['to_ts']}"
    )
    sys.exit(1)


if __name__ == "__main__":
    main()
