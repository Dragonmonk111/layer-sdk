#!/usr/bin/env node
/**
 * Merkle Tree Builder for JunoClaw Airdrop
 * 
 * Reads snapshot JSON, builds a SHA-256 merkle tree, outputs:
 * - merkle_root (hex string)
 * - proofs.json: { address: { amount, proof: [hex...] } }
 * 
 * Uses sorted-pair hashing (same as airdrop-claim contract):
 *   leaf = SHA-256(address || amount_be_bytes_16)
 *   parent = SHA-256(min(left, right) || max(left, right))
 * 
 * Usage: node build-merkle-tree.mjs --snapshot=juno-1-snapshot-41655555.json --output=merkle-proofs.json
 */

import fs from 'fs';
import crypto from 'crypto';

const args = process.argv.slice(2);
const snapshotFile = args.find(a => a.startsWith('--snapshot='))?.split('=')[1] || 'juno-1-snapshot-41655555.json';
const outputFile = args.find(a => a.startsWith('--output='))?.split('=')[1] || 'merkle-proofs.json';

function sha256(data) {
  return crypto.createHash('sha256').update(data).digest();
}

function computeLeaf(address, amount) {
  const hasher = crypto.createHash('sha256');
  hasher.update(Buffer.from(address, 'utf8'));
  // amount as 16-byte big-endian (u128)
  const amountBuf = Buffer.alloc(16);
  amountBuf.writeBigUInt64BE(BigInt(amount) >> 64n, 0);
  amountBuf.writeBigUInt64BE(BigInt(amount) & 0xFFFFFFFFFFFFFFFFn, 8);
  hasher.update(amountBuf);
  return hasher.digest();
}

function hashPair(left, right) {
  // Sort pair — same logic as contract
  if (Buffer.compare(left, right) <= 0) {
    return sha256(Buffer.concat([left, right]));
  } else {
    return sha256(Buffer.concat([right, left]));
  }
}

class MerkleTree {
  constructor(leaves) {
    this.leaves = leaves;
    this.layers = [leaves];
    this.build();
  }

  build() {
    let current = this.leaves;
    while (current.length > 1) {
      const next = [];
      for (let i = 0; i < current.length; i += 2) {
        if (i + 1 < current.length) {
          next.push(hashPair(current[i], current[i + 1]));
        } else {
          // Odd node — promote to next level
          next.push(current[i]);
        }
      }
      this.layers.push(next);
      current = next;
    }
    this.root = current[0];
  }

  getRoot() {
    return this.root.toString('hex');
  }

  getProof(index) {
    const proof = [];
    let idx = index;
    for (let level = 0; level < this.layers.length - 1; level++) {
      const layer = this.layers[level];
      const siblingIndex = idx % 2 === 0 ? idx + 1 : idx - 1;
      if (siblingIndex < layer.length) {
        proof.push(layer[siblingIndex].toString('hex'));
      }
      idx = Math.floor(idx / 2);
    }
    return proof;
  }
}

function main() {
  console.log(`Reading snapshot from ${snapshotFile}...`);
  const snapshot = JSON.parse(fs.readFileSync(snapshotFile, 'utf8'));

  if (!snapshot.staked_balances || !Array.isArray(snapshot.staked_balances)) {
    console.error('Invalid snapshot format: missing staked_balances array');
    process.exit(1);
  }

  // Filter out zero-amount entries
  const entries = snapshot.staked_balances.filter(e => BigInt(e.airdrop_ujclaw) > 0n);
  console.log(`Total entries: ${snapshot.staked_balances.length}`);
  console.log(`Entries with non-zero airdrop: ${entries.length}`);

  // Sort by address for deterministic ordering
  entries.sort((a, b) => a.address.localeCompare(b.address));

  // Build leaves
  const leaves = entries.map(e => ({
    address: e.address,
    amount: e.airdrop_ujclaw,
    leaf: computeLeaf(e.address, e.airdrop_ujclaw)
  }));

  console.log('Building merkle tree...');
  const leafBuffers = leaves.map(l => l.leaf);
  const tree = new MerkleTree(leafBuffers);
  const root = tree.getRoot();
  console.log(`Merkle root: ${root}`);

  // Build proofs
  console.log('Generating proofs...');
  const proofs = {};
  for (let i = 0; i < leaves.length; i++) {
    proofs[leaves[i].address] = {
      amount: leaves[i].amount,
      proof: tree.getProof(i)
    };
    if ((i + 1) % 1000 === 0) {
      process.stdout.write(`\rGenerated ${i + 1}/${leaves.length} proofs...`);
    }
  }
  console.log(`\nGenerated ${leaves.length} proofs`);

  const output = {
    merkle_root: root,
    leaf_count: leaves.length,
    total_airdrop_ujclaw: snapshot.summary?.total_airdrop_ujclaw || '0',
    snapshot_block_height: snapshot.snapshot_block_height,
    snapshot_date: snapshot.snapshot_date,
    proofs: proofs
  };

  fs.writeFileSync(outputFile, JSON.stringify(output, null, 2));
  console.log(`\n--- Merkle Tree Summary ---`);
  console.log(`Root: ${root}`);
  console.log(`Leaves: ${leaves.length}`);
  console.log(`Output: ${outputFile}`);
  console.log(`\nNext step: Deploy airdrop-claim contract with merkle_root = "${root}"`);
}

main();
