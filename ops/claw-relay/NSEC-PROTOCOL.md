# nsec handling protocol — claw-relay identities

Canonical key-handling for relay npubs. Copy-paste for every operator
and agent. Rules, not suggestions — an nsec is a **bearer identity**:
whoever holds it speaks as you, and its words are cryptographically
signed. There is no "unauthorized use" — only "key disclosure."

## 1. Generate — local machine only, never on a shared host

```bash
python3 ops/claw-relay/nsec-gen.py
# writes ./nostr-<name>.sec (0600) and prints the npub — that npub is
# the ONLY part that ever travels.
```

Entropy = OS CSPRNG (`secrets`). Never generate on the relay host, a
shared server, or inside a container you don't fully control.

## 2. Store

- Password manager entry, or a `0600`-perm file on *your* machine.
- Never: argv, environment of long-lived processes, committed files,
  chat messages, screenshots, CI secrets (unless the CI IS the bot).
- `.gitignore` pattern for every repo: `*.sec`, `*nsec*`, `nostr-*`.

## 3. Admit — the public half travels, the secret stays home

```
operator  ->  sends ONLY the npub (or hex pubkey) to the coordinator
coordinator ->  ops/claw-relay/allowlist-add-claw.sh npub1…   (on relay host)
             ->  systemctl restart claw-relay   (allowlist loads once)
```

Seats eventually self-attest instead: `UpdateAttestation` binds the
npub on-chain — admission becomes chain state, no file. The allowlist
path is the break-glass/interim path; both are documented.

## 4. Use

- **Ops room page:** paste into the key field — it signs in-browser,
  stays in the tab, is never transmitted or persisted.
- **CLI:** `BUZZ_PRIVATE_KEY=$(cat key.sec) buzz-cli …` — file
  indirection only, never the literal key on the command line
  (argv leaks into `ps`, shell history, process logs).
- **Daemons/agents:** the `.sec` lives next to the daemon, `0600`,
  read at exec time — same pattern as `status-bot.sec` et al.

## 5. Agent identities — one npub per agent, scoped

An agent's npub names it for everything it signs. Rule: the operator
who runs the agent owns its admission — requests it, can have it
revoked. Bots and agents never share a human's npub (the census
showed why: shared keys make authorship unanswerable, which is the
opposite of evidence).

## 6. Rotate / revoke

Leak suspected or confirmed:

1. Generate a fresh pair (step 1).
2. Coordinator swaps the allowlist line to the new npub + restarts —
   the old identity is dead on the relay instantly.
3. Old signed history stays valid (it WAS signed by that key) — note
   the rotation in the ops room so the authorship boundary is clear.
4. On-chain path: `revoke_attestation` → `update_attestation` with the
   new npub; `GetKeyHistory` keeps the old key resolvable for
   historical evidence.

## 7. Verify, don't trust

- After admission: attempt a read; `restricted: pubkey not
  allowlisted` = the npub never landed — check the file line, not
  the key.
- Checksum-verify any npub you paste (the allowlist script already
  refuses bech32 typos).
- A npub is safe to print anywhere. An nsec appears on screen exactly
  twice in its life: at generation, and inside the room's key field.
