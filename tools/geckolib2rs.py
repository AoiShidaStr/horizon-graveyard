#!/usr/bin/env python3
"""Translate upstream's GeckoLib models and animations into Rust tables.

Part of the Horizon port of The Graveyard by finallion (GPL-3.0-only); the
generated file is a translation of upstream's GPL `geo.json` and
`animation.json` files (see NOTICE.md).

    python -I tools/geckolib2rs.py <upstream checkout>

Writes mixins/graveyard/api/src/geo_data.rs. Horizon has no GeckoLib or
Bedrock geometry loader, so each model becomes a list of bones (pivot,
rotation, parent) whose cubes are baked here into quads, and each animation
clip a list of keyframed channels.

Frames. A Bedrock model is y-up with its feet at y = 0, front towards -z, and
GeckoLib draws it with x negated. A vanilla `ModelPart` is y-down with its
feet at y = 24, front towards -z (docs/native-mods.md, "Mobs drawn by a mod").
Working the two renderers through gives, in texels:

    vanilla = (x, 24 - y, z)            for a Bedrock point (x, y, z)
    rotation: the same degrees, as radians, applied z then y then x
    animated rotation: added to the bone's, same signs
    animated position: (x, -y, z) added to the part's place

Box UV follows vanilla's `Cube` texture net, which is Bedrock's box UV (face
`east` is vanilla's west face, `up` its down face: vanilla's y points down).
Per-face UV puts each face's rectangle where vanilla's face of that box would
take it.
"""
import json
import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
OUT = os.path.join(ROOT, "mixins", "graveyard", "api", "src", "geo_data.rs")

# Model, animation file, and the clips the port plays.
MOBS = {
    "ghoul": ("ghoul", "ghoul/ghoul", {
        "idle": "animation.ghoul.idle", "walk": "animation.ghoul.walk",
        "run": "animation.ghoul.run_biped", "crawl": "animation.ghoul.run_crawl",
        "attack": "animation.ghoul.attack", "death": "animation.ghoul.death",
        "spawn": "animation.ghoul.spawn", "rage": "animation.ghoul.rage"}),
    "revenant": ("revenant", "revenant/revenant", {
        "idle": "idle", "walk": "walk", "run": "running", "attack": "attack",
        "death": "death"}),
    "nightmare": ("nightmare", "nightmare/nightmare", {
        "idle": "idle", "walk": "walk", "attack": "attack", "death": "death"}),
}
CLIPS = ["idle", "walk", "run", "crawl", "attack", "death", "spawn", "rage"]

# Java face order and corners (ModelPart.Cube): down, up, west, north, east, south.
JAVA_FACES = [
    ("down", [5, 4, 0, 1], (0, -1, 0)),
    ("up", [2, 3, 7, 6], (0, 1, 0)),
    ("west", [0, 4, 7, 3], (-1, 0, 0)),
    ("north", [1, 0, 3, 2], (0, 0, -1)),
    ("east", [5, 1, 2, 6], (1, 0, 0)),
    ("south", [4, 5, 6, 7], (0, 0, 1)),
]
# Bedrock face name -> the vanilla face that takes its texture.
BEDROCK_TO_JAVA = {"up": "down", "down": "up", "east": "west", "west": "east",
                   "north": "north", "south": "south"}


def die(msg):
    raise SystemExit("geckolib2rs.py: " + msg)


def rot_matrix(deg):
    """Rz * Ry * Rx for rotations in degrees (vanilla's rotationZYX)."""
    x, y, z = (math.radians(a) for a in deg)
    cx, sx, cy, sy, cz, sz = math.cos(x), math.sin(x), math.cos(y), math.sin(y), math.cos(z), math.sin(z)
    rx = [[1, 0, 0], [0, cx, -sx], [0, sx, cx]]
    ry = [[cy, 0, sy], [0, 1, 0], [-sy, 0, cy]]
    rz = [[cz, -sz, 0], [sz, cz, 0], [0, 0, 1]]

    def mul(a, b):
        return [[sum(a[i][k] * b[k][j] for k in range(3)) for j in range(3)] for i in range(3)]
    return mul(mul(rz, ry), rx)


def apply(m, v):
    return [sum(m[i][k] * v[k] for k in range(3)) for i in range(3)]


def to_model(p):
    return [p[0], 24.0 - p[1], p[2]]


def face_rects(cube):
    """Bedrock face name -> (u0, v0, u1, v1) in texels, for this cube."""
    uv = cube.get("uv", [0, 0])
    if isinstance(uv, dict):
        out = {}
        for face, f in uv.items():
            u, v = f["uv"]
            du, dv = f.get("uv_size", [0, 0])
            out[face] = (u, v, u + du, v + dv)
        return out
    u, v = uv
    w, h, d = (math.floor(s) for s in cube["size"])
    return {
        "up": (u + d, v, u + d + w, v + d),
        "down": (u + d + w, v + d, u + d + w + w, v),
        "east": (u, v + d, u + d, v + d + h),
        "north": (u + d, v + d, u + d + w, v + d + h),
        "west": (u + d + w, v + d, u + d + w + d, v + d + h),
        "south": (u + d + w + d, v + d, u + d + w + d + w, v + d + h),
    }


def cube_quads(cube, bone_mirror, tex_w, tex_h):
    """The cube's faces as quads in model space (texels), UVs normalized."""
    ox, oy, oz = cube["origin"]
    sx, sy, sz = cube["size"]
    g = cube.get("inflate", 0.0)
    x0, x1 = ox - g, ox + sx + g
    y0, y1 = 24.0 - (oy + sy) - g, 24.0 - oy + g
    z0, z1 = oz - g, oz + sz + g
    box_uv = not isinstance(cube.get("uv"), dict)
    mirror = cube.get("mirror", bone_mirror) and box_uv
    if mirror:
        x0, x1 = x1, x0
    corners = [
        [x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0],
        [x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1],
    ]
    if cube.get("rotation"):
        pivot = to_model(cube.get("pivot", [0, 0, 0]))
        m = rot_matrix(cube["rotation"])
        corners = [[a + b for a, b in zip(apply(m, [c[i] - pivot[i] for i in range(3)]), pivot)]
                   for c in corners]
    rects = face_rects(cube)
    java_rect = {BEDROCK_TO_JAVA[k]: v for k, v in rects.items()}
    quads = []
    for name, idx, _normal in JAVA_FACES:
        if name not in java_rect:
            continue
        ua, va, ub, vb = java_rect[name]
        if ua == ub or va == vb:
            # A face with no texture area (a zero-width face) draws nothing.
            if not box_uv:
                continue
        uv = [(ub, va), (ua, va), (ua, vb), (ub, vb)]
        verts = [corners[i] for i in idx]
        uvs = [(u / tex_w, v / tex_h) for u, v in uv]
        if mirror:
            verts.reverse()
            uvs.reverse()
        quads.append((verts, uvs))
    return quads


def build_model(path):
    geo = json.load(open(path, encoding="utf-8"))["minecraft:geometry"][0]
    desc = geo["description"]
    tw, th = desc["texture_width"], desc["texture_height"]
    bones = geo["bones"]
    index = {b["name"]: i for i, b in enumerate(bones)}
    out = []
    for b in bones:
        parent = index.get(b.get("parent"), -1)
        pivot = to_model(b.get("pivot", [0, 0, 0]))
        if parent >= 0:
            pp = to_model(bones[parent].get("pivot", [0, 0, 0]))
            offset = [pivot[i] - pp[i] for i in range(3)]
        else:
            offset = pivot
        quads = []
        for c in b.get("cubes", []):
            for verts, uvs in cube_quads(c, b.get("mirror", False), tw, th):
                local = [[v[i] - pivot[i] for i in range(3)] for v in verts]
                quads.append((local, uvs))
        rot = [math.radians(a) for a in b.get("rotation", [0, 0, 0])]
        out.append({"name": b["name"], "parent": parent, "offset": offset, "rot": rot,
                    "quads": quads})
    return tw, th, out


def vec(v):
    if isinstance(v, (int, float)):
        return [float(v)] * 3
    if isinstance(v, list):
        if any(isinstance(x, str) for x in v):
            die(f"Molang value {v} is not supported")
        return [float(x) for x in v]
    die(f"unexpected keyframe value {v!r}")


def channel(value):
    """[(time s, pre, post, catmull)] for a Bedrock channel."""
    if not isinstance(value, dict) or "vector" in value:
        v = vec(value["vector"] if isinstance(value, dict) else value)
        return [(0.0, v, v, False)]
    frames = []
    for t, kf in sorted(value.items(), key=lambda kv: float(kv[0])):
        if isinstance(kf, dict):
            if "vector" in kf:
                pre = post = vec(kf["vector"])
            else:
                pre = vec(kf.get("pre", kf.get("post")))
                post = vec(kf.get("post", kf.get("pre")))
            cat = kf.get("lerp_mode") == "catmullrom"
        else:
            pre = post = vec(kf)
            cat = False
        frames.append((float(t), pre, post, cat))
    return frames


def f(x):
    s = f"{x:.4f}".rstrip("0").rstrip(".")
    if s in ("-0", ""):
        s = "0"
    return s + ("" if "." in s else ".0")


def v3(v):
    return "[" + ", ".join(f(x) for x in v) + "]"


def main():
    if len(sys.argv) != 2:
        die("usage: python -I tools/geckolib2rs.py <upstream checkout>")
    gl = os.path.join(sys.argv[1], "common", "src", "main", "resources", "assets",
                      "graveyard", "geckolib")
    lines = [
        "//! Generated by tools/geckolib2rs.py from The Graveyard's GeckoLib",
        "//! models and animations (finallion, GPL-3.0-only): see NOTICE.md.",
        "//! Do not edit; run the tool again.",
        "#![allow(clippy::all)]",
        "use crate::geo::{Bone, Channel, Clip, Geo, Key, Quad};",
        "",
    ]
    for mob, (model, anim, clips) in MOBS.items():
        tw, th, bones = build_model(os.path.join(gl, "models", model + ".geo.json"))
        names = [b["name"] for b in bones]
        up = mob.upper()
        for i, b in enumerate(bones):
            lines.append(f"static {up}_Q{i}: &[Quad] = &[")
            for verts, uvs in b["quads"]:
                lines.append("    Quad { pos: [" + ", ".join(v3(v) for v in verts) + "], uv: ["
                             + ", ".join(f"[{f(u)}, {f(v)}]" for u, v in uvs) + "] },")
            lines.append("];")
        lines.append(f"static {up}_BONES: &[Bone] = &[")
        for i, b in enumerate(bones):
            lines.append(f"    Bone {{ name: {json.dumps(b['name'])}, parent: {b['parent']}, "
                         f"offset: {v3(b['offset'])}, rot: {v3(b['rot'])}, quads: {up}_Q{i} }},")
        lines.append("];")

        data = json.load(open(os.path.join(gl, "animations", anim + ".animation.json"),
                              encoding="utf-8"))["animations"]
        clip_refs = []
        for clip in CLIPS:
            if clip not in clips:
                clip_refs.append("None")
                continue
            a = data[clips[clip]]
            chans = []
            for bone, ch in a.get("bones", {}).items():
                if bone not in names:
                    continue
                for kind, k in (("rotation", 0), ("position", 1), ("scale", 2)):
                    if kind in ch:
                        chans.append((names.index(bone), k, channel(ch[kind])))
            cname = f"{up}_{clip.upper()}"
            for n, (_, _, keys) in enumerate(chans):
                lines.append(f"static {cname}_K{n}: &[Key] = &[" + ", ".join(
                    f"Key {{ t: {f(t)}, pre: {v3(pre)}, post: {v3(post)}, catmull: {str(cat).lower()} }}"
                    for t, pre, post, cat in keys) + "];")
            lines.append(f"static {cname}: Clip = Clip {{ length: {f(float(a.get('animation_length', 0.0)))}, "
                         f"looping: {str(a.get('loop') is True).lower()}, channels: &[")
            for n, (bone, kind, _) in enumerate(chans):
                lines.append(f"    Channel {{ bone: {bone}, kind: {kind}, keys: {cname}_K{n} }},")
            lines.append("] };")
            clip_refs.append(f"Some(&{cname})")
        lines.append(f"pub static {up}: Geo = Geo {{ texture: [{tw}, {th}], bones: {up}_BONES, clips: [")
        for clip, ref in zip(CLIPS, clip_refs):
            lines.append(f"    {ref}, // {clip}")
        lines.append("] };")
        lines.append("")
        nq = sum(len(b["quads"]) for b in bones)
        print(f"{mob}: {len(bones)} bones, {nq} quads, clips "
              + ", ".join(c for c, r in zip(CLIPS, clip_refs) if r != "None"))
    with open(OUT, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("\n".join(lines))
    print(f"wrote {os.path.relpath(OUT, ROOT)} ({os.path.getsize(OUT)} bytes)")


if __name__ == "__main__":
    main()
