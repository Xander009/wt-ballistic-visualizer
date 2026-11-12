#!/usr/bin/env python3
"""Convert the extracted tank glTF models (../tanks/<folder>/<model>.gltf + .bin) into small files for ../index.html.

Writes ../data/tanks/<model>.js for every model used by a tank in ../data/index.js, plus t_72a (target B).
Each model is merged into one mesh, simplified by vertex clustering to at most --tris triangles and stored as
WTV_TANK("<model>", "<base64 zlib>"): magic "WTVK", u32 version 1, u32 vertex count, u32 triangle count,
int16 x y z per vertex in mm (y shifted so the lowest point is 0; +X = front, +Y = up), uint16 indices.
The glTF files carry no textures, so the page draws the models in one colour per side.

Models missing from ../tanks are replaced by a variant of the same vehicle when there is one (see stand_in).

Usage:  python tank_models.py [--tanks ../tanks] [--tris 12000] [--force]
Needs:  numpy.
"""
import argparse
import base64
import difflib
import json
import struct
import sys
import zlib
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parent.parent
TARGET_B = "t_72a"
DTYPE = {5120: np.int8, 5121: np.uint8, 5122: np.int16, 5123: np.uint16, 5125: np.uint32, 5126: np.float32}
WIDTH = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4}


def load_gltf(path):
    """All primitives of the (single, untransformed) mesh as one position array and one triangle array."""
    g = json.loads(path.read_text(encoding="utf-8"))
    buf = (path.parent / g["buffers"][0]["uri"]).read_bytes()

    def acc(i):
        a = g["accessors"][i]
        bv = g["bufferViews"][a["bufferView"]]
        n, w, dt = a["count"], WIDTH[a["type"]], np.dtype(DTYPE[a["componentType"]])
        start, size = bv.get("byteOffset", 0) + a.get("byteOffset", 0), w * dt.itemsize
        stride = bv.get("byteStride") or size
        rows = np.frombuffer(buf, np.uint8, count=(n - 1) * stride + size, offset=start)
        rows = np.lib.stride_tricks.as_strided(rows, (n, size), (stride, 1))  # interleaved views are strided
        return np.ascontiguousarray(rows).view(dt).reshape(n, w)

    P, I, base = [], [], 0
    for m in g["meshes"]:
        for p in m["primitives"]:
            if p.get("mode", 4) != 4:
                continue
            pos = acc(p["attributes"]["POSITION"]).astype(np.float64)
            idx = acc(p["indices"]).reshape(-1, 3).astype(np.int64) if "indices" in p else np.arange(len(pos)).reshape(-1, 3)
            P.append(pos); I.append(idx + base); base += len(pos)
    return np.concatenate(P), np.concatenate(I)


def cluster(P, I, cell):
    """Vertex clustering: merge vertices in the same cell to their mean, drop collapsed and duplicate triangles."""
    q = np.floor(P / cell).astype(np.int64) + (1 << 19)
    _, inv = np.unique((q[:, 0] << 40) | (q[:, 1] << 20) | q[:, 2], return_inverse=True)
    inv = inv.ravel()
    n = inv.max() + 1
    V = np.zeros((n, 3)); np.add.at(V, inv, P); V /= np.bincount(inv, minlength=n)[:, None]
    T = inv[I]
    T = T[(T[:, 0] != T[:, 1]) & (T[:, 1] != T[:, 2]) & (T[:, 0] != T[:, 2])]
    _, first = np.unique(np.sort(T, 1), axis=0, return_index=True)
    T = T[np.sort(first)]
    used, T = np.unique(T, return_inverse=True)
    return V[used], T.reshape(-1, 3)


def simplify(P, I, max_tris):
    V, T, cell = P, I, 0.012
    while len(T) > max_tris:
        V, T = cluster(P, I, cell)
        cell *= 1.25
    return V, T


def stand_in(model, names):
    """A variant of the same vehicle: the name parts but the last must match (at least two), e.g. t_34_1941 ->
    t_34_1942, pzkpfw_iv_ausf_g -> pzkpfw_iv_ausf_h; is_1 gets none (it_1 is another vehicle). Closest name wins."""
    parts = model.split("_")
    need = max(2, len(parts) - 1)
    ok = [n for n in names if n.split("_")[:need] == parts[:need]]
    return max(ok, key=lambda n: difflib.SequenceMatcher(None, model, n).ratio(), default=None)


def encode(V, T):
    V = V - [0, V[:, 1].min(), 0]
    q = np.clip(np.round(V * 1000), -32768, 32767).astype("<i2")
    assert len(V) < 65536, "too many vertices for uint16 indices"
    body = b"WTVK" + struct.pack("<III", 1, len(V), len(T)) + q.tobytes() + T.astype("<u2").tobytes()
    return base64.b64encode(zlib.compress(body, 9)).decode("ascii")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--tanks", default=str(ROOT / "tanks"))
    ap.add_argument("--out", default=str(ROOT / "data" / "tanks"))
    ap.add_argument("--tris", type=int, default=12000, help="triangle budget per model")
    ap.add_argument("--force", action="store_true", help="rewrite files that already exist")
    a = ap.parse_args()

    s = (ROOT / "data" / "index.js").read_text(encoding="utf-8")
    index = json.loads(s[len("WTV_INDEX("): s.rindex(")")])
    wanted = sorted({t["model"] for t in index["tanks"] if t.get("model")} | {TARGET_B})
    files = {p.stem.lower(): p for p in Path(a.tanks).glob("*/*.gltf")}
    out = Path(a.out); out.mkdir(parents=True, exist_ok=True)
    done = substituted = missing = 0
    for i, model in enumerate(wanted, 1):
        src = model if model in files else stand_in(model, files)
        if not src:
            missing += 1
            print(f"[{i}/{len(wanted)}] {model}: no model", file=sys.stderr)
            continue
        dst = out / f"{model}.js"
        if dst.exists() and not a.force:
            continue
        P, I = load_gltf(files[src])
        V, T = simplify(P, I, a.tris)
        dst.write_text(f'WTV_TANK({json.dumps(model)},{json.dumps(encode(V, T))});\n', encoding="ascii")
        done += 1
        substituted += src != model
        print(f"[{i}/{len(wanted)}] {model}{' <- ' + src if src != model else ''}: {len(I)} -> {len(T)} triangles, {dst.stat().st_size // 1024} KB", flush=True)
    print(f"wrote {done} models ({substituted} stand-ins), {missing} without a model")


if __name__ == "__main__":
    main()
