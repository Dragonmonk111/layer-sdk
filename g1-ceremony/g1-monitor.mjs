#!/usr/bin/env node
/**
 * g1-monitor.mjs - G1 closed-testnet health monitor (no dependencies).
 *
 * Polls gRPC `layer.lightclient.v1.Query/LatestHeight` on every seat and prints:
 *   per-seat height + age, cross-seat height spread, block period, stall alarms.
 *
 * Usage:
 *   node g1-monitor.mjs                       # uses ./g1-endpoints.json
 *   node g1-monitor.mjs --config my.json --interval 5 --once
 *
 * Exit code (--once): 0 healthy, 1 degraded (a seat down / lagging), 2 halted.
 * Read-only: only issues LatestHeight queries. Holds no keys.
 */
import http2 from 'node:http2';
import { readFileSync, appendFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const args = process.argv.slice(2);
const opt = (name, def) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 ? args[i + 1] : def;
};
const once = args.includes('--once');
const intervalS = Number(opt('interval', '5'));
const configPath = opt('config', join(here, 'g1-endpoints.json'));
const logPath = opt('log', null);
const MAX_LAG = Number(opt('max-lag', '5'));
const STALL_S = Number(opt('stall-seconds', '30'));

const cfg = JSON.parse(readFileSync(configPath, 'utf8'));
const polled = cfg.seats.filter((s) => s.grpc);
const logOnly = cfg.seats.filter((s) => !s.grpc);

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

function decodeLatestHeight(msg) {
  let pos = 0;
  const out = { height: 0n, timestampNanos: 0n };
  while (pos < msg.length) {
    let tag;
    [tag, pos] = readVarint(msg, pos);
    const field = Number(tag >> 3n);
    const wire = Number(tag & 7n);
    if (wire !== 0) break;
    let v;
    [v, pos] = readVarint(msg, pos);
    if (field === 1) out.height = v;
    if (field === 2) out.timestampNanos = v;
  }
  return out;
}

function queryLatestHeight(target, timeoutMs = 4000) {
  return new Promise((resolve) => {
    const started = Date.now();
    let settled = false;
    const done = (r) => {
      if (settled) return;
      settled = true;
      try { client.close(); } catch { /* ignore */ }
      resolve({ ...r, rttMs: Date.now() - started });
    };
    const client = http2.connect(`http://${target}`);
    const timer = setTimeout(() => done({ ok: false, error: 'timeout' }), timeoutMs);
    client.on('error', (e) => { clearTimeout(timer); done({ ok: false, error: e.code || e.message }); });
    const req = client.request({
      ':method': 'POST',
      ':path': '/layer.lightclient.v1.Query/LatestHeight',
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
      const d = decodeLatestHeight(body.subarray(5, 5 + len));
      done({ ok: true, height: d.height, timestampNanos: d.timestampNanos });
    });
    req.end(Buffer.from([0, 0, 0, 0, 0]));
  });
}

const history = new Map();

async function tick() {
  const results = await Promise.all(
    polled.map(async (s) => ({ seat: s, r: await queryLatestHeight(s.grpc) })),
  );
  const now = Date.now();
  const live = results.filter((x) => x.r.ok);
  const maxH = live.reduce((m, x) => (x.r.height > m ? x.r.height : m), 0n);
  let degraded = false;

  console.log(`\n[${new Date(now).toISOString()}] ${cfg.chain_id}  (${live.length}/${results.length} gRPC seats reachable; log-only: ${logOnly.map((s) => s.name).join(', ') || 'none'})`);
  for (const { seat, r } of results) {
    if (!r.ok) {
      degraded = true;
      console.log(`  ${seat.name.padEnd(12)} DOWN   ${seat.grpc}  (${r.error})`);
      continue;
    }
    const h = history.get(seat.name) ?? [];
    h.push({ t: now, height: r.height });
    while (h.length > 24) h.shift();
    history.set(seat.name, h);

    const lag = maxH - r.height;
    const first = h[0];
    const dt = (now - first.t) / 1000;
    const period = h.length > 1 && r.height > first.height
      ? (dt / Number(r.height - first.height)).toFixed(3) + 's/blk'
      : 'n/a';
    const ageS = r.timestampNanos > 0n ? ((now - Number(r.timestampNanos / 1_000_000n)) / 1000).toFixed(1) : '?';
    const stalled = h.length > 1 && r.height === first.height && dt >= STALL_S;
    const flags = [];
    if (lag > BigInt(MAX_LAG)) { flags.push(`LAGGING-${lag}`); degraded = true; }
    if (stalled) { flags.push('STALLED'); degraded = true; }
    console.log(
      `  ${seat.name.padEnd(12)} h=${String(r.height).padEnd(9)} lag=${String(lag).padEnd(4)} ` +
      `period=${period.padEnd(10)} tip-age=${ageS}s rtt=${r.rttMs}ms ${flags.join(' ')}`,
    );
  }

  let halted = false;
  if (live.length > 0) {
    const tipHist = history.get(live[0].seat.name) ?? [];
    const first = tipHist[0];
    if (first && maxH === first.height && (now - first.t) / 1000 >= STALL_S && tipHist.length > 1) halted = true;
  }
  const quorum = Math.floor((2 * cfg.seats.length) / 3) + 1;
  if (live.length + logOnly.length < quorum) { halted = true; console.log(`  !! below quorum (${live.length} polled + ${logOnly.length} unverified log-only < ${quorum}) - chain cannot finalize`); }
  if (halted) console.log('  !! CHAIN HALTED (no new finalized height)');
  if (logPath) {
    appendFileSync(logPath, JSON.stringify({
      t: new Date(now).toISOString(),
      seats: results.map(({ seat, r }) => ({ name: seat.name, ok: r.ok, height: r.ok ? String(r.height) : null, rttMs: r.rttMs, error: r.error ?? null })),
    }) + '\n');
  }
  return halted ? 2 : degraded ? 1 : 0;
}

if (once) {
  process.exit(await tick());
} else {
  for (;;) {
    await tick();
    await new Promise((r) => setTimeout(r, intervalS * 1000));
  }
}
