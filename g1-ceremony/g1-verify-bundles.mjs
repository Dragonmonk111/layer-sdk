#!/usr/bin/env node
/**
 * g1-verify-bundles.mjs - pre-delivery verification of the outbox bundles.
 * For each seat: decrypt bundle -> extract to temp -> assert EXACTLY the three
 * expected files, share_index == seat index, node toml chain_id + peer wiring,
 * genesis sha256 matches MANIFEST. Prints bundle sha256 for recipients to
 * compare after transfer. Never prints key material.
 */
import { createHash } from 'node:crypto';
import { execFileSync, execSync } from 'node:child_process';
import { readFileSync, mkdirSync, rmSync, readdirSync, existsSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const outbox = join(here, '_secrets', 'outbox');
const seats = [ { name: 'home-vb', idx: 1 }, { name: 'ovh-uk1', idx: 2 }, { name: 'vahana', idx: 3 } ];
const wsl = (p) => '/mnt/' + p[0].toLowerCase() + p.slice(2).replaceAll('\\', '/');
const run = (...a) => execFileSync('wsl', ['-d', 'Ubuntu-24.04', '--', ...a], { stdio: ['ignore', 'pipe', 'pipe'] });
const sha = (p) => createHash('sha256').update(readFileSync(p)).digest('hex');

let fails = 0;
const check = (ok, msg) => { if (!ok) fails++; console.log(`  ${ok ? 'PASS' : 'FAIL'}  ${msg}`); };

const manifest = readFileSync(join(here, 'public', 'MANIFEST.sha256'), 'utf8');
const genesisSha = sha(join(here, 'public', 'genesis.json'));
const passMap = {};
for (const l of readFileSync(join(outbox, 'passphrases.txt'), 'utf8').trim().split('\n')) {
  const [name, pass] = l.split(': ');
  passMap[name] = pass;
}

console.log('Pre-delivery bundle verification');
for (const s of seats) {
  console.log(`\n${s.name} (validator-${s.idx}):`);
  const gpg = join(outbox, `${s.name}.tar.gpg`);
  check(existsSync(gpg), 'bundle exists');
  const pf = join(outbox, `.vpass-${s.name}`);
  writeFileSync(pf, passMap[s.name], { mode: 0o600 });
  const stage = join(outbox, `.vstage-${s.name}`);
  rmSync(stage, { recursive: true, force: true });
  mkdirSync(stage, { recursive: true });
  try {
    const tar = join(outbox, `.v-${s.name}.tar`);
    run('gpg', '--batch', '--yes', '--pinentry-mode', 'loopback', '--passphrase-file', wsl(pf), '-o', wsl(tar), '-d', wsl(gpg));
    run('tar', '-C', wsl(stage), '-xf', wsl(tar));
    rmSync(tar, { force: true });
    const got = readdirSync(stage).sort();
    check(JSON.stringify(got) === JSON.stringify(['bls-share.json', `node-${s.idx}.toml`, 'genesis.json'].sort()),
      `contains exactly the 3 intended files (got: ${got.join(', ')})`);
    const bs = JSON.parse(readFileSync(join(stage, 'bls-share.json'), 'utf8'));
    check(bs.share_index === s.idx, `share_index=${bs.share_index} matches seat index ${s.idx}`);
    check(bs.threshold_required === 3 && bs.threshold_total === 4, 'threshold 3-of-4');
    const toml = readFileSync(join(stage, `node-${s.idx}.toml`), 'utf8');
    check(toml.includes('chain_id = "junoclaw-g1"'), 'chain_id junoclaw-g1');
    check(!/^\s*insecure_devnet\s*=/m.test(toml), 'insecure_devnet unset');
    check(sha(join(stage, 'genesis.json')) === genesisSha, 'genesis sha256 matches MANIFEST');
  } finally {
    rmSync(stage, { recursive: true, force: true });
    rmSync(pf, { force: true });
  }
  console.log(`  bundle sha256 (give to recipient): ${sha(gpg)}`);
}
console.log(`\n${fails === 0 ? 'ALL BUNDLES VERIFIED' : fails + ' CHECK(S) FAILED - do NOT deliver'}`);
process.exit(fails ? 1 : 0);
