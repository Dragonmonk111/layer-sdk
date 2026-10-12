#!/usr/bin/env node
/**
 * g1-bundle.mjs - build one AES256-encrypted (gpg symmetric) delivery bundle per
 * remote seat: bls-share.json + node-<i>.toml + genesis.json.
 *
 * Output (git-ignored): g1-ceremony/_secrets/outbox/<seat>.tar.gpg
 *                       g1-ceremony/_secrets/outbox/passphrases.txt
 * Send the .tar.gpg and the passphrase over DIFFERENT channels. Never prints
 * passphrases or key material.
 */
import { randomBytes } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { mkdirSync, writeFileSync, copyFileSync, rmSync, existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const outbox = join(here, '_secrets', 'outbox');
mkdirSync(outbox, { recursive: true });

const seats = [
  { name: 'home-vb', idx: 1 },
  { name: 'ovh-uk1', idx: 2 },
  { name: 'vahana', idx: 3 },
];
const wsl = (p) => '/mnt/' + p[0].toLowerCase() + p.slice(2).replaceAll('\\', '/');
const run = (...a) => execFileSync('wsl', ['-d', 'Ubuntu-24.04', '--', ...a], { stdio: ['ignore', 'pipe', 'pipe'] });

const passFile = join(outbox, 'passphrases.txt');
const lines = [];
for (const s of seats) {
  const stage = join(outbox, `stage-${s.name}`);
  rmSync(stage, { recursive: true, force: true });
  mkdirSync(stage, { recursive: true });
  const src = join(here, 'ceremony-out', `validator-${s.idx}`);
  copyFileSync(join(src, 'bls-share.json'), join(stage, 'bls-share.json'));
  copyFileSync(join(src, `node-${s.idx}.toml`), join(stage, `node-${s.idx}.toml`));
  copyFileSync(join(here, 'public', 'genesis.json'), join(stage, 'genesis.json'));

  const pass = randomBytes(24).toString('base64url');
  const pf = join(outbox, `.pass-${s.name}`);
  writeFileSync(pf, pass, { mode: 0o600 });
  const tar = join(outbox, `${s.name}.tar`);
  const gpg = join(outbox, `${s.name}.tar.gpg`);
  rmSync(gpg, { force: true });
  run('tar', '-C', wsl(stage), '-cf', wsl(tar), '.');
  run('gpg', '--batch', '--yes', '--pinentry-mode', 'loopback', '--passphrase-file', wsl(pf),
    '--symmetric', '--cipher-algo', 'AES256', '-o', wsl(gpg), wsl(tar));
  // verify round-trip before trusting the bundle
  const back = run('gpg', '--batch', '--pinentry-mode', 'loopback', '--passphrase-file', wsl(pf), '-d', wsl(gpg));
  if (!back.length) throw new Error(`bundle verification failed for ${s.name}`);
  for (const p of [stage, tar, pf]) rmSync(p, { recursive: true, force: true });
  lines.push(`${s.name}: ${pass}`);
  console.log(`built ${s.name}.tar.gpg (verified decrypt round-trip)`);
}
writeFileSync(passFile, lines.join('\n') + '\n', { mode: 0o600 });
console.log(`passphrases -> ${passFile} (git-ignored). Send each over a different channel than its bundle.`);
if (!existsSync(passFile) || !readFileSync(passFile, 'utf8')) process.exit(1);
