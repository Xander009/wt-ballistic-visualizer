// node --max-old-space-size=8000 tests/check_alignment.mjs
// Data check after extraction: terrain height under every mission spawn zone vs the zone's own Y.
// ~0 m median = heightmap and mission coordinates agree. Large positive medians mean the ground there is
// not in the HM2 heightmap (static scene geometry), so terrain-only collision is incomplete on that map.
import { readFileSync, readdirSync } from "node:fs";
import vm from "node:vm";
import zlib from "node:zlib";

const root = new URL("../", import.meta.url);
const ctx = {};
vm.runInNewContext(readFileSync(new URL("index.html", root), "utf8").match(/<script id="core">([\s\S]*?)<\/script>/)[1] + "\nthis.WT=WT;", ctx);
const rows = [];
for (const f of readdirSync(new URL("data/maps/", root)).filter(f => !/\.(obj|tex)\.js$/.test(f))) {
  let M;
  vm.runInNewContext(readFileSync(new URL("data/maps/" + f, root), "utf8"), { WTV_MAP: o => (M = o) });
  const b = zlib.inflateSync(Buffer.from(M.h, "base64"));
  const T = ctx.WT.makeTerrain(M, new Uint16Array(b.buffer, b.byteOffset, b.length / 2));
  const d = [];
  for (const m of M.missions) for (const a of m.areas) {
    const [x, y, z] = a.tm[3];
    if (a.kind === "spawn" && T.inside(x, z)) d.push(y - T.heightAt(x, z));
  }
  d.sort((p, q) => p - q);
  const med = d.length ? d[d.length >> 1] : NaN;
  rows.push({ map: M.name, missions: M.missions.length, spawns: d.length, median: med, flag: !(Math.abs(med) < 3) });
}
rows.sort((a, b) => (b.median || 0) - (a.median || 0));
console.table(rows.map(r => ({ ...r, median: isNaN(r.median) ? "-" : r.median.toFixed(2) })));
console.log(`${rows.filter(r => !r.flag).length}/${rows.length} maps within 3 m`);
