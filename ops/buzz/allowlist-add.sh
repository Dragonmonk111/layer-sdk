#!/bin/bash
# allowlist-add.sh npub1... ['note'] — add a Nostr pubkey to the relay allowlist.
# Decodes bech32 npub -> hex (WITH checksum verification — a typo'd npub
# must fail loudly, not silently allowlist garbage) and upserts into
# pubkey_allowlist for community relay.junoclaw.xyz. Idempotent
# (ON CONFLICT DO NOTHING).
set -e
NPUB=$1; NOTE=${2:-manual add}
case "$NPUB" in npub1*) ;; *) echo "expected npub1..." >&2; exit 1;; esac
HEX=$(python3 - "$NPUB" <<PY
import sys
n=sys.argv[1]
CS='qpzry9x8gf2tvdw0s3jn54khce6mua7l'
pos=n.rindex('1'); hrp=n[:pos]; rest=n[pos+1:]
d=[CS.index(c) for c in rest]
hx=[ord(c)>>5 for c in hrp]+[0]+[ord(c)&31 for c in hrp]
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
docker exec buzz-postgres psql -U buzz -d buzz -c "INSERT INTO pubkey_allowlist (community_id, pubkey, note)
    SELECT id, decode('$HEX','hex'), '$NOTE'
    FROM communities WHERE host='relay.junoclaw.xyz'
    ON CONFLICT DO NOTHING"
echo "allowlisted npub -> $HEX ($NOTE)"
