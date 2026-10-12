#!/usr/bin/env node
/**
 * soak-stats.mjs - summarize a g1-monitor.mjs JSONL log (no dependencies).
 *
 * Usage: node soak-stats.mjs [soak-t12.log] [--seat ovh-uk1]
 * Prints: span, line count, per-seat health, hourly block deltas for the
 * reference seat, mean block period, max inter-sample gap.
 */
import { readFileSync } from 'node:fs';

const args = process.argv.slice(2);
const seatIdx = args.indexOf('--seat');
const SEAT = seatIdx >= 0 ? args[seatIdx + 1] : 'ovh-uk1';
const file = args.find((a, i) => !a.startsWith('--') && args[i - 1] !== '--seat' && args[i - 1] !== '--log') || 'soak-t12.log';

const lines = readFileSync(file, 'utf8').split('\n').filter((l) => l.trim());
const rows = lines.map((l) => { try { return JSON.parse(l); } catch { return null; } }).filter(Boolean);
if (!rows.length) { console.error('no JSONL rows parsed'); process.exit(1); }

const first = rows[0], last = rows[rows.length - 1];
console.log(`log: ${file}  lines=${lines.length} parsed=${rows.length}`);
console.log(`span: ${first.t} -> ${last.t}  (${((new Date(last.t) - new Date(first.t)) / 3600_000).toFixed(1)} h)`);

// per-seat health
const seatNames = [...new Set(rows.flatMap((r) => r.seats.map((s) => s.name)))];
for (const name of seatNames) {
  const s = rows.map((r) => r.seats.find((x) => x.name === name)).filter(Boolean);
  const ok = s.filter((x) => x.ok);
  const errs = {};
  for (const x of s.filter((x) => !x.ok)) errs[x.error || 'unknown'] = (errs[x.error || 'unknown'] || 0) + 1;
  const h0 = ok[0]?.height, h1 = ok[ok.length - 1]?.height;
  console.log(`  ${name.padEnd(12)} ok=${String(ok.length).padEnd(5)} down=${s.length - ok.length}  h ${h0 ?? '-'} -> ${h1 ?? '-'}  ${Object.entries(errs).map(([k, v]) => `${k}×${v}`).join(' ')}`);
}

// max inter-sample gap (monitor outage indicator)
let maxGap = 0, gapAt = '';
for (let i = 1; i < rows.length; i++) {
  const g = new Date(rows[i].t) - new Date(rows[i - 1].t);
  if (g > maxGap) { maxGap = g; gapAt = rows[i].t; }
}
console.log(`max inter-sample gap: ${(maxGap / 1000).toFixed(0)} s @ ${gapAt}`);

// hourly block deltas on reference seat
const ref = rows.map((r) => ({ t: r.t, h: r.seats.find((x) => x.name === SEAT)?.height }))
  .filter((x) => x.h !== undefined && x.h !== null)
  .map((x) => ({ t: new Date(x.t), h: Number(x.h) }));
const buckets = new Map();
for (const { t, h } of ref) {
  const key = t.toISOString().slice(0, 13);
  const b = buckets.get(key) ?? { min: h, max: h };
  b.min = Math.min(b.min, h); b.max = Math.max(b.max, h);
  buckets.set(key, b);
}
console.log(`\nhourly block deltas (${SEAT}):`);
let prevMax = null, total = 0, hourDeltas = [];
for (const [k, b] of buckets) {
  const d = prevMax === null ? null : b.max - prevMax;
  if (d !== null) { total += d; hourDeltas.push(d); }
  console.log(`  ${k}:00  max=${b.max}  ${d === null ? '(first bucket)' : `+${d} blocks`}`);
  prevMax = b.max;
}
const spanS = (ref[ref.length - 1].t - ref[0].t) / 1000;
const blocks = ref[ref.length - 1].h - ref[0].h;
console.log(`\n${SEAT}: h${ref[0].h} -> h${ref[ref.length - 1].h} = ${blocks} blocks in ${(spanS / 3600).toFixed(1)} h`);
if (blocks > 0) {
  console.log(`mean block period: ${(spanS / blocks).toFixed(2)} s/blk  (hourly range ${Math.min(...hourDeltas)}-${Math.max(...hourDeltas)} blocks/h)`);
  const sorted = [...hourDeltas].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  const medianRate = sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
  const bestRate = Math.max(...hourDeltas);
  console.log(`median block period (hourly rate): ${(3600 / medianRate).toFixed(2)} s/blk`);
  console.log(`best sustained hour: ${(3600 / bestRate).toFixed(2)} s/blk (${bestRate} blocks/h)`);
}
