#!/usr/bin/env python3
"""Extract BlockPayload(digest=fdf4ab2e...) bytes from a live slay3rd process.

Anchors on parent_digest||state_root (64B, adjacent at offsets 64/96 in BOTH
the bincode-serialized blob and the BlockPayload struct, declaration order).
For each candidate: height@S==312344, ts@S+8, proposer enumerated over the 4
known ed25519 validator keys (struct form stores a heap ptr, not the bytes).
sha256(bincode_layout) must equal the target digest -> deterministic verify.
"""
import os, struct, hashlib, sys

PID = int(sys.argv[1])
OUT = sys.argv[2] if len(sys.argv) > 2 else "/tmp/fdf4ab2e.bin"

PARENT = bytes.fromhex("7c7d928c014a92363391b1f6b7147a8b9e9502dfc27f3d417d4cd2d968820684")
STATE  = bytes.fromhex("51165397df859ec9fecfc37f134b9a8bdbcd9fd63be4c2152c4b33f6e7fc2517")
TARGET = bytes.fromhex("fdf4ab2eaee1825b06bdac54e072ea0e6df568d0084387f49e4bf1707b327e42")
HEIGHT = 312344
NEEDLE = PARENT + STATE  # 64-byte tail, offsets 64..128 in both forms

PROPOSERS = [
    bytes.fromhex(h) for h in [
        "3dc093b1f8e49071d09f8d09fd8d5d89834066825ae562f0c400b37f4396a1e3",
        "42031ea06ec88d09e1eafc10a866a5741659235d2a701f2183bbc7ff01b4fc27",
        "62e51f5e242b142ae706ee7cecc406be0da66b025b8bb6f4512a0028440de2bc",
        "c98739580820987b7d52e6b0a995e9ac9ad1cf9dec03457d2cab41f68d98846f",
    ]
]

def ser(height, ts, prop):
    # bincode: u64le height, u64le ts, u64le len + prop bytes, u64le txs len=0,
    #          32B parent, 32B state_root
    return (struct.pack("<Q", height) + struct.pack("<Q", ts)
            + struct.pack("<Q", 32) + prop + struct.pack("<Q", 0)
            + PARENT + STATE)

def regions():
    out = []
    with open(f"/proc/{PID}/maps") as f:
        for line in f:
            parts = line.split()
            if len(parts) < 2 or "r" not in parts[1]:
                continue
            # skip file-backed mappings (binaries/libs) — payload lives on heap/anon
            if len(parts) >= 6 and parts[5].startswith("/") and "[heap]" not in parts[5]:
                continue
            a, b = (int(x, 16) for x in parts[0].split("-"))
            if b - a > 0:
                out.append((a, b))
    return out

def check_window(mem_off_S, get):
    """Candidate struct/blob start S: verify fields, try to match digest."""
    try:
        h = struct.unpack("<Q", get(S, 8))[0]
        ts = struct.unpack("<Q", get(S + 8, 8))[0]
    except Exception:
        return None
    if h != HEIGHT:
        return None
    if not (1_700_000_000_000_000_000 < ts < 1_900_000_000_000_000_000):
        return None
    # serialized form: proposer inline at S+24..56 (len@S+16==32, txs len@S+56==0)
    try:
        if struct.unpack("<Q", get(S + 16, 8))[0] == 32 and \
           struct.unpack("<Q", get(S + 56, 8))[0] == 0:
            blob = get(S, 128)
            if hashlib.sha256(blob).digest() == TARGET:
                return ("serialized", ts, None, blob)
    except Exception:
        pass
    # struct form: proposer is one of the 4 known validator keys
    for prop in PROPOSERS:
        blob = ser(HEIGHT, ts, prop)
        if hashlib.sha256(blob).digest() == TARGET:
            return ("struct", ts, prop.hex(), blob)
    return None

def main():
    mem = open(f"/proc/{PID}/mem", "rb", 0)
    regs = regions()
    total = sum(b - a for a, b in regs)
    print(f"PID {PID}: scanning {len(regs)} regions, {total/1e9:.2f} GB", flush=True)
    CHUNK = 64 << 20
    hits = []
    found = None
    for a, b in regs:
        pos = a
        overlap = b""  # carry 63B tail so needle can't straddle chunks
        while pos < b and found is None:
            n = min(CHUNK, b - pos)
            try:
                mem.seek(pos)
                data = overlap + mem.read(n)
            except OSError:
                break
            base = pos - len(overlap)
            i = 0
            while True:
                i = data.find(NEEDLE, i)
                if i < 0:
                    break
                P = base + i  # abs addr of parent_digest (needle head)
                # Field order under repr(Rust) is not guaranteed; try every
                # plausible struct/blob start: parent_digest sits at offset
                # o = P - S for o in 8..120 step 8 (height must precede it).
                for o in range(8, 121, 8):
                    S = P - o
                    res = check_window(S, lambda q, l, m=mem: (m.seek(q), m.read(l))[1])
                    if res:
                        kind, ts, prop, blob = res
                        print(f"HIT @{hex(S)} kind={kind} ts={ts} prop={prop}", flush=True)
                        found = blob
                        break
                if found:
                    break
                hits.append(P)
                i += 1
            overlap = data[-63:]
            pos += n
    if found:
        with open(OUT, "wb") as f:
            f.write(found)
        print(f"RECOVERED: sha256={hashlib.sha256(found).hexdigest()} -> {OUT}")
    else:
        print(f"no verified hit; {len(hits)} needle hits scanned")

main()
