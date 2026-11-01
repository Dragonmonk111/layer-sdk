#!/bin/bash
# allowlist-add-claw.sh npub1... — append a break-glass npub to the
# claw-relay allowlist file. Same checksum-verified bech32 decode as
# ops/buzz/allowlist-add.sh, but writes a file, not postgres.
#
# IMPORTANT: the allowlist is loaded ONCE at claw-relay start (no file
# watcher). After appending, run `sudo systemctl restart claw-relay`
# for the entry to take effect. Break-glass entries are LOCAL — they
# never federate to peer relays (by design; each relay operator adds
# their own).
set -e
NPUB=$1
F=/opt/claw/state/allowlist.txt
case "$NPUB" in npub1*) ;; *) echo "expected npub1..." >&2; exit 1;; esac
HEX=$(python3 - "$NPUB" <<'PY'
import sys
n=sys.argv[1]
CS='qpzry9x8gf2tvdw0s3jn54khce6mua7l'
pos=n.rindex('1'); rest=n[pos+1:]
d=[CS.index(c) for c in rest]
hx=[ord(c)>>5 for c in n[:pos]]+[0]+[ord(c)&31 for c in n[:pos]]
def pm(v):
    chk=1; G=[0x3b6a57b2,0x26508e6d,0x1ea119fa,0x3d4233dd,0x2a1462b3]
    for x in v:
        b=chk>>25; chk=(chk&0x1ffffff)<<5 ^ x
        for i in range(5):
            chk ^= G[i] if (b>>i)&1 else 0
    return chk
if pm(hx+d)!=1:
    sys.stderr.write('bech32 checksum FAILED — npub is mistyped, refusing to decode\n'); sys.exit(1)
acc=0; bits=0; out=[]
for x in d[:-6]:
    acc=(acc<<5)|x; bits+=5
    if bits>=8: bits-=8; out.append((acc>>bits)&0xff)
print(''.join(f'{b:02x}' for b in out))
PY
)
[ ${#HEX} -eq 64 ] || { echo "bad npub decode" >&2; exit 1; }
touch $F
grep -qi "^$HEX$" $F && { echo "already allowlisted: $HEX"; exit 0; }
echo "$HEX" >> $F
echo "appended $HEX to $F — RESTART REQUIRED: sudo systemctl restart claw-relay"
