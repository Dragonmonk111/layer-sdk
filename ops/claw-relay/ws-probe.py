#!/usr/bin/env python3
# ws-probe.py — raw-WebSocket negative probes for conformance.sh.
# Prints a one-word verdict the wrapper diffs between relays.
#
#   --probe forged   : EVENT with a zeroed signature     -> expect "rejected"
#   --probe outsider : valid sig, non-allowlisted pubkey -> expect "rejected"
#   --probe badchan  : valid EVENT, unconfigured h tag   -> expect "rejected"
#   --probe badkind  : valid EVENT, kind=1 (not admitted)-> expect "rejected"
#   --probe eose     : REQ on the channel -> expect "eose"
#
# verdicts: accepted | rejected | eose | timeout | closed | err:<detail>
# Requires: pip install websocket-client   (pure-python WS client)
import argparse, hashlib, json, sys, time

import websocket  # websocket-client

try:
    import coincurve  # optional fast path
    HAVE_CC = True
except Exception:
    HAVE_CC = False


def schnorr_keypair(key_hex: str):
    """Return (privkey_bytes, xonly_pubkey_hex, sign_fn)."""
    if HAVE_CC:
        sk = coincurve.PrivateKey(bytes.fromhex(key_hex))
        return sk, sk.public_key_xonly.format().hex(), lambda m: sk.sign_schnorr(m)
    # fallback: pure-python BIP-340 (slow but fine for one probe)
    return _py_keypair(bytes.fromhex(key_hex))


# --- minimal BIP-340 fallback ------------------------------------------
_P = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFC2F
_N = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141
_Gx = 0x79BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798
_Gy = 0x483ADA7726A3C4655DA4FBFC0E1108A8FD17B448A68554199C47D08FFB10D4B8


def _pt_add(p, q):
    if p is None: return q
    if q is None: return p
    if p[0] == q[0] and p[1] != q[1]: return None
    if p == q:
        l = (3 * p[0] * p[0]) * pow(2 * p[1], _P - 2, _P) % _P
    else:
        l = (q[1] - p[1]) * pow(q[0] - p[0], _P - 2, _P) % _P
    x = (l * l - p[0] - q[0]) % _P
    return (x, (l * (p[0] - x) - p[1]) % _P)


def _pt_mul(k, p):
    r = None
    while k:
        if k & 1: r = _pt_add(r, p)
        p = _pt_add(p, p); k >>= 1
    return r


def _tagged(tag, msg):
    t = hashlib.sha256(tag.encode()).digest()
    return hashlib.sha256(t + t + msg).digest()


def _py_keypair(d0: bytes):
    d = int.from_bytes(d0, "big")
    P = _pt_mul(d, (_Gx, _Gy))
    if P[1] % 2: d = _N - d  # even-y BIP-340 normalization
    pub = _pt_mul(d, (_Gx, _Gy))[0].to_bytes(32, "big").hex()

    def sign(msg32: bytes) -> bytes:
        import os
        aux = os.urandom(32)
        t = bytes(a ^ b for a, b in zip(d.to_bytes(32, "big"), _tagged("BIP0340/aux", aux)))
        k0 = int.from_bytes(_tagged("BIP0340/nonce", t + pub_bytes(d) + msg32), "big") % _N
        R = _pt_mul(k0, (_Gx, _Gy))
        k = k0 if R[1] % 2 == 0 else _N - k0
        e = int.from_bytes(_tagged("BIP0340/challenge", R[0].to_bytes(32, "big") + pub_bytes(d) + msg32), "big") % _N
        s = (k + e * d) % _N
        return R[0].to_bytes(32, "big") + s.to_bytes(32, "big")

    return d.to_bytes(32, "big"), pub, sign


def pub_bytes(d: int) -> bytes:
    return _pt_mul(d, (_Gx, _Gy))[0].to_bytes(32, "big")


def make_event(sk, pub: str, sign, content: str, chan: str, kind: int = 9) -> dict:
    ev = {"pubkey": pub, "created_at": int(time.time()), "kind": kind,
          "tags": [["h", chan]], "content": content}
    ser = json.dumps([0, ev["pubkey"], ev["created_at"], ev["kind"], ev["tags"], ev["content"]],
                     separators=(",", ":"), ensure_ascii=False)
    ev["id"] = hashlib.sha256(ser.encode()).hexdigest()
    ev["sig"] = sign(bytes.fromhex(ev["id"])).hex()
    return ev


def recv_until(ws, want: str, timeout=8):
    end = time.time() + timeout
    while time.time() < end:
        try:
            m = json.loads(ws.recv())
        except Exception as e:
            return f"err:{e.__class__.__name__}"
        if m[0] == "OK":
            return "accepted" if m[2] else "rejected"
        if m[0] == want:
            return want.lower()
    return "timeout"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--url", required=True)
    ap.add_argument("--channel", required=True)
    ap.add_argument("--probe", required=True)
    ap.add_argument("--key-file")
    a = ap.parse_args()

    if a.key_file:
        key_hex = open(a.key_file).read().strip()
    else:
        key_hex = "02" * 32  # fixed outsider key — allowlisted nowhere

    _, pub, sign = schnorr_keypair(key_hex)

    ws = websocket.create_connection(a.url, timeout=10)

    if a.probe == "eose":
        ws.send(json.dumps(["REQ", "p1", {"#h": [a.channel]}]))
        print(recv_until(ws, "EOSE"))
        return

    ev = make_event(None, pub, sign, f"probe-{a.probe}-{int(time.time())}",
                    "00000000-0000-0000-0000-000000000000" if a.probe == "badchan" else a.channel,
                    kind=1 if a.probe == "badkind" else 9)
    if a.probe == "forged":
        ev["sig"] = "00" * 64
    ws.send(json.dumps(["EVENT", ev]))
    print(recv_until(ws, "OK"))


if __name__ == "__main__":
    main()
