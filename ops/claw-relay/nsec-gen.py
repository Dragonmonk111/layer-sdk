#!/usr/bin/env python3
"""nsec-gen.py — local nostr keypair generator for claw-relay admission.

Generates a BIP-340 (secp256k1 schnorr) keypair, writes the SECRET to
./nostr-<label>.sec with 0600 perms, prints the npub for admission.

Usage:  python3 nsec-gen.py [label]
Only the printed npub ever leaves this machine.
"""
import os, secrets, stat, sys

P = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFC2F
G = (0x79BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798,
     0x483ADA7726A3C4655DA4FBFC0E1108A8FD17B448A68554199C47D08FFB10D4B8)

def add(Pt, Qt):
    if Pt is None: return Qt
    if Qt is None: return Pt
    if Pt[0] == Qt[0] and (Pt[1] + Qt[1]) % P == 0: return None
    if Pt == Qt:
        m = (3 * Pt[0] * Pt[0]) * pow(2 * Pt[1], P - 2, P) % P
    else:
        m = (Qt[1] - Pt[1]) * pow((Qt[0] - Pt[0]) % P, P - 2, P) % P
    x = (m * m - Pt[0] - Qt[0]) % P
    return (x, (m * (Pt[0] - x) - Pt[1]) % P)

def mul(k, Pt):
    R = None
    while k:
        if k & 1: R = Pt if R is None else add(R, Pt)
        Pt = add(Pt, Pt); k >>= 1
    return R

CS = "qpzry9x8gf2tvdw0s3jn54khce6mua7l"
def _pm(v):
    chk = 1
    for x in v:
        b = chk >> 25
        chk = (chk & 0x1ffffff) << 5 ^ x
        for i, g in enumerate([0x3b6a57b2, 0x26508e6d, 0x1ea119fa, 0x3d4233dd, 0x2a1462b3]):
            chk ^= g if (b >> i) & 1 else 0
    return chk

def bech32(hrp, payload):
    d = [0]
    acc, bits = 0, 0
    for b in payload:
        acc = (acc << 8) | b; bits += 8
        while bits >= 5:
            bits -= 5; d.append((acc >> bits) & 31)
    if bits: d.append((acc << (5 - bits)) & 31)
    hrpexp = [ord(c) >> 5 for c in hrp] + [0] + [ord(c) & 31 for c in hrp]
    pm = _pm(hrpexp + d + [0] * 6) ^ 1
    chk = [(pm >> 5 * (5 - i)) & 31 for i in range(6)]
    return hrp + "1" + "".join(CS[x] for x in d + chk)

label = sys.argv[1] if len(sys.argv) > 1 else "key"
sk = secrets.token_bytes(32)
pk = mul(int.from_bytes(sk, "big"), G)[0].to_bytes(32, "big")

sec_file = f"nostr-{label}.sec"
with open(sec_file, "w") as f:
    f.write(sk.hex() + "\n")
os.chmod(sec_file, stat.S_IRUSR | stat.S_IWUSR)

print(f"npub (admit this):  {bech32('npub', pk)}")
print(f"pubkey hex:         {pk.hex()}")
print(f"secret written:     ./{sec_file}  (0600 — never sends, never commits)")
print(f"nsec (keep offline): {bech32('nsec', sk)}")
