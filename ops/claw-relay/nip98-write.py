#!/usr/bin/env python3
# nip98-write.py — reference signer for claw-relay's HTTP bridge.
# Posts a signed channel event to POST /events with NIP-98 transport
# auth (`Authorization: Nostr <base64(kind-27235 event)>`), then prints
# the relay's JSON verdict.
#
#   python3 nip98-write.py --relay http://127.0.0.1:7070 \
#       --key-file mykey.hex --channel <uuid> --content "hello"
#
#   --kind 1     drive a genuine kind-1 write (expect reject on
#                channels that don't admit kind 1)
#   --probe      /query | /count instead of /events
#
# Deps: python3 stdlib only (urllib); coincurve optional for fast
# BIP-340 (pure-python fallback otherwise).
import argparse, base64, hashlib, json, sys, time, urllib.request

try:
    import coincurve
    HAVE_CC = True
except Exception:
    HAVE_CC = False


def schnorr_keypair(key_hex: str):
    if HAVE_CC:
        sk = coincurve.PrivateKey(bytes.fromhex(key_hex))
        return sk, sk.public_key_xonly.format().hex(), lambda m: sk.sign_schnorr(m)
    return _py_keypair(bytes.fromhex(key_hex))


# --- minimal BIP-340 fallback (same vectors as ws-probe.py) ----------
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
    if P[1] % 2: d = _N - d
    pub = _pt_mul(d, (_Gx, _Gy))[0].to_bytes(32, "big").hex()

    def sign(msg32: bytes) -> bytes:
        import os
        aux = os.urandom(32)
        t = bytes(a ^ b for a, b in zip(d.to_bytes(32, "big"), _tagged("BIP0340/aux", aux)))
        k0 = int.from_bytes(_tagged("BIP0340/nonce", t + _pt_mul(d, (_Gx, _Gy))[0].to_bytes(32, "big") + msg32), "big") % _N
        R = _pt_mul(k0, (_Gx, _Gy))
        k = k0 if R[1] % 2 == 0 else _N - k0
        e = int.from_bytes(_tagged("BIP0340/challenge", R[0].to_bytes(32, "big") + _pt_mul(d, (_Gx, _Gy))[0].to_bytes(32, "big") + msg32), "big") % _N
        s = (k + e * d) % _N
        return R[0].to_bytes(32, "big") + s.to_bytes(32, "big")

    return d.to_bytes(32, "big"), pub, sign


def _pub_bytes(d: int) -> bytes:
    return _pt_mul(d, (_Gx, _Gy))[0].to_bytes(32, "big")


def signed_event(pub, sign, kind, tags, content):
    ev = {"pubkey": pub, "created_at": int(time.time()), "kind": kind,
          "tags": tags, "content": content}
    ser = json.dumps([0, ev["pubkey"], ev["created_at"], ev["kind"], ev["tags"], ev["content"]],
                     separators=(",", ":"), ensure_ascii=False)
    ev["id"] = hashlib.sha256(ser.encode()).hexdigest()
    ev["sig"] = sign(bytes.fromhex(ev["id"])).hex()
    return ev


def post(relay: str, path: str, body: bytes, auth_event: dict):
    url = relay.rstrip("/") + path
    req = urllib.request.Request(url, data=body, method="POST")
    req.add_header("Authorization", "Nostr " + base64.standard_b64encode(
        json.dumps(auth_event, separators=(",", ":")).encode()).decode())
    req.add_header("Accept", "application/nostr+json")
    req.add_header("Content-Type", "application/json")
    try:
        with urllib.request.urlopen(req, timeout=15) as r:
            return r.status, r.read().decode()
    except urllib.error.HTTPError as e:
        return e.code, e.read().decode()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--relay", required=True, help="base URL, e.g. http://127.0.0.1:7070")
    ap.add_argument("--key", help="64-hex private key")
    ap.add_argument("--key-file", help="file containing 64-hex private key")
    ap.add_argument("--channel", help="channel uuid (h tag)")
    ap.add_argument("--kind", type=int, default=9)
    ap.add_argument("--content", default=None)
    ap.add_argument("--probe", choices=["events", "query", "count"], default="events")
    a = ap.parse_args()

    key_hex = a.key or open(a.key_file).read().strip()
    _, pub, sign = schnorr_keypair(key_hex)

    path = f"/{a.probe}"
    auth = signed_event(pub, sign, 27235,
                        [["u", a.relay.rstrip("/") + path], ["method", "POST"]], "")

    if a.probe == "events":
        ev = signed_event(pub, sign, a.kind, [["h", a.channel]],
                          a.content if a.content is not None else f"nip98-write {int(time.time())}")
        body = json.dumps(ev).encode()
    elif a.probe == "query":
        body = json.dumps({"filters": [{"#h": [a.channel]}]}).encode()
    else:
        body = json.dumps({"filters": [{"#h": [a.channel]}]}).encode()

    code, resp = post(a.relay, path, body, auth)
    print(f"HTTP {code} {resp}")


if __name__ == "__main__":
    main()
