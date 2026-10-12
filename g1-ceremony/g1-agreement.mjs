#!/usr/bin/env node
/**
 * g1-agreement.mjs - T2 agreement check: fetch Block(h) from every reachable
 * gRPC seat at the same sampled heights and compare proposal + certificate.
 *
 * Usage:
 *   node g1-agreement.mjs              # default samples: 1, 100, 1000, 3000
 *   node g1-agreement.mjs 1 50 1234   # explicit heights
 *
 * Read-only: only issues lightclient Block(height) queries. Holds no keys.
 * Exit 0 = all sampled heights agree across all reachable seats, 1 otherwise.
 */
import http2 from 'node:http2';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const cfg = JSON.parse(readFileSync(join(here, 'g1-endpoints.json'), 'utf8'));
const seats = cfg.seats.filter((s) => s.grpc);
const heights = process.argv.slice(2).length
  ? process.argv.slice(2).map(Number)
  : [1, 100, 1000, 3000];

const sha = (b) => createHash('sha256').update(b).digest('hex').slice(0, 16);

function encodeVarint(v) {
  const out = [];
  let n = BigInt(v);
  for (;;) {
    const b = Number(n & 0x7fn);
    n >>= 7n;
    out.push(n > 0n ? b | 0x80 : b);
    if (n === 0n) break;
  }
  return Buffer.from(out);
}

function readVarint(buf, pos) {
  let result = 0n;
  let shift = 0n;
  for (;;) {
    const b = buf[pos++];
    result |= BigInt(b & 0x7f) << shift;
    if (!(b & 0x80)) break;
    shift += 7n;
  }
  return [result, pos];
}

function decodeBlockResponse(msg) {
  // QueryBlockResponse { height=1 uint64, timestamp_nanos=2 uint64,
  //   proposal_bytes=3, certificate_bytes=4, payload_bytes=5 }
  let pos = 0;
  const out = { proposal: null, certificate: null, payload: null };
  while (pos < msg.length) {
    let tag;
    [tag, pos] = readVarint(msg, pos);
    const field = Number(tag >> 3n);
    const wire = Number(tag & 7n);
    if (wire === 0) {
      let v;
      [v, pos] = readVarint(msg, pos);
      if (field === 1) out.height = v;
      if (field === 2) out.timestampNanos = v;
      continue;
    }
    if (wire !== 2) break;
    let len;
    [len, pos] = readVarint(msg, pos);
    const v = msg.subarray(pos, pos + Number(len));
    pos += Number(len);
    if (field === 3) out.proposal = v;
    if (field === 4) out.certificate = v;
    if (field === 5) out.payload = v;
  }
  return out;
}

function queryBlock(target, height, timeoutMs = 8000) {
  return new Promise((resolve) => {
    let settled = false;
    const done = (r) => {
      if (settled) return;
      settled = true;
      try { client.close(); } catch { /* ignore */ }
      resolve(r);
    };
    const client = http2.connect(`http://${target}`);
    const timer = setTimeout(() => done({ ok: false, error: 'timeout' }), timeoutMs);
    client.on('error', (e) => { clearTimeout(timer); done({ ok: false, error: e.code || e.message }); });
    const req = client.request({
      ':method': 'POST',
      ':path': '/layer.lightclient.v1.Query/Block',
      'content-type': 'application/grpc',
      te: 'trailers',
    });
    const chunks = [];
    let grpcStatus = '0';
    req.on('trailers', (t) => { if (t['grpc-status'] !== undefined) grpcStatus = String(t['grpc-status']); });
    req.on('response', (h) => { if (h['grpc-status'] !== undefined) grpcStatus = String(h['grpc-status']); });
    req.on('data', (c) => chunks.push(c));
    req.on('error', (e) => { clearTimeout(timer); done({ ok: false, error: e.code || e.message }); });
    req.on('end', () => {
      clearTimeout(timer);
      if (grpcStatus !== '0') return done({ ok: false, error: `grpc-status ${grpcStatus}` });
      const body = Buffer.concat(chunks);
      if (body.length < 5) return done({ ok: false, error: 'empty response' });
      const len = body.readUInt32BE(1);
      const d = decodeBlockResponse(body.subarray(5, 5 + len));
      if (!d.proposal || !d.certificate || !d.payload) return done({ ok: false, error: 'missing proposal/certificate/payload' });
      const payloadDigest = createHash('sha256').update(d.payload).digest();
      const digestInProposal = d.proposal.includes(payloadDigest);
      done({ ok: true, proposal: d.proposal, certificate: d.certificate, payload: d.payload, digestInProposal });
    });
    const body = Buffer.concat([Buffer.from([0x08]), encodeVarint(height)]);
    const frame = Buffer.alloc(5 + body.length);
    frame.writeUInt32BE(body.length, 1);
    body.copy(frame, 5);
    req.end(frame);
  });
}

let fail = 0;
console.log(`\n[${new Date().toISOString()}] ${cfg.chain_id} T2 agreement - seats: ${seats.map((s) => s.name).join(', ')}`);
for (const h of heights) {
  const rs = await Promise.all(seats.map(async (s) => ({ s, r: await queryBlock(s.grpc, h) })));
  const digests = new Map();
  for (const { s, r } of rs) {
    if (!r.ok) { console.log(`  h=${h}  ${s.name.padEnd(12)} ERROR ${r.error}`); fail++; continue; }
    if (!r.digestInProposal) { console.log(`  h=${h}  ${s.name.padEnd(12)} BAD proposal does not embed sha256(payload)`); fail++; }
    const key = `${sha(r.proposal)} / ${sha(r.certificate)} / ${sha(r.payload)}`;
    digests.set(key, (digests.get(key) ?? []).concat(s.name));
  }
  if (digests.size === 1) {
    const [key, names] = digests.entries().next().value;
    console.log(`  h=${String(h).padEnd(6)} AGREE  ${key}  (${names.join(', ')})`);
  } else {
    console.log(`  h=${String(h).padEnd(6)} DIVERGENT:`);
    for (const [key, names] of digests) console.log(`          ${names.join(', ')} -> ${key}`);
    fail++;
  }
}
console.log(fail === 0 ? '\nT2 PASS: all sampled heights agree across reachable seats' : `\nT2 FAIL: ${fail} disagreement(s)/error(s)`);
process.exit(fail === 0 ? 0 : 1);
