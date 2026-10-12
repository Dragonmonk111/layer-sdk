#!/usr/bin/env node
/**
 * g1-preflight.mjs - deterministic G1 safety gate. Read-only except for the
 * public MANIFEST it writes. Prints PASS/FAIL per invariant; exit 1 on any FAIL.
 * Never prints key material.
 *
 *   node g1-ceremony/g1-preflight.mjs
 */
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync, existsSync, writeFileSync } from 'node:fs';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repo = join(here, '..');
const out = join(here, 'ceremony-out');
const srDir = join(here, 'share-requests');
const pub = join(here, 'public');
const EXPECTED_CHAIN = 'junoclaw-g1';

let fails = 0;
const check = (name, ok, detail = '') => {
  if (!ok) fails++;
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? '  - ' + detail : ''}`);
};
const sha = (p) => createHash('sha256').update(readFileSync(p)).digest('hex');
const git = (...a) => execFileSync('git', a, { cwd: repo, encoding: 'utf8' }).split('\n').filter(Boolean);

// 1. Git hygiene: private material is ignored and never tracked.
const SECRET_RE = /(^|\/)(keys\.json|bls-share\.json)$|(^|\/)_secrets\/|\.age$|\.keys\.b64$/;
const tracked = git('ls-files').filter((f) => SECRET_RE.test(f));
check('no private files are git-tracked', tracked.length === 0, tracked.join(', '));
const untrackedVisible = git('ls-files', '--others', '--exclude-standard').filter((f) => SECRET_RE.test(f));
check('no private files are visible to `git add`', untrackedVisible.length === 0, untrackedVisible.join(', '));

// 2. Share-requests: unique identities, literal-IP p2p (nodes do not resolve DNS).
const reqs = readdirSync(srDir).filter((f) => f.endsWith('.json')).map((f) => ({ f, j: JSON.parse(readFileSync(join(srDir, f), 'utf8')) }));
check('exactly 4 share-requests (N=3f+1, f=1)', reqs.length === 4, `found ${reqs.length}`);
const uniq = (a) => new Set(a).size === a.length;
check('unique monikers', uniq(reqs.map((r) => r.j.moniker)));
check('unique ed25519 identities', uniq(reqs.map((r) => r.j.ed25519_public_hex)));
check('unique MAYO2 public keys', uniq(reqs.map((r) => r.j.mayo_public_hex)));
check('unique p2p addresses', uniq(reqs.map((r) => r.j.p2p_address)));
const ipv4 = /^(\d{1,3}\.){3}\d{1,3}:\d{1,5}$/;
check('p2p addresses are literal IPv4:port', reqs.every((r) => ipv4.test(r.j.p2p_address)), reqs.filter((r) => !ipv4.test(r.j.p2p_address)).map((r) => r.j.moniker).join(','));
check('share-requests carry no private fields', reqs.every((r) => !Object.keys(r.j).some((k) => /priv|secret|seed/i.test(k))));

// 3. Ceremony output: every node.toml wires exactly the other 3 seats, correct chain id, no insecure_devnet.
const vdirs = existsSync(out) ? readdirSync(out).filter((d) => /^validator-\d+$/.test(d)).sort() : [];
check('4 validator packages present', vdirs.length === 4, `found ${vdirs.length}`);
const byEd = new Map(reqs.map((r) => [r.j.ed25519_public_hex, r.j]));
for (const d of vdirs) {
  const i = Number(d.split('-')[1]);
  const toml = readFileSync(join(out, d, `node-${i}.toml`), 'utf8');
  const peers = [...toml.matchAll(/public_key = "([0-9a-f]+)"\s*\naddress = "([^"]+)"/g)].map((m) => ({ pk: m[1], addr: m[2] }));
  const selfIdx = toml.match(/validator_index = (\d+)/)?.[1];
  const wiredOk = peers.length === 3 && peers.every((p) => byEd.get(p.pk)?.p2p_address === p.addr);
  check(`${d}: chain_id=${EXPECTED_CHAIN}, index matches, 3 correct peers`,
    toml.includes(`chain_id = "${EXPECTED_CHAIN}"`) && Number(selfIdx) === i && wiredOk);
  check(`${d}: insecure_devnet unset, hybrid_consensus on`,
    !/^\s*insecure_devnet\s*=/m.test(toml) && /hybrid_consensus = true/.test(toml));
  // Own seat (validator-0) retains its bls-share; delivered seats must have NO
  // coordinator copy left — a lingering share for a delivered seat is a leak.
  const shareP = join(out, d, 'bls-share.json');
  if (i === 0) {
    check(`${d}: bls-share present (own seat, not world-visible to git)`, existsSync(shareP));
  } else {
    check(`${d}: no coordinator bls-share copy (delivered)`, !existsSync(shareP), 'rebuild with assemble-genesis to re-deliver');
  }
}

// 4. Genesis: exists, small, no devnet addresses funded, hash recorded.
const gpath = join(pub, 'genesis.json');
check('genesis.json present', existsSync(gpath));
if (existsSync(gpath)) {
  const g = JSON.parse(readFileSync(gpath, 'utf8'));
  const holders = (g.bank ?? []).map((b) => b.address);
  const devnet = ['juno1dz875zg8p78anpjv3f0qt4gu5a3awpjfhtw992'];
  check('genesis does not fund known devnet account', !holders.some((h) => devnet.includes(h)));
  check('genesis DAO account matches _secrets address',
    existsSync(join(here, '_secrets', 'g1-dao-account.txt')) &&
    readFileSync(join(here, '_secrets', 'g1-dao-account.txt'), 'utf8').includes(`address: ${holders[0]}`), holders[0]);
}

// 4b. Devnet keys (commit c45772c, public on origin/commonware, fixed-seed) must never be reused.
const DEVNET_COMMIT = 'c45772c';
try {
  const devEd = [0, 1, 2, 3].map((i) =>
    JSON.parse(execFileSync('git', ['show', `${DEVNET_COMMIT}:testnet-keys/validator-${i}/keys.json`], { cwd: repo, encoding: 'utf8', maxBuffer: 1 << 26 })).ed25519_public_hex);
  check('no G1 identity equals a public devnet key', reqs.every((r) => !devEd.includes(r.j.ed25519_public_hex)));
} catch (e) {
  check('no G1 identity equals a public devnet key', false, `cannot read devnet keys from git (${DEVNET_COMMIT}): fail closed`);
}

// 5. Deterministic manifest of PUBLIC artifacts only.
const files = [gpath, join(out, 'shared.json'), ...reqs.map((r) => join(srDir, r.f)),
  ...vdirs.map((d) => join(out, d, `node-${d.split('-')[1]}.toml`))].filter((p) => existsSync(p));
const lines = files.map((p) => `${sha(p)}  ${relative(here, p).replaceAll('\\', '/')}`).sort((a, b) => a.split('  ')[1].localeCompare(b.split('  ')[1]));
writeFileSync(join(pub, 'MANIFEST.sha256'), lines.join('\n') + '\n');
console.log('\nPublic artifact manifest (g1-ceremony/public/MANIFEST.sha256):');
for (const l of lines) console.log('  ' + l);
console.log(`\n${fails === 0 ? 'ALL INVARIANTS HOLD' : fails + ' INVARIANT(S) FAILED'}`);
process.exit(fails === 0 ? 0 : 1);
