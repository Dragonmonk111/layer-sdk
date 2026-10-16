# JunoClaw — Agent Operating Rules

## Ceremony secret handling (NON-NEGOTIABLE)

1. **Never serve a directory over HTTP without auditing its contents first.**
   Create a dedicated clean directory (e.g. `~/serve/`), copy ONLY the
   public file into it, and serve that. Never point a server at a
   working directory that may contain `dealer.seed`, `keys.json`,
   `dealing-priv-*`, or key backups. (Near-miss: 2026-10-16, G2 reshare.)

2. **Never open a firewall port before the served directory is verified
   clean.** Order is: stage clean dir → start server → verify listing
   → then open port. Verify externally from a second host, not just
   localhost.

3. **Keep the access log.** A shared `> log` redirect overwrites the
   previous server instance's log — use `>>` so request evidence of
   earlier instances isn't destroyed.

4. **`keys.json` never leaves the operator's host.** `dealer.seed`
   stays on the dealer's host until `dkg-log`, then is deleted. Only
   `dealing-priv-*-for-*.json` and `dealing-pub-*.json` travel, and the
   priv files go only to their named recipient.

5. **Broad globs on remote dirs are dangerous.** `scp host:dir/*`
   pulled `dealer.seed` to the coordinator once. Copy named files only.

6. **Git:** `g2-window/{ceremony,dealers,g1-keys,share-requests,acks}/`,
   `dealer.seed`, `keys.json`, `bls-share.json` are gitignored. Never
   `git add` a ceremony dir wholesale. Check `git status` + search for
   secret filenames before every commit.

7. **Reshare, never re-deal:** live set changes use `dkg-info → dkg-deal
   → dkg-ack → dkg-log → dkg-observe → dkg-finalize`, preserving the
   group key (IBC trust root). `assemble-genesis` creates a NEW group
   key — never use it on a live chain.

8. **Known trust ceiling:** in the current file-relay flow the
   coordinator sees plaintext per-player dealing messages in transit.
   Before mainnet, encrypt each `dealing-priv-*` file to the
   recipient's Ed25519 key (X25519) so transit files are ciphertext.
