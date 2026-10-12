#!/usr/bin/env node
/**
 * t13-skew-probe.mjs — per-seat wall-clock skew measurement for T13.
 *
 * Block proposals carry a Round{epoch,view}. leader = view % N (round-robin).
 * The block timestamp is the PROPOSER's wall clock at proposal time. When we
 * observe a block finalize, `observed_ms - block_ts_ms` = finalization latency
 * (common to all leaders) + proposer's skew vs our clock. Comparing the median
 * delta across leader buckets gives relative inter-seat clock skew — the thing
 * MAX_FUTURE_DRIFT (2 s) actually constrains.
 *
 *   node t13-skew-probe.mjs [--blocks 150] [--grpc 51.195.201.74:9090]
 *
 * No secrets, no writes to the chain. Queries only.
 */
import { execFile } from 'node:child_process';
import { appendFileSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const TX = join(here, '..', 'target', 'release', 'tx-sender.exe');

const args = process.argv.slice(2);
const flag = (n, d) => { const i = args.indexOf(`--${n}`); return i >= 0 ? args[i + 1] : d; };
const GRPC = flag('grpc', '51.195.201.74:9090');
const MAX_BLOCKS = Number(flag('blocks', 120));
const MAX_MINUTES = Number(flag('minutes', 10));
const START = Number(flag('start', 0));
const LOG = join(here, 't13-skew.log');

function rpc(args) {
  return new Promise((resolve) => {
    execFile(TX, args, { timeout: 15000 }, (err, stdout, stderr) => {
      resolve({ ok: !err, out: stdout + stderr });
    });
  });
}

function varint(buf, pos) {
  let v = 0n, shift = 0n, i = pos;
  for (;;) {
    const b = buf[i++];
    v |= BigInt(b & 0x7f) << shift;
    if (!(b & 0x80)) break;
    shift += 7n;
  }
  return [v, i];
}

async function fetchBlock(h) {
  // NOTE: --grpc must come AFTER the subcommand+flags (tx-sender parses flags positionally)
  const r = await rpc(['block', '--height', String(h), '--dump', '--grpc', GRPC]);
  if (!/block height:/.test(r.out)) return null;
  const mTs = r.out.match(/timestamp_nanos: (\d+)/);
  const mP = r.out.match(/proposal_hex: ([0-9a-f]+)/);
  if (!mTs || !mP) return null;
  const prop = Buffer.from(mP[1], 'hex');
  let pos = 0;
  const [epoch, p1] = varint(prop, pos);
  const [view, p2] = varint(prop, p1);
  const [parent, p3] = varint(prop, p2);
  return { epoch: Number(epoch), view: Number(view), parent: Number(parent), ts_ms: Number(BigInt(mTs[1]) / 1000000n) };
}

const seatName = ['home-vb', 'ovh-uk1', 'vahana', 'akash-seat'];

// Read the current tip from the soak monitor log (heights tracked every 30s).
function tipFromSoakLog() {
  try {
    const lines = readFileSync(join(here, 'soak-t12.log'), 'utf8').trim().split('\n');
    for (let i = lines.length - 1; i >= 0; i--) {
      const m = lines[i].match(/"height":"(\d+)"/);
      if (m) return Number(m[1]);
    }
  } catch {}
  return 0;
}

async function waitTip(h) {
  for (;;) {
    const b = await fetchBlock(h);
    if (b) return { h, ...b };
    await new Promise(r => setTimeout(r, 500));
  }
}

console.log(`probing ${GRPC}; target ${MAX_BLOCKS} blocks, cap ${MAX_MINUTES} min; log -> ${LOG}`);
const rows = [];

let tip = START || tipFromSoakLog();
if (!tip) { console.error('no tip: pass --start <height>'); process.exit(1); }
console.log(`tip ~= h${tip}`);

const deadline = Date.now() + MAX_MINUTES * 60_000;
while (rows.length < MAX_BLOCKS && Date.now() < deadline) {
  const h = tip + 1;
  const observed = Date.now();
  const b = await waitTip(h);
  tip = h;
  const leader = b.view % 4;
  const row = { h, view: b.view, parent: b.parent, leader, delta_ms: observed - b.ts_ms };
  rows.push(row);
  appendFileSync(LOG, JSON.stringify({ ts: new Date(observed).toISOString(), ...row }) + '\n');
  console.log(`h${h} view=${b.view} leader=${leader} (${seatName[leader] ?? '?'}) delta=${row.delta_ms}ms`);
}

// Per-leader median delta -> relative skew table
const byLeader = [[], [], [], []];
for (const r of rows) byLeader[r.leader].push(r.delta_ms);
const med = a => { const s = [...a].sort((x, y) => x - y); return s[Math.floor(s.length / 2)]; };
const meds = byLeader.map(a => a.length ? med(a) : null);
const valid = meds.filter(m => m !== null);
const base = Math.min(...valid);
console.log('\n=== T13 relative clock skew (leader buckets, N=4) ===');
for (let i = 0; i < 4; i++) {
  if (meds[i] === null) { console.log(`seat ${i} (${seatName[i]}): no samples`); continue; }
  console.log(`seat ${i} (${seatName[i]}): n=${byLeader[i].length} median_delta=${meds[i]}ms rel_skew=+${meds[i] - base}ms`);
}
console.log(`(rel_skew vs fastest clock; includes common finalize latency. Max pairwise = ${Math.max(...valid) - base}ms)`);
