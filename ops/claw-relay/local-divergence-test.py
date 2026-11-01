#!/usr/bin/env python3
# local-divergence-test.py — the executable form of divergence-test.md,
# run entirely on one host. Two claw-relay instances share a stub chain
# API that serves a canned ListActiveSeats; allowlists are divergent.
#
# Proves, per the spec's pass criteria:
#   - break-glass events NEVER cross an admission boundary (E1 stays on A)
#   - member events cross via pull gossip (E3 lands on B)
#   - direct write of an unadmitted key is rejected (E4 denied on B)
#   - replay after re-mate dedupes member events and re-drops break-glass
#
#   python3 local-divergence-test.py /path/to/claw-relay
#
import base64, hashlib, importlib.util, json, os, secrets, subprocess, sys, time, urllib.request
from http.server import BaseHTTPRequestHandler, HTTPServer
from threading import Thread

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("nip98", os.path.join(HERE, "nip98-write.py"))
nip98 = importlib.util.module_from_spec(spec); spec.loader.exec_module(nip98)

CHANNEL = "ddddddee-1111-4222-8333-444444444444"
CONTRACT = "juno1fakeattest"
PASS = []

def report(name, ok, detail=""):
    PASS.append(ok)
    print(f"  [{'PASS' if ok else 'FAIL'}] {name} {detail}")

def gen_key():
    return secrets.token_hex(32)

def pub_of(key_hex):
    return nip98.schnorr_keypair(key_hex)[1]

def write_ev(key_hex, relay, content):
    _, pub, sign = nip98.schnorr_keypair(key_hex)
    path = "/events"
    auth = nip98.signed_event(pub, sign, 27235,
        [["u", relay + path], ["method", "POST"]], "")
    ev = nip98.signed_event(pub, sign, 9, [["h", CHANNEL]], content)
    code, resp = nip98.post(relay, path, json.dumps(ev).encode(), auth)
    return code, resp

def query(key_hex, relay):
    _, pub, sign = nip98.schnorr_keypair(key_hex)
    path = "/query"
    auth = nip98.signed_event(pub, sign, 27235,
        [["u", relay + path], ["method", "POST"]], "")
    code, resp = nip98.post(relay, path,
        json.dumps([{"#h": [CHANNEL]}]).encode(), auth)
    try:
        return code, json.loads(resp)
    except Exception:
        return code, resp

# --- stub chain API: ListActiveSeats with one member npub --------------
class Stub(BaseHTTPRequestHandler):
    seat_npub_b64 = ""
    def do_GET(self):
        seat = {"seat_index": 0, "member_addr": "juno1member",
                "hot_ed25519": "AA==", "bls_share_pubkey": "AA==",
                "mayo_pubkey": None, "npub": self.seat_npub_b64,
                "attested_at": 1}
        body = json.dumps({"data": {"seats": [seat]}}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)
    def log_message(self, *a): pass

def main():
    relay_bin = sys.argv[1] if len(sys.argv) > 1 else "claw-relay"
    workdir = "/tmp/claw-div-test"
    os.system(f"rm -rf {workdir}; mkdir -p {workdir}")

    test_key, member_key = gen_key(), gen_key()
    test_pub, member_pub = pub_of(test_key), pub_of(member_key)
    print(f"TEST   npub {test_pub[:16]}… (break-glass on A only)")
    print(f"MEMBER npub {member_pub[:16]}… (in stub ListActiveSeats)")

    Stub.seat_npub_b64 = base64.b64encode(bytes.fromhex(member_pub)).decode()
    api = HTTPServer(("127.0.0.1", 8899), Stub)
    Thread(target=api.serve_forever, daemon=True).start()

    open(f"{workdir}/a-allow.txt", "w").write(test_pub + "\n")
    open(f"{workdir}/b-allow.txt", "w").write("")

    def start(port, tag, extra):
        log = open(f"{workdir}/{tag}.log", "w")
        return subprocess.Popen([
            relay_bin, "--listen", f"127.0.0.1:{port}",
            "--db", f"{workdir}/{tag}.redb",
            "--channels", CHANNEL,
            "--attestation-contract", CONTRACT,
            "--chain-api", "http://127.0.0.1:8899",
            "--allowlist", f"{workdir}/{tag}-allow.txt",
            "--cw4-refresh-secs", "5",
        ] + extra, stdout=log, stderr=log)

    A = "http://127.0.0.1:7701"; B = "http://127.0.0.1:7702"
    procs = [start(7701, "a", []),
             start(7702, "b", ["--peer", "ws://127.0.0.1:7701"])]
    try:
        time.sleep(8)  # admission populate + gossip mate

        # A2: break-glass write on A
        code, r = write_ev(test_key, A, "divergence E1")
        report("A2 break-glass write accepted on A", code == 200 and "accepted" in r, f"HTTP {code}")

        # A6: member write on A
        code, r = write_ev(member_key, A, "divergence E3 member")
        report("A6 member write accepted on A", code == 200 and "accepted" in r, f"HTTP {code}")

        # unadmitted write direct to B
        code, r = write_ev(test_key, B, "divergence E4")
        report("A4' break-glass write REJECTED on B", code != 200 or "accepted" not in r, f"HTTP {code}")

        time.sleep(6)  # one gossip tick

        code, qa = query(member_key, A)
        a_contents = json.dumps(qa)
        code, qb = query(member_key, B)
        b_contents = json.dumps(qb)
        report("E1 stored on A", "divergence E1" in a_contents)
        report("E1 ABSENT on B (local break-glass)", "divergence E1" not in b_contents)
        report("E3 member event crossed to B", "divergence E3" in b_contents)

        # B2: re-mate — restart B, replay must dedupe + re-drop E1
        procs[1].terminate(); procs[1].wait()
        time.sleep(1)
        procs[1] = start(7702, "b", ["--peer", "ws://127.0.0.1:7701"])
        time.sleep(8)

        code, qb2 = query(member_key, B)
        b2 = json.dumps(qb2)
        report("post-remate: E1 still absent on B", "divergence E1" not in b2)
        report("post-remate: E3 still on B", "divergence E3" in b2)
        if isinstance(qb2, list):
            e3 = [e for e in qb2 if "divergence E3" in json.dumps(e)]
            report("post-remate: E3 stored exactly once", len(e3) == 1, f"count={len(e3)}")
    finally:
        for p in procs: p.terminate()
        api.shutdown()

    print(f"\n{'ALL PASS' if all(PASS) else 'FAILURES PRESENT'} — logs in {workdir}/")
    sys.exit(0 if all(PASS) else 1)

if __name__ == "__main__":
    main()
