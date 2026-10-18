# JunoClaw Validator Self-Audit Checklist

Run this once after onboarding, then roughly monthly. Each item lists
the check and what "good" looks like. Report blockers in the ops
channel — several of us have been through each of these.

## 1. Service & autostart

```bash
systemctl is-enabled <your-unit>     # → enabled
systemctl is-active  <your-unit>     # → active
systemctl show <your-unit> -p Restart -p RestartSec
# → Restart=always (or on-failure), RestartSec=3-10
```

- The unit survives a reboot: after a host reboot the validator
  rejoins on its own, catches up, and starts signing again.
- `ExecStart` paths are absolute; `User=` is a dedicated non-root
  user if convenient (not required).

## 2. Logs

```bash
journalctl -u <your-unit> --disk-usage
ls -lh <log file your unit writes, if StandardOutput=append:...>
```

- If your unit appends to a file, put it on `logrotate` — unrotated
  logs grow without bound (our own box hit **7 GB** in weeks):

```
# /etc/logrotate.d/junoclaw
/opt/<node>/slay3rd.log {
    daily
    rotate 7
    compress
    copytruncate
    missingok
    notifempty
}
```

(`copytruncate` — the process holds the fd; no restart needed.)

- Useful lines to recognize:
  - `Block finalized — height=…` — healthy, executing.
  - `peer mesh: reached N/M peers in last 2m` (rc11+) — N should
    equal your configured peer count. `only N/M` names the missing
    peer key prefixes.
  - `peer mesh: only 0/M` while the chain is healthy usually means
    *you're* isolated — check connectivity before assuming chain
    trouble.

## 3. Health verification (5 min, do after any restart)

```bash
# Executed height tracks the network tip:
grep 'Block finalized' <log> | tail -1     # height should advance
# Signer bitmaps — your seat's bit should appear in recent certs:
grep -oE 'signers=\[[01]+\]' <log> | tail -5
# Mesh (rc11+):
grep 'peer mesh' <log> | tail -3
# Sockets (rc10 fallback):
ss -tn state established '( dport = :7001 or sport = :7001 )' | wc -l
```

Good = tip advancing within seconds of other seats, your index set
in bitmaps, `reached M/M peers`.

## 4. SSH & box hygiene

```bash
sshd -T | grep -iE 'passwordauthentication|pubkeyauthentication'
# want: pubkeyauthentication yes, passwordauthentication no
ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub
# record the fingerprint; verify it out-of-band after any rebuild
```

- Firewall: only 22 (restricted to your IPs if possible), 7001,
  and 9090 (if you serve gRPC / are a state-sync donor).
- `ufw status` or equivalent should show the intended set only.

## 5. Key custody

```bash
ls -l keys.json                    # 600, owned by the service user
grep -r 'bls_private\|share_hex\|mayo_private' ~ /tmp /root 2>/dev/null | grep -v keys.json
# want: no matches outside keys.json
```

- `keys.json` never enters a git repo, cloud drive, or chat.
- One offline backup (encrypted USB / password manager entry) is
  correct; *no* copies scattered in /tmp or Downloads.
- Ceremony material already handled: `dealer.seed`, `dealing-priv-*`
  files, stray `bls-share.json` — all deleted (post-ceremony
  checklist). Confirm once here: ☐

## 6. State-sync readiness (makes your node self-healing)

If your `node.toml` lacks `[state_sync]`, add:

```toml
[state_sync]
peers = ["57.129.103.229:9090", "51.195.201.74:9090"]   # ovh, vahana
min_anchor_agree = 2
force = false        # set true ONLY to wipe local state and re-sync
```

With this configured, a node that falls irrecoverably behind can be
re-synced in minutes (`force = true` once, restart, flip back to
false). Donor gRPC (9090) must be reachable — if you want to *be* a
donor too, say so in ops; we rotate the donor list.

## 7. Sign-off

Post a one-liner in the ops channel:

```
<moniker>: autostart ✅ logs rotated ✅ ssh keys-only+hostkey ✅
keys.json 600+backed-up ✅ ceremony material deleted ✅ state_sync
configured ✅
```
