/* T13 clock-skew probe — batch mode.
 * Fetches the last --blocks finalized heights via `tx-sender block --height H
 * --dump` (past heights always respond in ~60ms), parses the proposal prefix
 * for view, and computes consecutive-block timestamp deltas grouped by
 * ordered leader pair (L_h -> L_{h+1}). For pair (i,j), median(dt) - median
 * of the global distribution approximates block_period + skew_j - skew_i;
 * comparing (i,j) vs (j,i) gives ~2*(skew_j - skew_i). Only pairs with a view
 * delta of exactly 1 are counted (timeout views distort timestamps).
 *
 *   node t13-skew-batch.mjs [--blocks 150] [--grpc 51.195.201.74:9090] [--end H]
 *
 * No secrets, no writes to the chain. Queries only.
 */
import { execFile } from 'node:child_process';
import { appendFileSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const TX = join(here, '..', 'target', 'release', 'tx-sender.exe');

const args = process.argv.slice(2);
const flag = (n, d) => { const i = args.indexOf(`--${n}`); return i >= 0 ? args[i + 1] : d; };
const GRPC = flag('grpc', '51.195.201.74:9090');
const N = Number(flag('blocks', 150));
const END = Number(flag('end', 0));
const LOG = join(here, 't13-skew.log');
const DIAG = join(here, 't13-skew-diag.log');

function rpc(a) {
  return new Promise((resolve) => {
    execFile(TX, a, { timeout: 15000, windowsHide: true }, (err, stdout, stderr) => {
      resolve({ ok: !err, code: err && err.code, out: String(stdout) + String(stderr) });
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
  const r = await rpc(['block', '--height', String(h), '--dump', '--grpc', GRPC]);
  if (!/block height:/.test(r.out)) return { ok: false, err: r.out.slice(0, 200) };
  const mTs = r.out.match(/timestamp_nanos: (\d+)/);
  const mP = r.out.match(/proposal_hex: ([0-9a-f]+)/);
  if (!mTs || !mP) return { ok: false, err: 'parse fail: ' + r.out.slice(0, 200) };
  const prop = Buffer.from(mP[1], 'hex');
  let pos = 0;
  const [epoch, p1] = varint(prop, pos);
  const [view, p2] = varint(prop, p1);
  const [parent, p3] = varint(prop, p2);
  return { ok: true, epoch: Number(epoch), view: Number(view), parent: Number(parent), ts_ms: Number(BigInt(mTs[1]) / 1000000n) };
}

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

const seatName = ['home-vb', 'ovh-uk1', 'vahana', 'akash-seat'];
const median = (a) => a.length ? a.slice().sort((x, y) => x - y)[Math.floor(a.length / 2)] : null;

const end = END || tipFromSoakLog();
if (!end) { console.error('no tip: pass --end <height>'); process.exit(1); }
console.log(`fetching h${end - N + 1}..h${end} from ${GRPC}`);

const blocks = new Map();
let fails = 0;
for (let h = end - N + 1; h <= end; h++) {
  const b = await fetchBlock(h);
  if (b.ok) blocks.set(h, b);
  else { fails++; if (fails <= 5) appendFileSync(DIAG, `h${h}: ${b.err}\n`); }
}
console.log(`got ${blocks.size}/${N} blocks (${fails} fail; diag -> ${DIAG})`);

// consecutive-block deltas by ordered leader pair, only when view advanced by 1
const pairDt = new Map(); // "i>j" -> ms[]
const byLeaderLead = new Map(); // leader of h -> dt to h+1 (same-epoch only)
for (let h = end - N + 1; h < end; h++) {
  const a = blocks.get(h), b = blocks.get(h + 1);
  if (!a || !b) continue;
  if (b.view !== a.view + 1) continue;
  const li = a.view % 4, lj = b.view % 4;
  const dt = b.ts_ms - a.ts_ms;
  const k = `${li}>${lj}`;
  (pairDt.get(k) ?? pairDt.set(k, []).get(k)).push(dt);
  (byLeaderLead.get(li) ?? byLeaderLead.set(li, []).get(li)).push(dt);
}

writeFileSync(LOG, '');
for (const [h, b] of [...blocks.entries()].sort((x, y) => x[0] - y[0])) {
  appendFileSync(LOG, JSON.stringify({ h, view: b.view, leader: b.view % 4, ts_ms: b.ts_ms }) + '\n');
}

console.log('\nper-leader median block period (outgoing edge i->next):');
for (const [i, dts] of [...byLeaderLead.entries()].sort()) {
  console.log(`  L${i} (${seatName[i] ?? '?'}): n=${dts.length} med=${median(dts)}ms  [${dts.slice(0, 8).join(',')}]`);
}

console.log('\npairwise median dt (only pairs with n>=3), skew_j - skew_i ~= med(i->j) - globalMed:');
const all = [...pairDt.values()].flat();
const g = median(all);
console.log(`  global median dt = ${g}ms`);
const skew = {};
for (const [k, dts] of pairDt) {
  if (dts.length < 3) continue;
  const [i, j] = k.split('>').map(Number);
  const med = median(dts);
  skew[`s${j}-s${i}`] = med - g;
  console.log(`  L${i}->L${j} (${seatName[i]}->${seatName[j]}): n=${dts.length} med=${med}ms  => (s_j - s_i) ~ ${med - g}ms`);
}
const vals = Object.values(skew).map(Math.abs);
console.log(`\nmax implied pairwise skew ~ ${vals.length ? Math.max(...vals) : 'n/a'}ms  (limit <2000ms: ${Math.max(...vals, 0) < 2000 ? 'PASS' : 'FAIL'})`);
