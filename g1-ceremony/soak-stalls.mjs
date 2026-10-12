#!/usr/bin/env node
/**
 * soak-stalls.mjs - tip-freeze analysis for soak-t12.log (T12).
 *
 * Definition (agreed 2026-10-12, reconciles the 08:05 overnight
 * entry with the later full-log analysis):
 *   A FREEZE = the cross-seat max finalized height stays unchanged
 *   for >= --min seconds continuously (default 90 s, the monitor's
 *   STALL alarm threshold). A freeze never spans a coverage gap:
 *   if the interval between consecutive samples exceeds --gap s
 *   (default 90 s = 3x the 30 s cadence), the gap is treated as a
 *   monitor outage, an open freeze is closed at the pre-gap
 *   sample, and counting resumes at the post-gap sample.
 *
 * Usage: node soak-stalls.mjs <soak-t12.log> [--min 90] [--gap 90]
 * Read-only. Mirrors soak-stats.mjs conventions (field `t`,
 * seats[].height as decimal string, ok flag).
 */
import { readFileSync } from 'node:fs';

const args = process.argv.slice(2);
const opt = (name, def) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 ? Number(args[i + 1]) : def;
};
const logPath = args.find((a) => !a.startsWith('--'));
const MIN_S = opt('min', 90);
const GAP_S = opt('gap', 90);

const lines = readFileSync(logPath, 'utf8').split('\n').filter((l) => l.trim());
let first = null, last = null;
let tip = -1n, prevT = null;
let freezeStart = null, freezeH = -1n;
const freezes = [];
const gaps = [];
let coverageGapS = 0;

const recordFreeze = () => {
  if (freezeStart === null) return;
  const dur = (freezeStart.endT - freezeStart.t) / 1000;
  if (dur >= MIN_S) freezes.push({ h: freezeH, start: freezeStart.t, dur });
  freezeStart = null;
};

for (const line of lines) {
  let j;
  try { j = JSON.parse(line); } catch { continue; }
  if (!j.seats) continue;
  const t = Date.parse(j.t);
  if (!Number.isFinite(t)) continue;
  let maxH = -1n;
  for (const s of j.seats) {
    if (!s.ok || s.height == null) continue;
    const h = BigInt(s.height);
    if (h > maxH) maxH = h;
  }
  if (maxH < 0n) continue;
  if (first === null) first = { t, h: maxH };
  last = { t, h: maxH };

  if (prevT !== null && t - prevT > GAP_S * 1000) {
    coverageGapS += (t - prevT) / 1000;
    gaps.push({ at: prevT, gapS: (t - prevT) / 1000 });
    recordFreeze();
  }
  if (maxH > tip) {
    recordFreeze();
    tip = maxH;
  } else if (freezeStart === null) {
    freezeStart = { t, endT: t };
    freezeH = tip;
  }
  if (freezeStart !== null) freezeStart.endT = t;
  prevT = t;
}
recordFreeze();

const spanS = last && first ? (last.t - first.t) / 1000 : 0;
const frozenS = freezes.reduce((a, f) => a + f.dur, 0);
const fmt = (ms) => new Date(ms).toISOString();
console.log(`log: ${logPath}`);
console.log(`window: ${first ? fmt(first.t) : '?'} -> ${last ? fmt(last.t) : '?'}  (${(spanS / 3600).toFixed(2)} h)`);
console.log(`tip: ${first ? first.h : '?'} -> ${last ? last.h : '?'}`);
console.log(`coverage gaps excluded: ${gaps.length} (total ${(coverageGapS / 60).toFixed(1)} min)` +
  (gaps.length ? ` — ${gaps.map((g) => `${fmt(g.at)} +${(g.gapS / 60).toFixed(1)}min`).join('; ')}` : ''));
console.log(`\nFREEZES >= ${MIN_S} s: ${freezes.length}`);
if (freezes.length) {
  const durs = freezes.map((f) => f.dur).sort((a, b) => a - b);
  const p = (q) => durs[Math.min(durs.length - 1, Math.floor(q * durs.length))];
  console.log(`  max ${durs[durs.length - 1].toFixed(0)} s   p50 ${p(0.5).toFixed(0)} s   total frozen ${(frozenS / 60).toFixed(1)} min  (${((100 * frozenS) / spanS).toFixed(1)}% of window)`);
  const buckets = { '90-120': 0, '120-180': 0, '180-300': 0, '>300': 0 };
  for (const f of freezes) {
    if (f.dur <= 120) buckets['90-120']++;
    else if (f.dur <= 180) buckets['120-180']++;
    else if (f.dur <= 300) buckets['180-300']++;
    else buckets['>300']++;
  }
  console.log('  buckets: ' + Object.entries(buckets).map(([k, v]) => `${k}s=${v}`).join('  '));
  console.log('  longest 5:');
  for (const f of [...freezes].sort((a, b) => b.dur - a.dur).slice(0, 5)) {
    console.log(`    h=${f.h}  start ${fmt(f.start)}  ${f.dur.toFixed(0)} s`);
  }
}
console.log(`\nverdict inputs: wedges (freeze > 300 s) = ${freezes.filter((f) => f.dur > 300).length}`);
