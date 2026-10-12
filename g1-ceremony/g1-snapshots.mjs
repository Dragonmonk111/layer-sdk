#!/usr/bin/env node
/**
 * g1-snapshots.mjs - List offered state-sync snapshots on every gRPC seat.
 * Read-only. Usage: node g1-snapshots.mjs
 */
import http2 from 'node:http2';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const cfg = JSON.parse(readFileSync(join(here, 'g1-endpoints.json'), 'utf8'));
const seats = cfg.seats.filter((s) => s.grpc);

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

function decodeSnapshots(msg) {
  // ListSnapshotsResponse { snapshots: repeated SnapshotMeta = 1 }
  // SnapshotMeta { height=1, format=2, chunks=3 varints; state_root=4 bytes; total_bytes=5 }
  let pos = 0;
  const out = [];
  while (pos < msg.length) {
    let tag;
    [tag, pos] = readVarint(msg, pos);
    const field = Number(tag >> 3n);
    const wire = Number(tag & 7n);
    if (field === 1 && wire === 2) {
      let len;
      [len, pos] = readVarint(msg, pos);
      const meta = msg.subarray(pos, pos + Number(len));
      pos += Number(len);
      let p = 0;
      const m = { height: 0n, format: 0, chunks: 0, state_root: null, total_bytes: 0n };
      while (p < meta.length) {
        let t;
        [t, p] = readVarint(meta, p);
        const f = Number(t >> 3n);
        const w = Number(t & 7n);
        if (w === 0) {
          let v;
          [v, p] = readVarint(meta, p);
          if (f === 1) m.height = v;
          if (f === 2) m.format = Number(v);
          if (f === 3) m.chunks = Number(v);
          if (f === 5) m.total_bytes = v;
        } else if (w === 2) {
          let l;
          [l, p] = readVarint(meta, p);
          const v = meta.subarray(p, p + Number(l));
          p += Number(l);
          if (f === 4) m.state_root = v;
        } else {
          break;
        }
      }
      out.push(m);
    } else if (wire === 0) {
      let v;
      [v, pos] = readVarint(msg, pos);
    } else if (wire === 2) {
      let l;
      [l, pos] = readVarint(msg, pos);
      pos += Number(l);
    } else {
      break;
    }
  }
  return out;
}

function listSnapshots(target, timeoutMs = 8000) {
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
      ':path': '/layer.statesync.v1.Query/ListSnapshots',
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
      done({ ok: true, snapshots: decodeSnapshots(body.subarray(5, 5 + len)) });
    });
    req.end(Buffer.from([0, 0, 0, 0, 0]));
  });
}

for (const s of seats) {
  const r = await listSnapshots(s.grpc);
  if (!r.ok) { console.log(`${s.name.padEnd(12)} ERROR ${r.error}`); continue; }
  if (r.snapshots.length === 0) { console.log(`${s.name.padEnd(12)} offers NO snapshots`); continue; }
  for (const m of r.snapshots) {
    const root = m.state_root ? Buffer.from(m.state_root).toString('hex').slice(0, 16) : '?';
    console.log(`${s.name.padEnd(12)} height=${String(m.height).padEnd(8)} chunks=${String(m.chunks).padEnd(4)} total=${m.total_bytes}B root=${root}`);
  }
}
