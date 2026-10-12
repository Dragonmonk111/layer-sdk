#!/usr/bin/env node
/**
 * g1-akash-manifest.mjs - fill deploy/g1/deploy.yaml with the Akash seat's
 * env (keys.json, node-0.toml, genesis.json) -> g1-ceremony/_secrets/deploy-akash-seat.yaml
 *
 * The output contains PRIVATE key material and lives in the git-ignored
 * _secrets/ directory. It is never printed. Refuses to run if the seat's
 * keys.json has not been finalized (no BLS share merged).
 */
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const b64 = (p) => readFileSync(p).toString('base64');

const keysPath = join(here, 'akash-seat', 'keys.json');
const keys = JSON.parse(readFileSync(keysPath, 'utf8'));
if (!keys.share_hex || !keys.threshold_public_key_hex || keys.threshold_required !== 3) {
  console.error('keys.json has no BLS material - run `finalize` first.');
  process.exit(1);
}
if (keys.validator_index !== 0) {
  console.error(`unexpected validator_index ${keys.validator_index} (expected 0 for akash-seat)`);
  process.exit(1);
}

let sdl = readFileSync(join(here, '..', 'deploy', 'g1', 'deploy.yaml'), 'utf8');
const fill = (name, value) => {
  const re = new RegExp(`(- ${name}=)\\s*$`, 'm');
  if (!re.test(sdl)) throw new Error(`placeholder ${name}= not found in deploy.yaml`);
  sdl = sdl.replace(re, `$1${value}`);
};
fill('KEYS_JSON_B64', b64(keysPath));
fill('NODE_TOML_B64', b64(join(here, 'ceremony-out', 'validator-0', 'node-0.toml')));
fill('GENESIS_B64', b64(join(here, 'public', 'genesis.json')));

const outDir = join(here, '_secrets');
mkdirSync(outDir, { recursive: true });
const outPath = join(outDir, 'deploy-akash-seat.yaml');
writeFileSync(outPath, sdl, { mode: 0o600 });
console.log(`wrote ${outPath} (${sdl.length} bytes) - contains PRIVATE keys, git-ignored, do not share`);
