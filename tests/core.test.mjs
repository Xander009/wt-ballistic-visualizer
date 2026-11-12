// node --test tests/   (Node 18+). Tests the ballistics/terrain core embedded in index.html.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const html = readFileSync(new URL("../index.html", import.meta.url), "utf8");
const core = html.match(/<script id="core">([\s\S]*?)<\/script>/)[1];
const ctx = {};
vm.runInNewContext(core + "\nthis.WT = WT;", ctx);
const WT = ctx.WT;

const L44_DM23 = { v0: 1640, m: 4.3, cal: 0.032, cx: 0.30562 }; // from 120mm_Rheinmetall_L44_user_cannon
const vacuum = { ...L44_DM23, cx: 0 };

function flatTerrain(size, cell, f = () => 0) {
  const w = Math.round(size / cell) + 1, h16 = new Uint16Array(w * w);
  const hmin = -50, hmax = 200;
  for (let j = 0; j < w; j++) for (let i = 0; i < w; i++) h16[j * w + i] = Math.round((f(i * cell, j * cell) - hmin) / (hmax - hmin) * 65535);
  return WT.makeTerrain({ width: w, height: w, cell, ox: 0, oz: 0, hmin, hmax }, h16);
}

test("roof grid raises the surface only where it is set", () => {
  const w = 101, meta = { width: w, height: w, cell: 2, ox: 0, oz: 0, hmin: -50, hmax: 200 };
  const h16 = new Uint16Array(w * w).fill(Math.round(50 / 250 * 65535)); // flat at 0 m
  const q = new Uint16Array(w * w);
  for (let j = 0; j < w; j++) for (let i = 50; i <= 52; i++) q[j * w + i] = 200; // a 12 m wall at x 100..104
  const T = WT.makeTerrain(meta, h16), Tc = WT.makeTerrain(meta, h16, [{ base: 2, scale: 0.05, q }]);
  assert.ok(Math.abs(Tc.heightAt(102, 50) - 12) < 1e-3);
  assert.equal(Tc.srcAt(102, 50), 1); assert.equal(Tc.srcAt(20, 50), 0);
  assert.ok(Math.abs(Tc.heightAt(20, 50) - T.heightAt(20, 50)) < 1e-6);
  assert.notEqual(WT.losBlocked(Tc, { x: 10, z: 50 }, 2, { x: 190, z: 50 }, 2, 1), null);
  assert.equal(WT.losBlocked(T, { x: 10, z: 50 }, 2, { x: 190, z: 50 }, 2, 1), null);
  // yPt keeps the shooter on the ground when it stands on a roof cell
  const tab = WT.buildTable(L44_DM23, -5, 5, 0.5, 2, -50, 150, 2);
  const onRoof = WT.sweepReach(Tc, tab, { x: 102, z: 50 }, { nAz: 4, gunH: 2, tgtH: 1.5 }, 0, 4);
  const onGround = WT.sweepReach(Tc, tab, { x: 102, z: 50 }, { nAz: 4, gunH: 2, tgtH: 1.5, yPt: 0 }, 0, 4);
  assert.ok(onRoof.reduce((a, b) => a + b) !== onGround.reduce((a, b) => a + b));
});

test("visibility: open ground seen, ground behind a wall hidden, wall marked as the cut", () => {
  const w = 201, meta = { width: w, height: w, cell: 2, ox: 0, oz: 0, hmin: -50, hmax: 200 };
  const h16 = new Uint16Array(w * w).fill(Math.round(50 / 250 * 65535));
  const q = new Uint16Array(w * w);
  for (let j = 0; j < w; j++) q[j * w + 120] = 200; // 12 m wall along x = 240
  const T = WT.makeTerrain(meta, h16, [{ base: 2, scale: 0.05, q }]);
  // 4 rays from (200, 200): ray 1 points +X, through the wall at 40 m
  const out = WT.sweepView(T, { x: 200, z: 200 }, { nAz: 4, ds: 2, range: 150, yPt: 0, gunH: 2, tgtH: 1.5 }, 0, 4), n = 76;
  assert.equal(out[0 * n + 30], 1);                 // +Z: open ground at 60 m
  assert.equal(out[1 * n + 10], 1);                 // +X before the wall
  assert.equal(out[1 * n + 20], 3);                 // the wall (grid 1) cuts the view
  assert.equal(out[1 * n + 40], 0);                 // ground 80 m behind... hidden
  // in a hole: 10 m deep pit around the eye sees nothing past its rim
  const pit = new Uint16Array(w * w).fill(Math.round(50 / 250 * 65535));
  for (let j = 90; j <= 110; j++) for (let i = 90; i <= 110; i++) pit[j * w + i] = Math.round(40 / 250 * 65535);
  const out2 = WT.sweepView(WT.makeTerrain(meta, pit), { x: 200, z: 200 }, { nAz: 4, ds: 2, range: 150, yPt: -10, gunH: 2, tgtH: 1.5 }, 0, 4);
  assert.equal(out2[0 * n + 50], 0);
  assert.equal(out2[0 * n + 11], 2);                // the pit's rim (terrain) cuts the view
});

test("vacuum trajectory matches the parabola", () => {
  const e = 5 * WT.DEG, tr = WT.simulate(vacuum, e, 0, 3000);
  for (const s of [500, 1500, 2900]) {
    const exact = s * Math.tan(e) - WT.G * s * s / (2 * vacuum.v0 ** 2 * Math.cos(e) ** 2);
    assert.ok(Math.abs(WT.atDist(tr, "y", s) - exact) < 0.01, `s=${s}`);
  }
});

test("drag slows the shell and adds drop", () => {
  const a = WT.simulate(L44_DM23, 0, 0, 2000), b = WT.simulate(vacuum, 0, 0, 2000);
  assert.ok(WT.atDist(a, "v", 2000) < L44_DM23.v0 - 50);
  assert.ok(WT.atDist(a, "y", 2000) < WT.atDist(b, "y", 2000));
  assert.ok(WT.atDist(a, "t", 2000) > WT.atDist(b, "t", 2000));
});

test("solveExact hits the requested point, low arc first", () => {
  const sols = WT.solveExact(L44_DM23, 1800, 12, 100, -10, 20);
  assert.ok(sols.length >= 1);
  const tr = WT.simulate(L44_DM23, sols[0], 100, 1801);
  assert.ok(Math.abs(WT.atDist(tr, "y", 1800) - 112) < 0.02);
  const slow = { v0: 250, m: 20, cal: 0.15, cx: 0.3 }; // howitzer-like: two arcs inside 0..80 deg
  const two = WT.solveExact(slow, 2000, 0, 0, 0, 80);
  assert.equal(two.length, 2);
  assert.ok(two[0] < two[1]);
});

test("terrain interpolation returns node heights and plane values", () => {
  const T = flatTerrain(100, 2, (x, z) => 0.1 * x + 0.05 * z);
  assert.ok(Math.abs(T.heightAt(10, 20) - 2) < 0.01);
  assert.ok(Math.abs(T.heightAt(11.3, 20.7) - (1.13 + 1.035)) < 0.01); // a plane stays a plane
});

test("sight line blocked by a wall, clear without it", () => {
  const flat = flatTerrain(400, 2), wall = flatTerrain(400, 2, x => (x > 198 && x < 204 ? 10 : 0));
  const A = { x: 50, z: 100 }, B = { x: 350, z: 100 };
  assert.equal(WT.losBlocked(flat, A, 2, B, 1.5, 1), null);
  const at = WT.losBlocked(wall, A, 2, B, 1.5, 1);
  assert.ok(at > 145 && at < 156, `blocked at ${at}`);
});

test("table rows agree with direct simulation", () => {
  const tab = WT.buildTable(L44_DM23, -2, 4, 0.5, 0, -500, 2000, 4);
  const k = 6, e = tab.elev[k], tr = WT.simulate(L44_DM23, e, 0, 2001);
  for (const i of [100, 300, 500]) assert.ok(Math.abs(tab.Y[k * tab.n + i] - WT.atDist(tr, "y", i * 4)) < 0.05);
});

test("reach: flat ground all hit, nothing behind a tall ridge", () => {
  const p = { nAz: 8, gunH: 2, tgtH: 1.5 };
  const flat = flatTerrain(3000, 4);
  const A = { x: 1500, z: 1500 };
  let tab = WT.buildTable(L44_DM23, -10, 20, 0.05, 2, -60, 1200, 4);
  let out = WT.sweepReach(flat, tab, A, p, 0, 8);
  for (const i of [10, 100, 250]) assert.equal(out[i], 1, `ray 0 at ${i * 4} m`);
  // 60 m wall at 800..820 m to the north (+Z): low pitch limit cannot lob over it
  const ridge = flatTerrain(3000, 4, (x, z) => (z > 2300 && z < 2320 ? 60 : 0));
  tab = WT.buildTable(L44_DM23, -10, 3, 0.05, 2, -60, 1200, 4);
  out = WT.sweepReach(ridge, tab, A, p, 0, 8);
  assert.equal(out[150], 1, "600 m: in front of the ridge");
  assert.equal(out[260], 0, "1040 m: behind the ridge");
});

test("hit target: shooters behind a ridge only with a high arc", () => {
  const slow = { v0: 250, m: 20, cal: 0.15, cx: 0.3 };
  const ridge = flatTerrain(4000, 4, (x, z) => (z > 1800 && z < 1830 ? 40 : 0));
  const B = { x: 2000, z: 1500 }, p = { nAz: 8, gunH: 2, tgtH: 1.5 };
  const tab = WT.buildTable(slow, 0, 89, 0.1, 1.5, -60, 1000, 4);
  const out = WT.sweepHit(ridge, tab, B, p, 0, 1); // ray 0 points +Z; ridge 300..330 m from B
  assert.equal(out[37] & 1, 1, "148 m, in front of the ridge: low arc clear");
  assert.equal(out[200] & 1, 0, "800 m, behind the ridge: low arc blocked");
  assert.equal(out[200] & 2, 2, "800 m, behind the ridge: high arc clears");
});
