#!/usr/bin/env node
/**
 * build-genesis.mjs — Generate JunoClaw genesis JSON from snapshot.
 *
 * Strategy:
 *   - DAO wallet gets 1,090,000 ujclaw directly (2%)
 *   - Community treasury gets the remainder (airdrop + community pool)
 *   - After chain launch: deploy airdrop-claim contract, transfer 35M ujclaw to it
 *   - Users claim with merkle proofs from merkle-proofs.json
 *
 * Usage:
 *   node build-genesis.mjs --snapshot <path> --output <path> --dao <addr> [--treasury <addr>] [--gov <addr>] [--insecure-devnet]
 *
 * --dao MUST be a key-controlled junoclaw bech32 account (20-byte payload).
 * NEVER pass a juno-1 contract/ICA address (32-byte payload) — nobody holds a
 * private key for those on this chain and the entire supply would be locked
 * forever. A bech32 length guard below enforces this at build time.
 * For the G1 ceremony use the ceremony-operated key; for G2+ plan is a
 * multisig contract (see docs/GOVERNANCE_PLAN.md).
 *
 * The node's built-in devnet addresses (deployer key = SHA256("junoclaw-deployer-v1"),
 * derivable by anyone) are refused for every role unless --insecure-devnet is
 * passed; nodes likewise refuse such a genesis unless insecure_devnet = true.
 */

import fs from 'fs';
import { createHash } from 'crypto';

const args = process.argv.slice(2);
let snapshotPath = null;
let outputPath = null;
let daoAddress = null;
let treasuryAddress = null;
let govAddress = null;
let airdropFile = null;
let insecureDevnet = false;

for (let i = 0; i < args.length; i++) {
  switch (args[i]) {
    case '--snapshot': snapshotPath = args[++i]; break;
    case '--output': outputPath = args[++i]; break;
    case '--dao': daoAddress = args[++i]; break;
    case '--treasury': treasuryAddress = args[++i]; break;
    case '--gov': govAddress = args[++i]; break;
    case '--airdrop-file': airdropFile = args[++i]; break;
    case '--insecure-devnet': insecureDevnet = true; break;
  }
}

if (!snapshotPath || !outputPath || !daoAddress) {
  console.error('Usage: node build-genesis.mjs --snapshot <path> --output <path> --dao <addr> [--treasury <addr>] [--gov <addr>] [--insecure-devnet]');
  console.error('');
  console.error('--dao is REQUIRED and must be a key-controlled junoclaw account address.');
  console.error('There is intentionally NO default — the previous default was a juno-1');
  console.error('contract address that no key exists for on this chain.');
  process.exit(1);
}

// --- Bech32 guard ------------------------------------------------------------
// Minimal bech32 decode (no checksum check needed for a build-time guard — we
// only need the payload length). Accounts = 20 bytes; contract/ICA = 32 bytes.
const BECH32_CHARSET = 'qpzry9x8gf2tvdw0s3jn54khce6mua7l';

function bech32PayloadBytes(addr) {
  if (!addr || addr.indexOf('1') <= 0) return null;
  const sep = addr.lastIndexOf('1');
  const data = addr.slice(sep + 1).toLowerCase();
  const values = [];
  for (const ch of data) {
    const v = BECH32_CHARSET.indexOf(ch);
    if (v < 0) return null;
    values.push(v);
  }
  // strip 6-char checksum
  const payload5 = values.slice(0, -6);
  // convertbits 5 -> 8
  const out = [];
  let acc = 0, bits = 0;
  for (const v of payload5) {
    acc = (acc << 5) | v;
    bits += 5;
    if (bits >= 8) {
      bits -= 8;
      out.push((acc >> bits) & 0xff);
    }
  }
  return out.length;
}

function assertKeyControlled(addr, role) {
  const hrp = addr.split('1')[0];
  if (hrp !== 'juno') {
    console.error(`FATAL: ${role} address "${addr}" has hrp "${hrp}", expected "juno".`);
    process.exit(1);
  }
  const n = bech32PayloadBytes(addr);
  if (n === null) {
    console.error(`FATAL: ${role} address "${addr}" is not valid bech32.`);
    process.exit(1);
  }
  if (n !== 20) {
    console.error(`FATAL: ${role} address "${addr}" decodes to ${n} bytes.`);
    console.error('20-byte payload = key-controlled account. 32-byte = contract/ICA address');
    console.error('with NO private key on this chain — funds and gov powers would be locked forever.');
    console.error('If this is intentional (genesis-instantiated contract), pass it via a dedicated');
    console.error('flag and instantiate it in genesis wasm state, NOT as a bank/gov address.');
    process.exit(1);
  }
}

// --- Public devnet address guard -----------------------------------------------
// Must match the built-in devnet addresses in app/slay3rd/src/genesis.rs.
const PUBLIC_DEVNET_ADDRESSES = new Map([
  ['juno1dz875zg8p78anpjv3f0qt4gu5a3awpjfhtw992', 'devnet deployer (key = SHA256("junoclaw-deployer-v1"))'],
  ['juno1pkptre7fdkl6gfrzlesjjvhxhlc3r4gmdyychx', 'built-in devnet gov_account'],
]);

function assertNotPublicDevnet(addr, role) {
  const what = PUBLIC_DEVNET_ADDRESSES.get(addr.toLowerCase());
  if (!what) return;
  if (insecureDevnet) {
    console.warn(`WARNING: ${role} is the ${what}. Devnet-only genesis (--insecure-devnet).`);
    return;
  }
  console.error(`FATAL: ${role} address "${addr}" is the ${what}.`);
  console.error('Anyone can derive that key, so it would control the supply and gov powers.');
  console.error('Use the ceremony key (docs/GOVERNANCE_PLAN.md). For a throwaway devnet pass');
  console.error('--insecure-devnet (nodes then also need insecure_devnet = true to load it).');
  process.exit(1);
}

assertKeyControlled(daoAddress, '--dao');
if (treasuryAddress) assertKeyControlled(treasuryAddress, '--treasury');
if (govAddress) assertKeyControlled(govAddress, '--gov');
assertNotPublicDevnet(daoAddress, '--dao');
if (treasuryAddress) assertNotPublicDevnet(treasuryAddress, '--treasury');
if (govAddress) assertNotPublicDevnet(govAddress, '--gov');

const TOTAL_SUPPLY = 54_660_000_000_000; // 54.66M ujclaw in micro units (6 decimals)
const DAO_ALLOCATION = 1_090_000_000_000; // 1.09M ujclaw

console.log(`Reading snapshot from ${snapshotPath}...`);
const snapshot = JSON.parse(fs.readFileSync(snapshotPath, 'utf8'));

// Airdrop total: prefer the merkle-proofs output (which excludes unclaimable
// 32-byte contract/ICA recipients — their share falls into the community pool).
// Falls back to the raw snapshot summary if no --airdrop-file is given.
let airdropAmount = BigInt(snapshot.summary.total_airdrop_ujclaw);
if (airdropFile) {
  const proofs = JSON.parse(fs.readFileSync(airdropFile, 'utf8'));
  airdropAmount = BigInt(proofs.total_airdrop_ujclaw);
  console.log(`Using claimable airdrop total from ${airdropFile}: ${airdropAmount} ujclaw`);
  if (proofs.excluded_ujclaw_to_community_pool) {
    console.log(`Excluded contract/ICA share -> community pool: ${proofs.excluded_ujclaw_to_community_pool} ujclaw`);
  }
}
const communityPool = BigInt(TOTAL_SUPPLY) - airdropAmount - BigInt(DAO_ALLOCATION);

console.log(`\n--- Genesis Allocation ---`);
console.log(`Airdrop (to contract after launch): ${airdropAmount} ujclaw (${(Number(airdropAmount) / 1_000_000).toFixed(2)} JCLAW)`);
console.log(`DAO wallet:                         ${DAO_ALLOCATION} ujclaw (${(DAO_ALLOCATION / 1_000_000).toFixed(2)} JCLAW)`);
console.log(`Community pool (treasury):           ${communityPool} ujclaw (${(Number(communityPool) / 1_000_000).toFixed(2)} JCLAW)`);
console.log(`Total:                              ${TOTAL_SUPPLY} ujclaw (${(TOTAL_SUPPLY / 1_000_000).toFixed(2)} JCLAW)`);

if (!treasuryAddress) {
  // Use DAO wallet as treasury if not specified
  treasuryAddress = daoAddress;
  console.log(`\nNote: --treasury not specified, using DAO wallet as treasury.`);
  console.log(`The DAO wallet will hold both its 2% allocation and the community pool + airdrop funds.`);
  console.log(`After launch: deploy airdrop-claim contract, transfer ${airdropAmount} ujclaw to it.`);
}

if (!govAddress) {
  govAddress = daoAddress;
  console.log(`\nNote: --gov not specified, wasm.gov_account = --dao (root contract operator).`);
}

const bank = [];

if (treasuryAddress === daoAddress) {
  bank.push({
    address: daoAddress,
    balance: [{ denom: 'ujclaw', amount: (BigInt(DAO_ALLOCATION) + communityPool + airdropAmount).toString() }]
  });
} else {
  bank.push(
    {
      address: daoAddress,
      balance: [{ denom: 'ujclaw', amount: BigInt(DAO_ALLOCATION).toString() }]
    },
    {
      address: treasuryAddress,
      balance: [{ denom: 'ujclaw', amount: (communityPool + airdropAmount).toString() }]
    }
  );
}

const genesis = {
  bank,
  wasm: {
    gov_account: govAddress
  }
};

fs.writeFileSync(outputPath, JSON.stringify(genesis, null, 2));
console.log(`\nGenesis written to ${outputPath}`);
console.log(`\nNext steps:`);
console.log(`1. Launch chain with this genesis`);
console.log(`2. Deploy airdrop-claim contract with merkle root`);
console.log(`3. Transfer ${airdropAmount} ujclaw from treasury to airdrop-claim contract`);
console.log(`4. Users claim with merkle proofs from merkle-proofs.json`);
