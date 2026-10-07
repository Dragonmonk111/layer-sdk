#!/usr/bin/env node
/**
 * g1-account.mjs — Generate (or reproduce) the key-controlled G1 account.
 *
 * Matches tx-sender's derivation exactly:  privkey = SHA256(<label>)[0..32]
 * so the SAME label works with `TX_SENDER_KEY_SEED=<label>` to sign txs
 * from this account after launch.
 *
 * Usage:
 *   node g1-account.mjs                # fresh random label
 *   node g1-account.mjs --label <str>  # reproduce address for a known label
 *
 * Output: the juno1 address (safe to share / bake into genesis) and the
 * label (PRIVATE — keep offline; never commit).
 */

import { createHash, createECDH, randomBytes } from 'crypto';

const args = process.argv.slice(2);
let label = null;
for (let i = 0; i < args.length; i++) {
  if (args[i] === '--label') label = args[++i];
}
const generated = !label;
if (!label) label = randomBytes(32).toString('hex');

// --- Cosmos-style address: RIPEMD160(SHA256(compressed secp256k1 pubkey)) ---
const priv = createHash('sha256').update(label, 'utf8').digest();
const ecdh = createECDH('secp256k1');
ecdh.setPrivateKey(priv);
const pubCompressed = ecdh.getPublicKey(null, 'compressed');
const sha = createHash('sha256').update(pubCompressed).digest();
const addrBytes = createHash('ripemd160').update(sha).digest();

// --- bech32 encode (BIP-173), HRP 'juno' ---
const CHARSET = 'qpzry9x8gf2tvdw0s3jn54khce6mua7l';
const polymod = (values) => {
  const GEN = [0x3b6a57b2, 0x26508e6d, 0x1ea119fa, 0x3d4233dd, 0x2a1462b3];
  let chk = 1;
  for (const v of values) {
    const top = chk >> 25;
    chk = ((chk & 0x1ffffff) << 5) ^ v;
    for (let i = 0; i < 5; i++) if ((top >> i) & 1) chk ^= GEN[i];
  }
  return chk;
};
const hrpExpand = (hrp) =>
  [...hrp].map((c) => c.charCodeAt(0) >> 5).concat([0],
    [...hrp].map((c) => c.charCodeAt(0) & 31));
const convertBits = (data, from, to, pad = true) => {
  let acc = 0, bits = 0; const out = [], maxv = (1 << to) - 1;
  for (const v of data) {
    acc = (acc << from) | v; bits += from;
    while (bits >= to) { bits -= to; out.push((acc >> bits) & maxv); }
  }
  if (pad && bits > 0) out.push((acc << (to - bits)) & maxv);
  return out;
};
const data5 = convertBits(addrBytes, 8, 5);
const values = hrpExpand('juno').concat(data5).concat([0, 0, 0, 0, 0, 0]);
const mod = polymod(values) ^ 1;
const checksum = [...Array(6)].map((_, i) => (mod >> (5 * (5 - i))) & 31);
const address = 'juno1' + data5.concat(checksum).map((d) => CHARSET[d]).join('');

console.log(`address: ${address}`);
console.log(`label:   ${label}`);
if (generated) {
  console.log('\n(PRIVATE — store offline. Use with: TX_SENDER_KEY_SEED=<label>)');
}
