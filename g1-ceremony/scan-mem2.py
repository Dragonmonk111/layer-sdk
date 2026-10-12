#!/usr/bin/env python3
"""scan-mem2: generalize needle handling.

Each parent_digest||state_root adjacency (needle hit at P) is a candidate TAIL
of a serialized BlockPayload: E = P + 64. Scan backward up to WINDOW for the
LE bytes of height=312344; every occurrence is a candidate blob start S.
Slice mem[S:E], verify sha256 == target. No framing assumptions about txs.
Also keeps the original fixed-window struct check for completeness.
"""
import os, struct, hashlib, sys

PID = int(sys.argv[1])
OUT = sys.argv[2] if len(sys.argv) > 2 else "/tmp/fdf4ab2e.bin"

PARENT = bytes.fromhex("7c7d928c014a92363391b1f6b7147a8b9e9502dfc27f3d417d4cd2d968820684")
STATE  = bytes.fromhex("51165397df859ec9fecfc37f134b9a8bdbcd9fd63be4c2152c4b33f6e7fc2517")
TARGET = bytes.fromhex("fdf4ab2eaee1825b06bdac54e072ea0e6df568d0084387f49e4bf1707b327e42")
HEIGHT = 312344
NEEDLE = PARENT + STATE
HEIGHT_LE = struct.pack("<Q", HEIGHT)
WINDOW = 32 << 20          # scan up to 32MB before each needle hit
MIN_BLOB = 64 + 8 + 8 + 8  # absolute floor

PROPOSERS = [
    bytes.fromhex(h) for h in [
        "3dc093b1f8e49071d09f8d09fd8d5d89834066825ae562f0c400b37f4396a1e3",
        "42031ea06ec88d09e1eafc10a866a5741659235d2a701f2183bbc7ff01b4fc27",
        "62e51f5e242b142ae706ee7cecc406be0da66b025b8bb6f4512a0028440de2bc",
        "c98739580820987b7d52e6b0a995e9ac9ad1cf9dec03457d2cab41f68d98846f",
    ]
]

def regions():
    out = []
    with open(f"/proc/{PID}/maps") as f:
        for line in f:
            parts = line.split()
            if len(parts) < 2 or "r" not in parts[1]:
                continue
            if len(parts) >= 6 and parts[5].startswith("/") and "[heap]" not in parts[5]:
                continue
            a, b = (int(x, 16) for x in parts[0].split("-"))
            if b - a > 0:
                out.append((a, b))
    return out

def read_at(mem, off, n):
    mem.seek(off)
    return mem.read(n)

def try_blob(mem, S, E):
    """Return blob if sha256(mem[S:E]) == TARGET."""
    n = E - S
    if n < MIN_BLOB or n > WINDOW:
        return None
    blob = read_at(mem, S, n)
    if len(blob) != n:
        return None
    if hashlib.sha256(blob).digest() == TARGET:
        return blob
    return None

def main():
    mem = open(f"/proc/{PID}/mem", "rb", 0)
    regs = regions()
    total = sum(b - a for a, b in regs)
    print(f"PID {PID}: scanning {len(regs)} regions, {total/1e9:.2f} GB", flush=True)
    CHUNK = 64 << 20
    hits = 0
    found = None
    for a, b in regs:
        pos = a
        overlap = b""
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
                P = base + i
                E = P + 64
                hits += 1
                # backward window for serialized-form tail
                ws = max(a, E - WINDOW)
                try:
                    w = read_at(mem, ws, E - ws)
                except OSError:
                    w = b""
                j = 0
                while True:
                    j = w.find(HEIGHT_LE, j)
                    if j < 0:
                        break
                    S = ws + j
                    blob = try_blob(mem, S, E)
                    if blob is not None:
                        print(f"HIT serialized @{hex(S)} len={len(blob)}", flush=True)
                        found = blob
                        break
                    j += 1
                if found:
                    break
                # struct-form check: height at S, needle at S+o for o in 8..120
                for o in range(8, 121, 8):
                    S = P - o
                    try:
                        h = struct.unpack("<Q", read_at(mem, S, 8))[0]
                        ts = struct.unpack("<Q", read_at(mem, S + 8, 8))[0]
                    except Exception:
                        continue
                    if h != HEIGHT or not (1_700_000_000_000_000_000 < ts < 1_900_000_000_000_000_000):
                        continue
                    for prop in PROPOSERS:
                        blob = (struct.pack("<Q", HEIGHT) + struct.pack("<Q", ts)
                                + struct.pack("<Q", 32) + prop + struct.pack("<Q", 0)
                                + PARENT + STATE)
                        if hashlib.sha256(blob).digest() == TARGET:
                            print(f"HIT struct @{hex(S)} ts={ts} prop={prop.hex()}", flush=True)
                            found = blob
                            break
                    if found:
                        break
                i += 1
            overlap = data[-63:]
            pos += n
    print(f"needle hits: {hits}", flush=True)
    if found:
        with open(OUT, "wb") as f:
            f.write(found)
        print(f"RECOVERED: sha256={hashlib.sha256(found).hexdigest()} len={len(found)} -> {OUT}")
    else:
        print("no verified hit")

main()
