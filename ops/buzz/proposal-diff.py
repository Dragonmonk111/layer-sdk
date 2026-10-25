#!/usr/bin/env python3
# proposal-diff.py <seen.json> — reads list_proposals API response on stdin,
# prints each state-change post as ONE \n-escaped line, rewrites seen file.
import json, sys, time, hashlib

seen_path = sys.argv[1]
try:
    seen = json.load(open(seen_path))
except Exception:
    seen = {}

resp = json.load(sys.stdin)
proposals = resp.get('data', {}).get('proposals', [])
now = {}
for p in proposals:
    pr = p['proposal']
    pid = str(p['id'])
    v = pr['votes']
    status = pr['status']
    fp = hashlib.sha256(f"{status}|{v['yes']}|{v['no']}|{v['abstain']}".encode()).hexdigest()[:16]
    now[pid] = fp
    if seen.get(pid) == fp:
        continue
    exp = pr.get('expiration', {}).get('at_time')
    exp_s = time.strftime('%Y-%m-%d %H:%M UTC', time.gmtime(int(exp[:10]))) if exp else '?'
    quorum = pr['threshold']['threshold_quorum']['quorum']['percent']
    post = (f"[dao:proposal] #{pid} {status.upper()} — \"{pr['title']}\"\n"
            f"quorum {quorum} · majority · expires {exp_s}\n"
            f"votes: yes {v['yes']} · no {v['no']} · abstain {v['abstain']} (total power {pr['total_power']})\n"
            f"To vote: reply !vote yes|no|abstain {pid} signed by your member npub — your seat's daemon casts it on-chain.\n"
            f"{pr.get('description','')[:400]}")
    print(post.replace('\\', '\\\\').replace('\n', '\\n'))
seen.update(now)
json.dump(seen, open(seen_path, 'w'))
