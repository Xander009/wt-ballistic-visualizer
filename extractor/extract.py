#!/usr/bin/env python3
"""Extract War Thunder ground maps, mission areas and tank shell ballistics into ../data for the app.

Reads the installed game with wt-unpack.exe (vromfs, BLK -> JSON, HM2 heightmap -> 16-bit PNG).
Writes ../data/index.js and ../data/maps/<level>.js (loaded by <script> tags, so file:// works offline).

Usage:  python extract.py [--game "D:\\...\\War Thunder"] [--levels avg_abandoned_factory,avg_berlin] [--far]
        --far also writes maps/<level>.far.js (+ .far.obj.js, .far.tex.js): the whole level from its landmesh,
        for the Max build's view-only terrain around the tank area.
Needs:  Pillow, numpy.
"""
import argparse
import base64
import csv
import io
import json
import math
import re
import shutil
import struct
import subprocess
import sys
import tempfile
import zlib
from pathlib import Path

import numpy as np
from PIL import Image, ImageOps

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_GAME = r"D:\SteamLibrary\steamapps\common\War Thunder"
MAX_GRID = 4096  # heightmaps above this are strided down to keep the browser under ~100 MB
RUNWAY_LIFT = 2.0  # metres a runway or pad surface floats over its group: the 32 m whole-level ground is too coarse to follow it
SLAB_PX = 2048  # largest side of a slab texture kept (the app draws airfields at about 2 m, heli pads at 0.5 m)
SLAB_TILE = 8  # metres per landmesh height around airfields and heli pads
SLAB_MARGIN = 96  # metres of that detail kept around a slab
FAR_CELL = 32  # metres per grid step of the whole-level (--far) terrain: about 2048 x 2048 on a 65 km level


def find_wt_unpack(arg):
    if arg:
        return Path(arg)
    hits = sorted(ROOT.glob("wt-unpack*/wt-unpack.exe"))
    if not hits:
        sys.exit("wt-unpack.exe not found next to the project; pass --wt-unpack")
    return hits[-1]


def run(exe, *args):
    r = subprocess.run([str(exe), *map(str, args)], capture_output=True, text=True, encoding="utf-8", errors="replace")
    if r.returncode != 0:
        raise RuntimeError(f"wt-unpack {' '.join(map(str, args))} failed:\n{r.stderr[-2000:]}")
    return r.stdout


def load_json(path):
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def as_list(v):
    return v if isinstance(v, list) else ([] if v is None else [v])


def iter_dicts(o):
    if isinstance(o, dict):
        yield o
        for v in o.values():
            yield from iter_dicts(v)
    elif isinstance(o, list):
        for v in o:
            yield from iter_dicts(v)


# ---------------------------------------------------------------- lang

def load_lang(lang_dir):
    names = {}
    for fn in ("units.csv", "units_weaponry.csv"):
        p = lang_dir / "lang" / fn
        if not p.exists():
            continue
        with open(p, encoding="utf-8-sig", newline="") as f:
            for row in csv.reader(f, delimiter=";", quotechar='"'):
                if len(row) > 1 and row[0] and not row[0].startswith("<"):
                    names.setdefault(row[0].lower(), row[1])
    return names


# ---------------------------------------------------------------- tanks

def first(v):
    # a BLK key repeated in one block becomes a JSON list; Dagor's getReal() returns the first one
    return v[0] if isinstance(v, list) and v else v


def shell_from_bullet(b, ammo_key):
    # Cx may be missing (many WW2 guns): the game then uses a built-in default we cannot read; see estimate_cx()
    if not isinstance(b, dict) or "speed" not in b or "mass" not in b:
        return None
    b = {k: first(b[k]) for k in ("speed", "mass", "Cx", "caliber", "ballisticCaliber", "maxDistance", "bulletName", "bulletType") if k in b}
    cal = b.get("ballisticCaliber") or b.get("caliber")
    if not isinstance(cal, (int, float)) or not cal or b["mass"] <= 0 or b["speed"] <= 0:
        return None
    return {
        "ammo": ammo_key,
        "id": b.get("bulletName", ammo_key),
        "type": b.get("bulletType", "?"),
        "v0": round(float(b["speed"]), 3),
        "m": float(b["mass"]),
        "cal": float(cal),  # drag reference diameter (ballisticCaliber for sub-calibre rounds)
        "cx": float(b["Cx"]) if isinstance(b.get("Cx"), (int, float)) else None,
        "maxDist": float(b.get("maxDistance", 0) or 0),
    }


def estimate_cx(tanks):
    """Fill shells without Cx with the median Cx of shells of the same bulletType (else the same type family,
    e.g. "apcbc", else all shells) and mark them cxEst. The game's own default is not in the data files."""
    by_type, by_family, everything = {}, {}, []
    shells = [s for t in tanks for g in t["guns"] for s in g["shells"]]
    for s in shells:
        if s["cx"] is not None:
            by_type.setdefault(s["type"], []).append(s["cx"])
            by_family.setdefault(s["type"].split("_")[0], []).append(s["cx"])
            everything.append(s["cx"])
    for s in shells:
        if s["cx"] is None:
            pool = by_type.get(s["type"]) or by_family.get(s["type"].split("_")[0]) or everything
            s["cx"], s["cxEst"] = round(float(np.median(pool)), 4), True


def shells_of_weapon(w):
    """Default bullet plus every named ammo block that holds a bullet (or a belt of them)."""
    out, seen = [], set()
    groups = [("default", w.get("bullet"))] + [(k, v.get("bullet")) for k, v in w.items() if isinstance(v, dict) and "bullet" in v]
    for key, bl in groups:
        for b in as_list(bl):
            s = shell_from_bullet(b, key)
            if s and (s["id"], s["v0"], s["cx"]) not in seen and not b.get("rocket"):
                seen.add((s["id"], s["v0"], s["cx"]))
                out.append(s)
    return out


def stock_spread(char):
    """Per module name, how much it widens a gun's spread while not researched: (horizontal, vertical) factors
    (char.vromfs config/modifications: effects of modules with invertEnableLogic, e.g. Adjustment of Fire)."""
    mods = (load_json(char / "config" / "modifications.blk.json").get("modifications") or {}) if (char / "config" / "modifications.blk.json").exists() else {}
    out = {}
    for name, m in mods.items():
        e = m.get("effects") if isinstance(m, dict) and m.get("invertEnableLogic") in (True, "yes") else None
        if isinstance(e, dict) and ("mulMaxDeltaAngle" in e or "mulMaxDeltaAngleVertical" in e):
            out[name] = (float(e.get("mulMaxDeltaAngle", 1.0)), float(e.get("mulMaxDeltaAngleVertical", 1.0)))
    return out


def extract_tanks(aces, names, spread_mods=None):
    tank_dir = aces / "gamedata" / "units" / "tankmodels"
    weapon_cache = {}
    tanks = []
    for f in sorted(tank_dir.glob("*.blk.json")):
        tid = f.name[: -len(".blk.json")]
        title = names.get(tid + "_shop") or names.get(tid + "_0")
        if not title:
            continue  # AI/dummy units have no shop name
        t = load_json(f)
        sources = [t]
        for d in iter_dicts(t.get("weapon_presets", {})):
            p = d.get("blk")
            if isinstance(p, str) and p.lower().endswith(".blk"):
                pp = aces / (p.lower() + ".json")
                if pp.exists():
                    sources.append(load_json(pp))
        cannons, others, seen_blk = [], [], set()
        for src in sources:
            for d in iter_dicts(src):
                p = d.get("blk")
                if not isinstance(p, str) or "groundmodels_weapons" not in p.lower() or p.lower() in seen_blk:
                    continue
                seen_blk.add(p.lower())
                wp = aces / (p.lower() + ".json")
                if wp not in weapon_cache:
                    weapon_cache[wp] = load_json(wp) if wp.exists() else None
                w = weapon_cache[wp]
                if not w:
                    continue
                shells = shells_of_weapon(w)
                if not shells:
                    continue
                pitch = (d.get("limits") or {}).get("pitch") or [-10.0, 20.0]
                for s in shells:
                    s["name"] = names.get(s["id"].lower(), s["id"])
                gun = {"weapon": Path(p).stem, "pitch": [float(pitch[0]), float(pitch[1])], "shells": shells}
                h = w.get("maxDeltaAngle")
                if isinstance(h, (int, float)):
                    # the largest angle a shot leaves the bore line by, in degrees (all modules researched)
                    v = w.get("maxDeltaAngleVertical")
                    gun["disp"] = [float(h), float(v) if isinstance(v, (int, float)) else float(h)]
                (cannons if w.get("cannon") else others).append(gun)
        # machine-gun-class weapons (autocannons of SPAA, 20 mm light tanks) only for vehicles without a cannon
        guns = cannons or others
        if guns:
            model = t.get("model")
            rec = {"id": tid, "name": title, "model": model.lower() if isinstance(model, str) else None, "guns": guns}
            # the spread factors of the modules this tank has, while none is researched
            stock = [1.0, 1.0]
            for mod in (t.get("modifications") or {}):
                f = (spread_mods or {}).get(mod)
                if f:
                    stock = [stock[0] * f[0], stock[1] * f[1]]
            if stock != [1.0, 1.0]:
                rec["stock"] = [round(stock[0], 4), round(stock[1], 4)]
            tanks.append(rec)
    estimate_cx(tanks)
    return tanks


# ---------------------------------------------------------------- missions / areas

def resolve_mission(path, mis_root, depth=0):
    """Areas and units of a mission merged with everything it imports (imports first, own entries win)."""
    d = load_json(path)
    areas, units = {}, []
    trig = d.get("triggers") or {}
    # The ground battle area: the area of a missionBattleArea action for ground units with killOutOfBattleArea
    # (leaving it starts the countdown; the kill-area actions are detection areas without that flag).
    # Its `area` is a name or "@variable"; _vars holds every string value a varSetString gives each variable.
    d["_battle_refs"] = [dd.get("area") or dd.get("target") for t in iter_dicts(trig)
                         for k in ("missionBattleArea", "missionMarkAsBattleArea") for dd in as_list(t.get(k))
                         if isinstance(dd, dict) and dd.get("ground") in (True, "yes") and dd.get("killOutOfBattleArea") in (True, "yes")]
    # kill areas (prohibited zones): missionBattleArea actions with killArea for ground units
    d["_kill_refs"] = [dd.get("area") or dd.get("target") for t in iter_dicts(trig) for dd in as_list(t.get("missionBattleArea"))
                       if isinstance(dd, dict) and dd.get("ground") in (True, "yes") and dd.get("killArea") in (True, "yes")]
    # variables: defaults from the `variables` block (battlearea_exclude_template sets its squad name there)
    d["_vars"] = {k: [v] for k, v in (d.get("variables") or {}).items() if isinstance(v, str)}
    for t in iter_dicts(trig):
        for dd in as_list(t.get("varSetString")):
            if isinstance(dd, dict) and isinstance(dd.get("value"), str):
                for v in as_list(dd.get("var")):
                    d["_vars"].setdefault(v, []).append(dd["value"])
    if depth < 6:
        for rec in as_list((d.get("imports") or {}).get("import_record")):
            fp = rec.get("file") if isinstance(rec, dict) else None
            if fp:
                ip = mis_root / (fp.lower() + ".json")
                if ip.exists():
                    a, u, sub = resolve_mission(ip, mis_root, depth + 1)
                    rr = rec.get("rankRange")
                    if isinstance(rr, list) and len(rr) == 2:
                        # loaded only for battles of these ranks (low or high rank airfields, heli pads)
                        a = {k: {**x, "_rank": rank_and(x.get("_rank"), rr)} for k, x in a.items()}
                        u = [(k, {**x, "_rank": rank_and(x.get("_rank"), rr)}) for k, x in u]
                    areas.update(a)
                    units += u
                    d["_battle_refs"] += sub["_battle_refs"]
                    d["_kill_refs"] += sub["_kill_refs"]
                    for v, vals in sub["_vars"].items():
                        d["_vars"].setdefault(v, []).extend(vals)
    for k, v in (d.get("areas") or {}).items():
        for a in as_list(v):
            if isinstance(a, dict) and "tm" in a:
                areas[k] = a
    for kind, v in (d.get("units") or {}).items():
        for u in as_list(v):
            if isinstance(u, dict):
                units.append((kind, u))
    return areas, units, d


def rank_and(a, b):
    """The ranks two rankRanges both allow ([lo, hi], None = any)."""
    if not a or not b:
        return a or [int(b[0]), int(b[1])]
    return [max(int(a[0]), int(b[0])), min(int(a[1]), int(b[1]))]


def team_of(name):
    n = name.lower()
    return 1 if n.startswith("t1") or "_t1_" in n else 2 if n.startswith("t2") or "_t2_" in n else 0


RANK_MAX = 99  # rankRange without an upper end (the game's go to 50)
AIR_SPAWN = re.compile(r"t[12]_(air|bombers|heavy_bombers|heli)_spawn", re.I)
MODE_SUFFIX = {"arcade": "arcade", "realistic": "realistic", "simulator": "hardcore"}


def area_mode(name):
    """Game mode an area variant is for, from the suffix the mode templates append (simulator = _hardcore)."""
    n = name.lower()
    return next((m for m, s in MODE_SUFFIX.items() if n.endswith("_" + s)), None)


def mission_areas(path, mis_root):
    areas, units, d = resolve_mission(path, mis_root)
    ms = d.get("mission_settings") or {}
    picked = {}  # name -> kind

    def add(name, kind):
        if name in areas and name not in picked:
            picked[name] = kind

    # 1. what the ground missionBattleArea trigger uses (killOutOfBattleArea: the countdown border). A "@var"
    #    area is a base name the template extends by game mode (conq_battle_area + _arcade/_realistic/_hardcore),
    #    so every area named base or base_<suffix> is kept, tagged by mode (see area_mode)
    # area_squad units by name: an action's area can be a squad (battlearea_exclude_squad), meaning all its members
    squads = {str(u.get("name", "")): as_list((u.get("props") or {}).get("squad_members"))
              for kind, u in units if kind == "area_squad"}

    def add_refs(refs, kind):
        for ref in refs:
            if not isinstance(ref, str):
                continue
            bases = d["_vars"].get(ref[1:], []) if ref.startswith("@") else [ref]
            for b in bases:
                for m in squads.get(b, []):
                    add(m, kind)
                for n in areas:
                    if n == b or n.startswith(b + "_"):
                        add(n, kind)
    add_refs(d["_kill_refs"], "kill")  # first: kill areas are named like t1_killarea, never like battle areas
    add_refs(d["_battle_refs"], "battle")
    # 2. the briefing's battle area (what the briefing map draws), when no trigger names one
    brief = ms.get("briefing") or {}
    for dd in iter_dicts(brief):
        if "battleArea" in dd and not any(k == "battle" for k in picked.values()):
            for ba in as_list(dd["battleArea"]):
                if isinstance(ba, dict):
                    add(ba.get("target"), "battle")
        it = dd.get("icontype")
        if isinstance(it, str) and "zone" in it.lower():
            add(dd.get("target"), "capture")
    # 3. an area named like a battle area (some templates build the name in ways the variables above miss)
    if not any(k == "battle" for k in picked.values()):
        for n in areas:
            if "battle_area" in n.lower():
                add(n, "battle")
    for kind, u in units:
        if kind != "area_squad":
            continue
        n = str(u.get("name", "")).lower()
        k = "kill" if "killarea" in n else "spawn" if "spawn" in n and "tank" in n else None
        if k:
            for m in as_list((u.get("props") or {}).get("squad_members")):
                add(m, k)
    # 4. where aircraft and helicopters appear in the air (spheres; the suffix is the air mode, not the ground one)
    for n in areas:
        if AIR_SPAWN.match(n):
            add(n, "airspawn")
    out = []
    for name, kind in picked.items():
        a = areas[name]
        rec = {"name": name, "kind": kind, "team": team_of(name), "type": a.get("type", "?"),
               "tm": [[round(x, 3) for x in row] for row in a["tm"]]}
        if a.get("_rank"):
            rec["rank"] = a["_rank"]
        mode = area_mode(name) if kind != "airspawn" else None
        if mode:
            rec["mode"], rec["group"] = mode, name[: name.lower().rindex("_" + MODE_SUFFIX[mode])]
        out.append(rec)
    mission = ms.get("mission") or {}
    # the mission's object groups (airfields, heli pads, props): placed composites, see group_placements
    groups = [(str(u.get("unit_class", "")), u["tm"], u.get("_rank") or [0, RANK_MAX]) for kind, u in units if kind == "objectGroups" and isinstance(u.get("tm"), list)]
    # anti-aircraft guns (airfield and heli pad defences): [name, unit class, x, z]; the game sets them on the ground
    aa = [[str(u.get("name", "")), str(u.get("unit_class", "")), round(u["tm"][3][0], 1), round(u["tm"][3][2], 1)] + (u.get("_rank") or [0, RANK_MAX])
          for kind, u in units if kind == "air_defence" and isinstance(u.get("tm"), list)]
    return str(mission.get("level", "")), str(mission.get("type", "")), out, groups, aa


def extract_missions(mis):
    root = mis / "gamedata" / "missions" / "cta" / "tanks"
    by_level = {}
    for f in sorted(root.rglob("*.blk.json")):
        if f.name.startswith("template_"):
            continue  # imported building blocks; some still name another map's level
        try:
            level, mtype, areas, groups, aa = mission_areas(f, mis)
        except (KeyError, TypeError, ValueError, AttributeError) as e:
            print(f"  skip mission {f.name}: {e}", file=sys.stderr)
            continue
        if not level.startswith("levels/") or not areas:
            continue
        lv = Path(level).stem
        by_level.setdefault(lv, []).append({"name": f.name[: -len(".blk.json")], "type": mtype, "areas": areas, "aa": aa, "_groups": groups})
    return by_level


def load_object_groups(root):
    """gamedata/objectgroups/*.blk as JSON, by lower-case name."""
    return {f.name[: -len(".blk.json")].lower(): load_json(f) for f in (root / "gamedata" / "objectgroups").glob("*.blk.json")}


def snap_aa(missions, ogs):
    """Airfield AA guns stand in the airfield object group's air_def slots, not at the mission's spots (those are
    30-700 m off any slot): each gun moves to the nearest free slot of an airfield in the same battles; the
    airfield's slots left free stay as spare AA positions (class ""). Heli pad guns keep their spots (pads have no slots)."""
    for m in missions:
        slots = []  # [x, z, lo, hi, airfield]
        for g, (cls, tm, rank) in enumerate(m["_groups"]):
            for n in as_list((ogs.get(cls.lower()) or {}).get("node")):
                if isinstance(n, dict) and n.get("name") == "#position" and n.get("type") == "air_def" and isinstance(n.get("tm"), list):
                    p = compose(tm, n["tm"])[3]
                    slots.append([round(p[0], 1), round(p[2], 1), int(rank[0]), int(rank[1]), g])
        pairs = sorted((math.hypot(a[2] - sl[0], a[3] - sl[1]), i, j) for i, a in enumerate(m["aa"]) for j, sl in enumerate(slots)
                       if "helipad" not in (a[0] + a[1]).lower() and a[4] <= sl[3] and sl[2] <= a[5])
        gun, used = {}, {}
        for d, i, j in pairs:
            if d < 1500 and i not in gun and j not in used:
                gun[i], used[j] = j, i
        for i, j in gun.items():
            m["aa"][i][2:4] = slots[j][:2]
        fields = {slots[j][4] for j in used}
        m["aa"] += [["", "", sl[0], sl[1], sl[2], sl[3]] for j, sl in enumerate(slots) if j not in used and sl[4] in fields]


def compose(g, n):
    """Dagor tm (rows: x, y, z axis, position) of n placed inside g."""
    axes = [[sum(n[i][k] * g[k][j] for k in range(3)) for j in range(3)] for i in range(3)]
    return axes + [[g[3][j] + sum(n[3][k] * g[k][j] for k in range(3)) for j in range(3)]]


def expand_group(cls, tm, ogs, depth=0):
    """(kind, name, world tm) of every node of an object group, nested groups expanded: kind "model" (a RendInst
    or DynModel), "slab" (the textured ground the game lays under airfields, heli pads and trenches: ^^ in the
    node's flag prefix, or * on a runway's surface mesh) or "spawn" (name "heli" or "plane": a parking spot units spawn on, from the airfield block).
    Node names carry flag prefixes (# $ % > * +); the exporter skips names that are no model (lights, boxes)."""
    d = ogs.get(cls.lower())
    if d is None or depth > 4:
        return [("model", cls, tm)]
    out, nodes = [], []
    for n in as_list(d.get("node")):
        raw = str(n.get("name", "")) if isinstance(n, dict) else ""
        name = re.sub(r"^[^A-Za-z0-9_]+", "", raw)
        if name and isinstance(n.get("tm"), list):
            flags = raw[: len(raw) - len(name)]
            # ^^: a decal slab; * with a surface name: the runway's own decal mesh (a RendInst; * ramps are objects)
            surface = "^^" in flags or ("*" in flags and ("_mesh" in name or name.endswith("_decals")))
            nodes.append((surface, name, compose(tm, n["tm"])))
    has_slab = any(slab for slab, _, _ in nodes)
    for af in as_list(d.get("airfield")):
        if not isinstance(af, dict):
            continue
        spawn = af.get("spawn") or []
        for p in (spawn if spawn and isinstance(spawn[0], list) else [spawn] if spawn else []):
            out.append(("spawn", "heli" if "helipad" in cls.lower() else "plane", compose(tm, [[1, 0, 0], [0, 1, 0], [0, 0, 1], p])))
        if has_slab:
            continue
        # no slab: the runway or pad surface as a scaled concrete unit square, from start, end and width
        try:
            (sx, sy, sz), (ex, ey, ez), w = af["start"], af["end"], float(af["width"])
        except (KeyError, TypeError, ValueError):
            continue
        dx, dz = ex - sx, ez - sz
        ln = (dx * dx + dz * dz) ** 0.5 or 1.0
        local = [[dx, 0, dz], [0, 1, 0], [-dz / ln * w, 0, dx / ln * w], [(sx + ex) / 2, (sy + ey) / 2 + RUNWAY_LIFT, (sz + ez) / 2]]
        out.append(("model", "__runway__", compose(tm, local)))
    for slab, name, wtm in nodes:
        if slab:
            out.append(("slab", name, wtm))
        else:
            out += expand_group(name, wtm, ogs, depth + 1) if name.lower() in ogs else [("model", name, wtm)]
    return out


def group_placements(missions, ogs):
    """Placements file lines (tag = placement id), per mission the [id, lowest rank, highest rank] it places, the ground slabs
    [(id, model, tm)], the spawn spots [[id, "heli" | "plane", x, y, z]] and per id the height of the group's
    origin (the game sets groups on the ground; their own heights are often far off it): missions of one level
    share most of their groups, so each distinct (group, tm) is exported once."""
    ids, lines, per, slabs, spawns, origins = {}, [], {}, [], [], []
    for m in missions:
        for cls, tm, rank in m["_groups"]:
            key = (cls.lower(), tuple(round(x, 2) for row in tm for x in row))
            if key not in ids:
                ids[key] = i = len(ids)
                origins.append(round(tm[3][1], 2))
                for kind, name, wtm in expand_group(cls, tm, ogs):
                    if kind == "model":
                        lines.append(f"{i} {name} " + " ".join(f"{x:.4f}" for row in wtm for x in row))
                    elif kind == "slab":
                        slabs.append((i, name, wtm))
                    else:
                        spawns.append([i, name] + [round(x, 2) for x in wtm[3]])
            per.setdefault(m["name"], []).append([ids[key], int(rank[0]), int(rank[1])])
    return lines, per, slabs, spawns, origins


def extract_slabs(game, names, tmp, out_dir):
    """data/slabs/<model>.js per ground slab model (shared by every level): its mesh and its textures as JPEG.
    Returns the vertices (n x 3, model frame) per model that exported, for the tiles around its placements."""
    out_dir.mkdir(parents=True, exist_ok=True)
    jobs = tmp / "slab_jobs.txt"
    jobs.write_text("".join(f"slab {n} {tmp / ('slab_' + n)}\n" for n in sorted(names)))
    r = subprocess.run([str(OBJECTS_EXE), str(game), str(jobs)], capture_output=True, text=True, encoding="utf-8", errors="replace")
    print(r.stdout, end="")
    verts = {}
    for n in sorted(names):
        src = tmp / f"slab_{n}.slab"
        if not src.exists():
            continue
        raw = src.read_bytes()
        parts, = struct.unpack_from("<I", raw)
        o, pts, texs = 4, [], set()
        for _ in range(parts):
            t, _, nv = struct.unpack_from("<III", raw, o)
            texs.add(t)
            pts.append(np.frombuffer(raw, "<f4", nv * 5, o + 12).reshape(nv, 5)[:, :3])
            o += 12 + nv * 20
            ni, = struct.unpack_from("<I", raw, o)
            o += 4 + ni * 4
        tex, alpha = [], []
        for k in range(len(texs)):
            b = (tmp / f"slab_{n}.{k}.rgba").read_bytes()
            w, h = struct.unpack_from("<II", b)
            im = Image.frombytes("RGBA", (w, h), b[8:])
            im.thumbnail((SLAB_PX, SLAB_PX), Image.LANCZOS)
            # the alpha is the decal's mask: concrete and tracks opaque, the ground around them shows through
            for ch, out in ((im.convert("RGB"), tex), (im.getchannel("A"), alpha)):
                buf = io.BytesIO()
                ch.save(buf, "JPEG", quality=85)
                out.append("data:image/jpeg;base64," + base64.b64encode(buf.getvalue()).decode("ascii"))
        blob = base64.b64encode(zlib.compress(raw, 6)).decode("ascii")
        (out_dir / f"{n}.js").write_text(f'WTV_SLAB("{n}",' + json.dumps({"mesh": blob, "tex": tex, "alpha": alpha}, separators=(",", ":")) + ");\n", encoding="ascii")
        verts[n] = np.concatenate(pts).astype(np.float64)
    return verts


def slab_tiles(exe, game, level, slabs, verts, tmp):
    """Per ground slab placement: its model, tm and the landmesh around it every SLAB_TILE m (the whole level has
    one height every FAR_CELL m, which misses the landmesh's finer triangles around airfields and heli pads)."""
    png = tmp / f"{level}.tile.png"
    run(exe, "level", Path(game) / "levels" / f"{level}.bin", "--terrain", "landmesh", "--cell", str(SLAB_TILE), "--heightmap", png)
    meta = load_json(png.with_suffix(".json"))
    Image.MAX_IMAGE_PIXELS = None  # our own export: a 130 km level is 16k x 16k at SLAB_TILE
    with Image.open(png) as im:
        h = np.asarray(im)  # 16-bit, converted per tile below (the whole level in float64 would take gigabytes)
    out = []
    for tag, name, tm in slabs:
        if name not in verts:
            continue
        a = np.asarray(tm, dtype=np.float64)
        wx, wz = verts[name] @ a[:3, 0] + a[3, 0], verts[name] @ a[:3, 2] + a[3, 2]  # the rows of tm are its axes
        i0 = max(0, int((wx.min() - SLAB_MARGIN - meta["origin_x"]) // SLAB_TILE))
        j0 = max(0, int((wz.min() - SLAB_MARGIN - meta["origin_z"]) // SLAB_TILE))
        i1 = min(h.shape[1] - 1, int((wx.max() + SLAB_MARGIN - meta["origin_x"]) // SLAB_TILE) + 1)
        j1 = min(h.shape[0] - 1, int((wz.max() + SLAB_MARGIN - meta["origin_z"]) // SLAB_TILE) + 1)
        t = meta["height_min"] + (meta["height_max"] - meta["height_min"]) * h[j0:j1 + 1, i0:i1 + 1].astype(np.float64) / 65535
        lo, hi = float(t.min()), float(t.max()) + 0.01
        q = np.round((t - lo) / (hi - lo) * 65535).astype("<u2")
        out.append({"tag": tag, "model": name, "tm": [round(x, 4) for row in tm for x in row],
                    "tile": {"x0": meta["origin_x"] + i0 * SLAB_TILE, "z0": meta["origin_z"] + j0 * SLAB_TILE, "cell": SLAB_TILE,
                             "w": int(t.shape[1]), "h": int(t.shape[0]), "lo": round(lo, 3), "hi": round(hi, 3),
                             "q": base64.b64encode(zlib.compress(q.tobytes(), 6)).decode("ascii")}})
    png.unlink(missing_ok=True)
    return out


# ---------------------------------------------------------------- maps

def level_settings(exe, game, level):
    """levels/<level>.blk as text (from aces.vromfs.bin), or "" when the level has none."""
    try:
        return run(exe, "cat", f"{Path(game) / 'aces.vromfs.bin'}:levels/{level}.blk")
    except RuntimeError:
        return ""


def map_bounds(text):
    """[x0, z0, x1, z1] of the tank map from the level settings (tankMapCoord0/1, else mapCoord0/1), or None.
    mapCoord is the air map and spans tens of km on tank levels, so tankMapCoord comes first."""
    for key in ("tankMapCoord", "mapCoord"):
        pts = [re.search(rf"^\s*{key}{i}:p2\s*=\s*([-\d.eE+]+)\s*,\s*([-\d.eE+]+)", text, re.M) for i in (0, 1)]
        if all(pts):
            (x0, z0), (x1, z1) = [(float(p.group(1)), float(p.group(2))) for p in pts]
            return [min(x0, x1), min(z0, z1), max(x0, x1), max(z0, z1)]
    return None


def water_level(text, hmin):
    """The level's water plane height (top-level water_level:r), or None when it has none or no ground lies below it."""
    m = re.search(r"^water_level:r\s*=\s*([-\d.eE+]+)", text, re.M)
    return float(m.group(1)) if m and float(m.group(1)) > hmin else None


def extract_map(exe, game, level, missions, tmp, out_dir, far=False):
    """maps/<level>.js from the HM2 heightmap; far: maps/<level>.far.js from the whole landmesh instead."""
    name = f"{level}.far" if far else level
    png = tmp / f"{name}.png"
    extra = ["--terrain", "landmesh", "--cell", str(FAR_CELL)] if far else []
    run(exe, "level", Path(game) / "levels" / f"{level}.bin", *extra, "--heightmap", png)
    meta = load_json(png.with_suffix(".json"))
    with Image.open(png) as im:
        h = np.asarray(im).astype(np.uint16)
    assert h.shape == (meta["height"], meta["width"]), (h.shape, meta)
    cell = float(meta["cell_size"])
    stride = 1
    while max(h.shape) // stride > MAX_GRID:
        stride *= 2
    if stride > 1:
        h = np.ascontiguousarray(h[::stride, ::stride])  # shortcut: strided, not filtered; fine for >8 km landmesh maps
        cell *= stride
    blob = base64.b64encode(zlib.compress(h.astype("<u2").tobytes(), 6)).decode("ascii")
    settings = level_settings(exe, game, level)
    info = {
        "name": name, "bounds": map_bounds(settings), "width": int(h.shape[1]), "height": int(h.shape[0]), "cell": cell,
        "ox": float(meta["origin_x"]), "oz": float(meta["origin_z"]),
        "hmin": float(meta["height_min"]), "hmax": float(meta["height_max"]),
        "source": meta.get("source", "?"), "missions": missions,
        "water": water_level(settings, float(meta["height_min"])),
    }
    js = "WTV_MAP(" + json.dumps(info, separators=(",", ":"))[:-1] + ',"h":"' + blob + '"});\n'
    (out_dir / f"{name}.js").write_text(js, encoding="ascii")
    return {k: info[k] for k in ("name", "width", "height", "cell", "ox", "oz", "source")} | {"missions": len(missions), "level": level}


def keep_objects(maps, out_dir):
    """--skip-objects: keep the object and ground files of an earlier run (same heightmap grid)."""
    for m in maps:
        m["objects"] = (out_dir / f"{m['name']}.obj.js").exists()
        m["ground"] = (out_dir / f"{m['name']}.tex.js").exists()


OBJECTS_EXE = Path(__file__).resolve().parent / "objects" / "target" / "release" / "wtv-objects.exe"


def extract_objects(game, maps, tmp, out_dir):
    """Objects, trees and ground colours via the Rust exporter in objects/:
    <level>.obj.js (objects, trees, collision grids) and <level>.tex.js (ground colour JPEG) per map."""
    if not OBJECTS_EXE.exists():
        print(f"no objects or ground colours: {OBJECTS_EXE} is not built (see README)", file=sys.stderr)
        return
    print("exporting objects, trees and ground colours ...")
    jobs = tmp / "objects_jobs.txt"
    # shortcut: the exporter splits job lines on whitespace, so tmp must not contain spaces (tempfile's dir has none)
    base = {m["name"]: tmp / m["name"].replace(".", "_") for m in maps}  # the exporter replaces the extension
    jobs.write_text("".join(f"{m['level']} {base[m['name']]} {m['ox']} {m['oz']} {m['cell']} {m['width']} {m['height']}{' ' + str(m['placements']) if 'placements' in m else ''}\n" for m in maps))
    r = subprocess.run([str(OBJECTS_EXE), str(game), str(jobs)], capture_output=True, text=True, encoding="utf-8", errors="replace")
    print(r.stdout, end="")
    if r.returncode:
        print(r.stderr.strip().splitlines()[-1] if r.stderr.strip() else "wtv-objects failed", file=sys.stderr)
    for m in maps:
        src = base[m["name"]].with_suffix(".obj")
        if src.exists():
            blob = base64.b64encode(zlib.compress(src.read_bytes(), 6)).decode("ascii")
            extra = "," + json.dumps({k: m[k] for k in ("missions", "slabs", "spawns", "origins")}, separators=(",", ":")) if "placements" in m else ""
            (out_dir / f"{m['name']}.obj.js").write_text(f'WTV_OBJ("{m["name"]}","{blob}"{extra});\n', encoding="ascii")
            m["objects"] = True
        ground = base[m["name"]].with_suffix(".ground")
        if ground.exists():
            raw = ground.read_bytes()
            w, h = struct.unpack("<II", raw[:8])
            o = 8 + w * h * 4

            def data_url(im, fmt, **kw):
                buf = io.BytesIO()
                im.save(buf, fmt, **kw)
                return f"data:image/{fmt.lower()};base64,{base64.b64encode(buf.getvalue()).decode('ascii')}"
            # row 0 is the smallest z; the page draws +Z up, like the heightmap shading
            colour = data_url(ImageOps.flip(Image.frombytes("RGBA", (w, h), raw[8:o]).convert("RGB")), "JPEG", quality=85)
            detail = None
            if len(raw) > o:  # detail index per texel + the detail textures, side by side in one strip
                idx = ImageOps.flip(Image.frombytes("L", (w, h), raw[o:o + w * h]))
                o += w * h
                n, = struct.unpack_from("<I", raw, o)
                o += 4
                tiles, strip = [], None
                for i in range(n):
                    t, side = struct.unpack_from("<fI", raw, o)
                    strip = strip or Image.new("L", (side * n, side))
                    strip.paste(Image.frombytes("L", (side, side), raw[o + 8:o + 8 + side * side]), (side * i, 0))
                    tiles.append(round(t, 2))
                    o += 8 + side * side
                if n:
                    detail = {"idx": data_url(idx, "PNG", optimize=True), "strip": data_url(strip, "PNG", optimize=True), "tiles": tiles}
            (out_dir / f"{m['name']}.tex.js").write_text(
                f'WTV_TEX("{m["name"]}","{colour}",{json.dumps(detail)});\n', encoding="ascii")
            m["ground"] = True


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--game", default=DEFAULT_GAME)
    ap.add_argument("--wt-unpack")
    ap.add_argument("--levels", help="comma-separated level names (default: every level used by a tank mission)")
    ap.add_argument("--out", default=str(ROOT / "data"))
    ap.add_argument("--keep-tmp", action="store_true")
    ap.add_argument("--skip-objects", action="store_true", help="keep the objects/trees/ground files of an earlier run")
    ap.add_argument("--far", action="store_true", help="also export each whole level from its landmesh (Max build)")
    a = ap.parse_args()

    exe = find_wt_unpack(a.wt_unpack)
    game = Path(a.game)
    out = Path(a.out)
    (out / "maps").mkdir(parents=True, exist_ok=True)
    tmp = Path(tempfile.mkdtemp(prefix="wtv_"))
    try:
        print("unpacking archives ...")
        run(exe, "extract", game / "aces.vromfs.bin", "-o", tmp / "aces", "--blk", "json", "--filter", "tankmodels")
        run(exe, "extract", game / "aces.vromfs.bin", "-o", tmp / "aces", "--blk", "json", "--filter", "groundmodels_weapons")
        run(exe, "extract", game / "mis.vromfs.bin", "-o", tmp / "mis", "--blk", "json", "--filter", "gamedata/missions")
        run(exe, "extract", game / "lang.vromfs.bin", "-o", tmp / "lang", "--filter", "lang/units")
        run(exe, "extract", game / "char.vromfs.bin", "-o", tmp / "char", "--blk", "json", "--filter", "config/modifications")
        if a.far:
            run(exe, "extract", game / "aces.vromfs.bin", "-o", tmp / "og", "--blk", "json", "--filter", "gamedata/objectgroups")

        names = load_lang(tmp / "lang")
        tanks = extract_tanks(tmp / "aces", names, stock_spread(tmp / "char"))
        print(f"tanks: {len(tanks)}")
        by_level = extract_missions(tmp / "mis")
        if a.far:
            ogs = load_object_groups(tmp / "og")
            for ms in by_level.values():
                snap_aa(ms, ogs)
        # "water" is the empty ocean level that many tank test/event missions point at, not a battlefield
        levels = a.levels.split(",") if a.levels else sorted(set(by_level) - {"water"})
        groups_of = {lv: [{"name": m["name"], "_groups": m.pop("_groups")} for m in ms] for lv, ms in by_level.items()}
        maps = []
        for i, lv in enumerate(levels, 1):
            try:
                m = extract_map(exe, game, lv, by_level.get(lv, []), tmp, out / "maps")
                maps.append(m)
                print(f"[{i}/{len(levels)}] {lv}: {m['width']}x{m['height']} @ {m['cell']} m ({m['source']}), {m['missions']} missions")
            except (RuntimeError, OSError, AssertionError, KeyError) as e:
                print(f"[{i}/{len(levels)}] {lv}: FAILED {str(e).splitlines()[0]}", file=sys.stderr)
        fars = []
        groups = []
        if a.far:
            placed = {m["level"]: group_placements(groups_of.get(m["level"], []), ogs) for m in maps}
            verts = extract_slabs(game, {n for _, _, sl, _, _ in placed.values() for _, n, _ in sl}, tmp, out / "slabs")
            for m in maps:
                lines, per, slabs, spawns, origins = placed[m["level"]]
                if lines:
                    pf = tmp / f"{m['level']}_placements.txt"
                    pf.write_text("\n".join(lines) + "\n")
                    groups.append({"name": f"{m['level']}.groups", "level": m["level"], "ox": 0, "oz": 0, "cell": 1, "width": 0, "height": 0,
                                   "placements": pf, "missions": per, "spawns": spawns, "origins": origins,
                                   "slabs": slab_tiles(exe, game, m["level"], slabs, verts, tmp) if slabs else []})
            for i, m in enumerate(maps, 1):
                try:
                    f = extract_map(exe, game, m["level"], [], tmp, out / "maps", far=True)
                    fars.append(f)
                    print(f"[{i}/{len(maps)}] {f['name']}: {f['width']}x{f['height']} @ {f['cell']} m ({f['source']})")
                except (RuntimeError, OSError, AssertionError, KeyError) as e:
                    print(f"[{i}/{len(maps)}] {m['level']}.far: FAILED {str(e).splitlines()[0]}", file=sys.stderr)
        if a.skip_objects:
            keep_objects(maps + fars, out / "maps")
            extract_objects(game, groups, tmp, out / "maps")  # cheap, and its format follows the slabs and spawns
        else:
            extract_objects(game, maps + fars + groups, tmp, out / "maps")
        by_name = {m["name"]: m for m in maps}
        for f in fars:
            if f["name"].endswith(".far"):
                by_name[f["level"]]["far"] = {k: v for k, v in f.items() if k != "level"}  # the app finds the whole-level files here
        for f in groups:
            if f.get("objects") and "far" in by_name[f["level"]]:
                by_name[f["level"]]["far"]["groups"] = True
        index = {"maps": maps, "tanks": tanks}
        (out / "index.js").write_text("WTV_INDEX(" + json.dumps(index, separators=(",", ":")) + ");\n", encoding="utf-8")
        print(f"wrote {out / 'index.js'} ({len(maps)} maps)")
    finally:
        if a.keep_tmp:
            print(f"kept {tmp}")
        else:
            shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    main()
