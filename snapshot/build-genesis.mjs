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
 *   node build-genesis.mjs --snapshot <path> --output <path> [--dao <addr>] [--treasury <addr>]
 */

import fs from 'fs';
import { createHash } from 'crypto';

const args = process.argv.slice(2);
let snapshotPath = null;
let outputPath = null;
let daoAddress = 'juno18k65at7fkf8elhece0fnhsvuxggqg6cved6trp5fyk3lftfn93xsmpeaac';
let treasuryAddress = null;

for (let i = 0; i < args.length; i++) {
  switch (args[i]) {
    case '--snapshot': snapshotPath = args[++i]; break;
    case '--output': outputPath = args[++i]; break;
    case '--dao': daoAddress = args[++i]; break;
    case '--treasury': treasuryAddress = args[++i]; break;
  }
}

if (!snapshotPath || !outputPath) {
  console.error('Usage: node build-genesis.mjs --snapshot <path> --output <path> [--dao <addr>] [--treasury <addr>]');
  process.exit(1);
}

const TOTAL_SUPPLY = 54_660_000_000_000; // 54.66M ujclaw in micro units (6 decimals)
const DAO_ALLOCATION = 1_090_000_000_000; // 1.09M ujclaw

console.log(`Reading snapshot from ${snapshotPath}...`);
const snapshot = JSON.parse(fs.readFileSync(snapshotPath, 'utf8'));

const airdropAmount = BigInt(snapshot.summary.total_airdrop_ujclaw);
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

const bank = [
  {
    address: daoAddress,
    balance: [{ denom: 'ujclaw', amount: (BigInt(DAO_ALLOCATION) + communityPool + airdropAmount).toString() }]
  }
];

const genesis = {
  bank,
  wasm: {
    gov_account: daoAddress
  }
};

fs.writeFileSync(outputPath, JSON.stringify(genesis, null, 2));
console.log(`\nGenesis written to ${outputPath}`);
console.log(`\nNext steps:`);
console.log(`1. Launch chain with this genesis`);
console.log(`2. Deploy airdrop-claim contract with merkle root`);
console.log(`3. Transfer ${airdropAmount} ujclaw from treasury to airdrop-claim contract`);
console.log(`4. Users claim with merkle proofs from merkle-proofs.json`);
